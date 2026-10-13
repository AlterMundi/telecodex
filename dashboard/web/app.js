"use strict";
const $ = (selector) => document.querySelector(selector);
const tg = window.Telegram?.WebApp;
let snapshot,
  csrf,
  filter = "attention",
  refreshing = false,
  timer,
  authenticated = false,
  sessionToken,
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
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort(),
    route === "stop" ? 30000 : 20000,
  );
  try {
    const headers = body
      ? { "Content-Type": "application/json", "X-Dashboard-CSRF": csrf || "" }
      : {};
    if (sessionToken && route !== "session")
      headers.Authorization = `Bearer ${sessionToken}`;
    const response = await fetch(`api/${route}`, {
      signal: controller.signal,
      method: body ? "POST" : "GET",
      credentials: "same-origin",
      headers,
      body: body ? JSON.stringify(body) : undefined,
    });
    const value = await response.json();
    if (!response.ok) throw new Error(value.detail || "Request unavailable");
    return value;
  } catch (e) {
    if (controller.signal.aborted)
      throw new Error(route === "session"
        ? "Connection timed out. Close and reopen Sessions"
        : "Request timed out. Refresh to check current state");
    throw e;
  } finally {
    clearTimeout(timeout);
  }
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
  const ram =
    data.memory_total && data.memory_available !== undefined
      ? `${Math.round(100 * (1 - data.memory_available / data.memory_total))}%`
      : "?";
  const cpu = data.cpu_percent == null ? "Sampling…" : `${data.cpu_percent}%`;
  const disks = (data.storage || [])
    .map((d) => `${d.label} ${d.unavailable ? "?" : bytes(d.available)} free`)
    .join(" · ");
  const rate = (n) => (n == null ? "?" : `${bytes(n)}/s`);
  $("#host-summary").textContent =
    `CPU ${cpu} · RAM ${ram} · ${disks} · ↓ ${rate(data.network_rx_per_second)} ↑ ${rate(data.network_tx_per_second)}`;
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
  const position = target.scrollTop;
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
        ["unknown", "systemError"].includes(s.status) ||
        s.bridge_busy ||
        s.goal_state === "known" ||
        s.children.some((c) => c.status === "active"),
    );
  $("#count").textContent = `${rows.length} / ${snapshot.sessions.length}`;
  document.querySelector('[data-filter="all"]').textContent =
    `All (${snapshot.sessions.length})`;
  if (!rows.length)
    target.append(
      el(
        "p",
        "No active sessions or retained goals. Choose All to see idle conversations.",
      ),
    );
  for (const s of rows) {
    const card = el(
      "details",
      undefined,
      `session ${s.status === "active" ? "active" : ""}`,
    );
    card.dataset.session = s.id;
    card.open = expanded.has(s.id);
    const head = el("summary", undefined, "session-head");
    head.append(
      el("span", s.title, "session-title"),
      el(
        "span",
        [statuses[s.status] || s.status, elapsed(s.elapsed_seconds)]
          .filter(Boolean)
          .join(" · "),
        "badge",
      ),
    );
    head.title = s.title;
    if (s.goal) head.append(el("span", `Goal · ${s.goal.status}`, "goal-chip"));
    const activeChildren = s.children.filter(
      (child) => child.status === "active",
    ).length;
    if (activeChildren)
      head.append(el("span", `${activeChildren} active`, "child-chip"));
    card.append(head);
    card.append(el("h3", s.title, "full-title"));
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
      detail.dataset.session = `${s.id}:children`;
      detail.open = expanded.has(`${s.id}:children`);
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
  target.scrollTop = position;
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
    `${stale ? "Stale · " : ""}${new Date(snapshot.observed_at * 1000).toLocaleTimeString("es-AR", { timeZone: "America/Argentina/Buenos_Aires", hour: "2-digit", minute: "2-digit", hourCycle: "h23" })} AR`;
  $("#feedback").textContent = stale
    ? "Showing an older observation. Refresh to check current work."
    : "";
  const working = snapshot.sessions.filter((s) => s.status === "active").length;
  const goals = snapshot.sessions.filter(
    (s) => s.goal_state === "known",
  ).length;
  const children = snapshot.sessions
    .flatMap((s) => s.children)
    .filter((child) => child.status === "active").length;
  $("#overview").textContent =
    `${working} working · ${goals} goals · ${children} active subagents`;
  host(snapshot.host);
  sessions();
  $("#work").hidden = $("#control").hidden = false;
  const scope = `${snapshot.sessions.length} owned conversation(s) on ${snapshot.provider}. Filters do not change Stop scope.`;
  $("#scope").textContent = $("#confirm-scope").textContent = scope;
  $("#stop").disabled =
    stale || !snapshot.sessions.length || pendingNonce !== undefined;
  receipt(snapshot.stop);
}
async function refresh(force = false) {
  if (refreshing || !authenticated || (document.hidden && !force)) return;
  refreshing = true;
  $("#refresh").disabled = true;
  if (!snapshot) $("#overview").textContent = "Reading native state…";
  try {
    snapshot = await api("state");
    render();
  } catch (e) {
    $("#feedback").textContent = e.message;
    if (!snapshot) $("#overview").textContent = "State unavailable";
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
$("#refresh").addEventListener("click", () => refresh(true));
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
  // The selected control-panel design stays dark even in a light Telegram shell.
  document.documentElement.dataset.theme = "dark";
  for (const method of ["setHeaderColor", "setBackgroundColor", "setBottomBarColor"]) {
    try {
      tg?.[method]?.("#090e17");
    } catch (_) {
      // Older clients may expose an SDK method without supporting its bridge.
    }
  }
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
    const connection = await api("session", { init_data: tg.initData });
    // Older qualified backends still support the secure cookie transport.
    if (connection.session_token !== undefined) {
      if (!/^[A-Za-z0-9_-]{43}$/.test(connection.session_token))
        throw new Error("Invalid session response. Close and reopen Sessions");
      sessionToken = connection.session_token;
    }
    authenticated = true;
    // Some WebViews report hidden during launch and never emit visibilitychange.
    // Fetch the first observation once; periodic refresh still requires visibility.
    await refresh(true);
    schedule();
  } catch (e) {
    $("#feedback").textContent = e.message;
    $("#overview").textContent = "Connection unavailable";
    $("#refresh").disabled = true;
  }
})();
