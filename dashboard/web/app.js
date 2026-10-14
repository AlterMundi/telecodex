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
  pendingNonce,
  pendingKind,
  controlIntent;
const acknowledgmentRequests = new Map();
let resolvedHistoryExpanded = false;
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
    ["stop", "resume"].includes(route) ? 30000 : 20000,
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
function controlButton(op, stale, count) {
  if (op.state === "stopped" && pendingKind === "stop") {
    pendingNonce = undefined;
    pendingKind = undefined;
  }
  if (
    ["resumed", "resumed_with_failures", "released", "superseded"].includes(
      op.state,
    )
  ) {
    pendingNonce = undefined;
    pendingKind = undefined;
  }
  const phase = op.state;
  $("#stop").textContent = phase === "stopped" ? "Resume owned sessions"
    : phase === "resuming" ? "Resuming…"
    : phase === "stopping" ? "Stopping…" : "Stop owned sessions";
  $("#stop").disabled = stale || !count || pendingNonce !== undefined
    || !["no_operation", "stopped", "resumed", "resumed_with_failures", "released", "superseded"].includes(phase)
    || (phase === "stopped" && !op.operation);
}
function alerts(data) {
  const section = $("#operations");
  if (!data || data.state === "not_configured") { section.hidden = true; return; }
  section.hidden = false;
  const open = (data.incidents || []).filter((i) => i.projection.processing_state === "open").length;
  const unacknowledged = [...(data.incidents || []), ...(data.resolved_incidents || [])]
    .filter((i) => i.projection.processing_state === "open" && !i.acknowledgments.length).length;
  section.dataset.attention = String(unacknowledged > 0);
  $("#operations-summary").textContent = `${unacknowledged ? "⚠ " : ""}Operational alerts · ${open} open`
    + (unacknowledged ? ` · ${unacknowledged} unacknowledged` : "");
  $("#alerts-state").textContent = data.state === "unavailable"
    ? "Alert service unavailable. Processing and delivery cannot be confirmed."
    : data.state === "stale" ? "Showing an older observation. Refresh before acting."
    : data.observer_state === "unobserved" ? "Worker observation has not been established."
    : data.observer_state === "degraded" ? "Observation requires attention. Check source and observer freshness."
    : "Observation route current. Processing and delivery are independent.";
  const explanations = {
    unauthorized: "Alert service credential needs attention.",
    forbidden: "Alert service grant needs attention.",
    credential_binding_changed: "Alert credentials changed; operator requalification is required.",
    receiving_bot_unconfirmed: "The receiving bot has not been verified.",
    local_capacity_exceeded: "The delivery journal is full; reconcile retained attempts.",
  };
  if (data.state === "unavailable" && explanations[data.error])
    $("#alerts-state").textContent += " " + explanations[data.error];
  const target = $("#incidents"); target.replaceChildren();
  target.append(el("h3", `Open incidents · ${open}`, "incident-group-title"));
  const resolvedItems = [...(data.incidents || []), ...(data.resolved_incidents || [])]
    .filter((i) => i.projection.processing_state === "resolved");
  const history = el("details", undefined, "incident-history");
  history.open = resolvedHistoryExpanded;
  history.addEventListener("toggle", () => { resolvedHistoryExpanded = history.open; });
  const unseenHistory = resolvedItems.filter((i) => !i.acknowledgments.length).length;
  history.append(el("summary", `Resolved history · ${resolvedItems.length}`
    + (unseenHistory ? ` · ${unseenHistory} unacknowledged` : "")));
  for (const incident of [...(data.incidents || []), ...(data.resolved_incidents || [])]) {
    const p = incident.projection;
    const resolved = p.processing_state === "resolved";
    const acknowledged = incident.acknowledgments.length > 0;
    const item = el("article", undefined, `incident incident--${resolved ? "resolved" : "open"}`);
    item.dataset.processingState = p.processing_state;
    item.dataset.acknowledgment = acknowledged ? "acknowledged" : "unacknowledged";
    const badges = el("div", undefined, "incident-status");
    badges.append(el("span", resolved ? "✓ Resolved" : "! Open", `incident-badge incident-badge--${resolved ? "resolved" : "open"}`),
      el("span", acknowledged ? "Acknowledged" : "Unacknowledged", "incident-badge"));
    item.append(badges, el("h3", `Phase: ${p.phase}`),
      el("p", p.summary), el("p", p.processing_state === "resolved" ? `Previous action: ${p.action}` : p.action));
    const deliveries = incident.deliveries.map((d) => d.state).join(", ") || "No receiving route configured";
    item.append(el("p", `Observation ${incident.freshness} · Delivery: ${deliveries}`, "note"));
    if (incident.acknowledgments.length) item.append(el("p", "Acknowledged. Producer recovery remains separate.", "note"));
    else {
      const button = el("button", "Acknowledge");
      const requestKey = `${p.producer_id}/${p.incident_id}`;
      button.disabled = data.state !== "available";
      button.addEventListener("click", async () => {
        if (!acknowledgmentRequests.has(requestKey)) acknowledgmentRequests.set(requestKey, crypto.randomUUID());
        const requestId = acknowledgmentRequests.get(requestKey); button.disabled = true;
        try {
          const acknowledgment = await api("alerts/acknowledge", {producer_id:p.producer_id, incident_id:p.incident_id, request_id:requestId});
          button.textContent = "Acknowledged";
          // Reflect confirmed acceptance immediately; polling supplies durable state.
          const accepted = (items) => (items || []).map((i) => i === incident
            ? {...i, acknowledgments:[acknowledgment]} : i);
          alerts({...data, incidents:accepted(data.incidents), resolved_incidents:accepted(data.resolved_incidents)});
        } catch (e) { $("#alerts-state").textContent = e.message; button.disabled = false; }
      });
      item.append(button);
    }
    (resolved ? history : target).append(item);
  }
  if (!data.incidents?.length && data.state === "available") target.append(el("p", "No open incidents reported in this receiving scope."));
  if (data.truncated) target.append(el("p", "Partial view: more incidents exist in this receiving scope.", "note"));
  if (data.history_truncated) history.append(el("p", "Resolved history is partial; open incidents are listed separately.", "note"));
  if (resolvedItems.length || data.history_truncated) target.append(history);
  const uncertain = (data.local_deliveries || []).filter((d) =>
    ["uncertain", "failed"].includes(d.outcome) || d.state === "claim_unconfirmed").length;
  if (uncertain) target.append(el("p", `${uncertain} recent delivery attempt(s) failed or unconfirmed. No automatic resend.`, "note"));
}
function render() {
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
  controlButton(snapshot.stop, stale, snapshot.sessions.length);
  receipt(snapshot.stop);
}
async function refresh(force = false) {
  if (refreshing || !authenticated || (document.hidden && !force)) return;
  refreshing = true;
  $("#refresh").disabled = true;
  if (!snapshot) $("#overview").textContent = "Reading native state…";
  // This request remains independent of native Codex availability.
  const operations = api("alerts").then((data) => {
    csrf = data.csrf;
    alerts(data);
  }).catch(() => {
    $("#operations").hidden = false;
    $("#operations-summary").textContent = "Operational alerts · unavailable";
    $("#alerts-state").textContent = "Alert observation unavailable. Processing and delivery cannot be confirmed.";
    for (const button of document.querySelectorAll("#incidents button")) button.disabled = true;
  });
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
    await operations;
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
$("#stop").addEventListener("click", () => {
  if (!snapshot || $("#stop").disabled) return;
  const resume = snapshot.stop.state === "stopped";
  controlIntent = resume ? { kind: "resume", operation: snapshot.stop.operation } : { kind: "stop" };
  $("#confirm-title").textContent = resume ? "Resume these sessions?" : "Stop these sessions?";
  $("#approve").textContent = resume ? "Resume" : "Stop";
  $("#confirm-detail").textContent = resume
    ? "Continue formerly active work from retained history. Previously paused or blocked goals stay unchanged. Subagent recovery is delegated to the resumed parent."
    : "Previously paused or blocked goals stay unchanged. Work remains in its native history.";
  $("#confirm").showModal();
});
$("#cancel").addEventListener("click", () => {
  controlIntent = undefined;
  $("#confirm").close();
});
$("#approve").addEventListener("click", async () => {
  $("#confirm").close();
  if (!controlIntent) return;
  const intent = controlIntent;
  controlIntent = undefined;
  $("#stop").disabled = true;
  pendingNonce = crypto.randomUUID();
  pendingKind = intent.kind;
  let failure;
  try {
    const body = { nonce: pendingNonce };
    if (intent.kind === "resume") body.operation = intent.operation;
    const op = await api(intent.kind, body);
    receipt(op);
    controlButton(op, false, snapshot?.sessions?.length || 0);
    $("#feedback").textContent =
      `${intent.kind === "resume" ? "Resume" : "Stop"} requested. Its observed progress will appear below.`;
  } catch (e) {
    failure = `${e.message}. Check Stop status before trying another action.`;
    $("#feedback").textContent = failure;
  } finally {
    await refresh(true);
    if (failure) $("#feedback").textContent = failure;
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
