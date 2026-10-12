"use strict";
const $ = (selector) => document.querySelector(selector);
const tg = window.Telegram?.WebApp;
let snapshot,
  csrf,
  filter = "attention",
  refreshing = false,
  timer,
  authenticated = false,
  pendingNonce;
const statuses = {
  active: "Working",
  idle: "Idle",
  notLoaded: "Not loaded",
  unknown: "Unavailable",
  systemError: "Native error",
};
const el = (tag, text, cls) => {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (cls) node.className = cls;
  return node;
};
const bytes = (n) => {
  if (n === null || n === undefined) return "Unavailable";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (n >= 1024 && i < 4) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(i ? 1 : 0)} ${units[i]}`;
};
const elapsed = (n) =>
  n === null || n === undefined
    ? ""
    : n < 60
      ? `${n}s`
      : `${Math.floor(n / 60)}m ${n % 60}s`;
async function api(route, body) {
  const response = await fetch(`api/${route}`, {
    method: body ? "POST" : "GET",
    credentials: "same-origin",
    headers: body
      ? { "Content-Type": "application/json", "X-Dashboard-CSRF": csrf || "" }
      : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  const value = await response.json();
  if (!response.ok) throw new Error(value.detail || "Request unavailable");
  return value;
}
function metric(label, value, detail) {
  const box = el("div", undefined, "metric");
  box.append(
    el("div", label, "metric-label"),
    el("div", value, "metric-value"),
  );
  if (detail) box.append(el("div", detail, "metric-detail"));
  $("#metrics").append(box);
}
function host(data) {
  $("#metrics").replaceChildren();
  metric(
    "CPU",
    data.cpu_percent === null ? "Sampling…" : `${data.cpu_percent}%`,
    `${data.cpu_count || "?"} logical CPUs`,
  );
  metric(
    "Memory",
    data.memory_total
      ? bytes(data.memory_total - data.memory_available)
      : "Unavailable",
    `${bytes(data.memory_available)} available / ${bytes(data.memory_total)}`,
  );
  for (const disk of data.storage || [])
    metric(
      disk.label,
      disk.unavailable ? "Unavailable" : `${bytes(disk.available)} free`,
      disk.total ? `${bytes(disk.total)} total` : null,
    );
  metric(
    "Network ↓",
    data.network_rx_per_second === null
      ? "Sampling…"
      : `${bytes(data.network_rx_per_second)}/s`,
    "Aggregate interfaces",
  );
  metric(
    "Network ↑",
    data.network_tx_per_second === null
      ? "Sampling…"
      : `${bytes(data.network_tx_per_second)}/s`,
    "Includes virtual links",
  );
  $("#host").hidden = false;
}
function tree(children, parent) {
  const list = el("ul", undefined, "children");
  for (const child of children.filter((c) => c.parent_id === parent)) {
    const item = el(
      "li",
      `${child.title} · ${statuses[child.status] || child.status}`,
    );
    const nested = tree(children, child.id);
    if (nested.children.length) item.append(nested);
    list.append(item);
  }
  return list;
}
function sessions() {
  const target = $("#sessions");
  const expanded = new Set(
    [...target.querySelectorAll("details[open]")].map(
      (node) => node.dataset.session,
    ),
  );
  target.replaceChildren();
  const rows = [...snapshot.sessions]
    .sort(
      (a, b) =>
        Number(b.status === "active") - Number(a.status === "active") ||
        Number(b.goal_state === "known") - Number(a.goal_state === "known"),
    )
    .filter(
      (s) =>
        filter === "all" ||
        s.status === "active" ||
        s.bridge_busy ||
        s.goal_state === "known" ||
        s.children.some((c) => c.status === "active"),
    );
  $("#count").textContent = `${rows.length} / ${snapshot.sessions.length}`;
  if (!rows.length)
    target.append(
      el(
        "p",
        "No active sessions or retained goals. Choose All to see idle conversations.",
      ),
    );
  for (const s of rows) {
    const card = el(
      "article",
      undefined,
      `session ${s.status === "active" ? "active" : ""}`,
    );
    const head = el("div", undefined, "session-head");
    head.append(
      el("h3", s.title),
      el("span", statuses[s.status] || s.status, "badge"),
    );
    card.append(head);
    let meta = elapsed(s.elapsed_seconds);
    if (s.tools_active) meta += ` · ${s.tools_active} tool call(s)`;
    if (s.bridge_busy && s.status !== "active")
      meta += " · Bridge turn in progress";
    if (meta) card.append(el("p", meta, "session-meta"));
    if (s.goal) {
      card.append(
        el("div", `Goal · ${s.goal.status}`, "goal-label"),
        el("p", s.goal.objective, "session-goal"),
      );
    } else if (s.goal_state === "unavailable")
      card.append(el("p", "Goal metadata unavailable", "session-meta"));
    if (s.children.length) {
      const detail = el("details");
      detail.dataset.session = s.id;
      detail.open = expanded.has(s.id);
      detail.append(
        el(
          "summary",
          `${s.children.length} subagent(s) · ${s.children.filter((c) => c.status === "active").length} working`,
        ),
        tree(s.children, s.native_id),
      );
      if (s.children_state === "partial")
        detail.append(
          el(
            "p",
            "Partial tree · some children were not observed",
            "session-meta",
          ),
        );
      card.append(detail);
    } else if (s.children_state === "unavailable")
      card.append(el("p", "Subagent metadata unavailable", "session-meta"));
    if (s.topic_url) {
      const link = el(
        "a",
        s.topic_link_scope === "bot_chat"
          ? "Open bot chat"
          : "Open conversation",
      );
      link.href = s.topic_url;
      link.addEventListener("click", (e) => {
        if (tg?.openTelegramLink) {
          e.preventDefault();
          tg.openTelegramLink(link.href);
        }
      });
      card.append(link);
    }
    target.append(card);
  }
}
function receipt(op) {
  $("#receipt").replaceChildren();
  if (op.state === "no_operation") return;
  $("#receipt").append(el("p", `Stop · ${op.state}`));
  if (op.operation) $("#receipt").append(el("p", op.operation, "session-meta"));
  if (op.detail) $("#receipt").append(el("p", op.detail));
  const counts = {};
  for (const target of op.targets || [])
    counts[target.state] = (counts[target.state] || 0) + 1;
  for (const [state, count] of Object.entries(counts))
    $("#receipt").append(el("p", `${count} ${state}`));
}
function render() {
  if (
    ["resumed", "resumed_with_failures", "released", "superseded"].includes(
      snapshot.stop.state,
    )
  )
    pendingNonce = undefined;
  csrf = snapshot.csrf;
  $("#provider").textContent = snapshot.provider;
  const stale = Date.now() / 1000 - snapshot.observed_at > 30;
  $("#freshness").textContent =
    `${stale ? "Stale · " : ""}${new Date(snapshot.observed_at * 1000).toLocaleTimeString("es-AR", { timeZone: "America/Argentina/Buenos_Aires", hour: "2-digit", minute: "2-digit", second: "2-digit" })} AR`;
  $("#feedback").textContent = stale
    ? "Showing an older observation. Refresh to check current work."
    : "";
  host(snapshot.host);
  sessions();
  $("#work").hidden = $("#control").hidden = false;
  const scope = `${snapshot.sessions.length} owned conversation(s) on ${snapshot.provider}. Filters do not change Stop scope.`;
  $("#scope").textContent = $("#confirm-scope").textContent = scope;
  $("#stop").disabled =
    stale || !snapshot.sessions.length || pendingNonce !== undefined;
  receipt(snapshot.stop);
}
async function refresh() {
  if (refreshing || !authenticated || document.hidden) return;
  refreshing = true;
  $("#refresh").disabled = true;
  try {
    snapshot = await api("state");
    render();
  } catch (e) {
    $("#feedback").textContent = e.message;
    if (snapshot && Date.now() / 1000 - snapshot.observed_at > 30)
      $("#freshness").textContent = "Stale observation";
    $("#stop").disabled = true;
  } finally {
    refreshing = false;
    $("#refresh").disabled = false;
  }
}
function schedule() {
  clearInterval(timer);
  if (!document.hidden) timer = setInterval(refresh, 10000);
}
$("#refresh").addEventListener("click", refresh);
for (const button of document.querySelectorAll("[data-filter]"))
  button.addEventListener("click", () => {
    filter = button.dataset.filter;
    for (const peer of document.querySelectorAll("[data-filter]"))
      peer.setAttribute("aria-pressed", String(peer === button));
    if (snapshot) sessions();
  });
$("#stop").addEventListener("click", () => $("#confirm").showModal());
$("#cancel").addEventListener("click", () => $("#confirm").close());
$("#approve").addEventListener("click", async () => {
  $("#confirm").close();
  $("#stop").disabled = true;
  pendingNonce = crypto.randomUUID();
  try {
    const op = await api("stop", { nonce: pendingNonce });
    receipt(op);
    $("#feedback").textContent =
      "Stop requested. Its observed progress will appear below.";
  } catch (e) {
    $("#feedback").textContent =
      `${e.message}. Check Stop status before trying another action.`;
  } finally {
    await refresh();
  }
});
document.addEventListener("visibilitychange", () => {
  schedule();
  if (!document.hidden) refresh();
});
function theme() {
  if (tg) document.documentElement.dataset.theme = tg.colorScheme;
}
theme();
tg?.onEvent?.("themeChanged", theme);
tg?.onEvent?.("activated", () => {
  schedule();
  refresh();
});
tg?.onEvent?.("deactivated", () => clearInterval(timer));
tg?.ready?.();
(async () => {
  if (!tg?.initData) {
    $("#feedback").textContent =
      "Open Sessions from this bot’s Telegram menu to authenticate. Private data is not available in an ordinary browser.";
    $("#refresh").disabled = true;
    return;
  }
  try {
    await api("session", { init_data: tg.initData });
    authenticated = true;
    await refresh();
    schedule();
  } catch (e) {
    $("#feedback").textContent = e.message;
  }
})();
