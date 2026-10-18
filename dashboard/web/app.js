"use strict";
const $ = (selector) => document.querySelector(selector);
const monitorOnly = new URL(location.href).searchParams.get('view') === 'monitor';
const tg = monitorOnly ? undefined : window.Telegram?.WebApp;
let monitorAuthorization;
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
let conversationMode = false;
const acknowledgmentRequests = new Map();
let resolvedHistoryExpanded = false;
let alertTrailOpened = false;
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
function authorizationHeaders() {
  if(monitorOnly){
    if(!monitorAuthorization)throw new Error('Open a current monitor credential');
    return {Authorization:monitorAuthorization};
  }
  return sessionToken ? {Authorization:'Bearer '+sessionToken} : {};
}
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
    if(monitorOnly && body)throw new Error('This monitor is read-only');
    if(route!=="session")Object.assign(headers,authorizationHeaders());
    const response = await fetch(`api/${route}`, {
      signal: controller.signal,
      method: body ? "POST" : "GET",
      credentials: "same-origin",
      headers,
      body: body ? JSON.stringify(body) : undefined,
    });
    const value = await response.json();
    if (!response.ok) {
      const error = new Error(value.detail || "Request unavailable");
      error.status = response.status;
      if(value.code==="visual_reselection"&&value.submission_attempted===false)error.visualReselection=true;
      throw error;
    }
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
function metric(label, value, detail, target = "#metrics") {
  const box = el("div", undefined, "metric");
  box.append(
    el("div", label, "metric-label"),
    el("div", value, "metric-value"),
  );
  if (detail) box.append(el("div", detail, "metric-detail"));
  if (target) $(target).append(box);
  return box;
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
  if (!alertTrailOpened && unacknowledged > 0) { section.open = true; alertTrailOpened = true; }
  $("#operations-summary").textContent = `${data.attention?.symbol ? data.attention.symbol + " " : unacknowledged ? "⚠ " : ""}Operational alerts · ${open} open`
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
    // Legacy HMK producer IDs remain valid after the DMK project rename.
    const source = /^(hmk|dmk)-/.test(p.producer_id) ? "DMK (Memory)" : p.producer_id;
    const trailSymbol = !resolved && !acknowledged ? ({critical:"🔴",error:"🔴",warning:"🟠",info:"🔵"}[p.severity] || "") : "";
    item.append(badges, el("h3", `${trailSymbol ? trailSymbol + " " : ""}${source} · ${p.phase}`),
      el("p", `Reported by: ${p.producer_id} · Resource: ${p.resource_ref || "Unspecified"}`, "note"),
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
function traffic(data) {
  $("#traffic").hidden = data.state === "not_configured";
  if (data.state === "not_configured") return;
  const available = ["available", "degraded"].includes(data.state) && data.totals;
  if (!available) {
    $("#traffic-summary").textContent = `Bot traffic · ${data.state === "checking" ? "checking" : "unavailable"}`;
    $("#traffic-status").textContent = "Capture is not currently observable. Traffic and Telegram recovery cannot be confirmed.";
    $("#traffic").dataset.attention = "true";
    for (const id of ["traffic-metrics", "traffic-chart", "traffic-sources", "traffic-topics"]) $(`#${id}`).replaceChildren();
    $("#traffic-coverage").textContent = "No current statistics available.";
    return;
  }
  const cooldown = data.cooldown || {state:"none", methods:[]};
  const blocked = cooldown.state !== "none";
  $("#traffic").dataset.attention = String(blocked || !data.meter_healthy || data.dropped_records > 0 || (data.failed_methods || []).length > 0 || data.alerts_state === "unavailable");
  $("#traffic-summary").textContent = `Bot traffic · ${Math.round(data.requests_last_5m / 5)}/min (5m)${blocked ? " · Telegram limited" : ""}`;
  const until = cooldown.until ? new Date(cooldown.until * 1000).toLocaleString() : "not reported";
  $("#traffic-status").textContent = !data.meter_healthy ? "Collector unavailable; showing retained journal data."
    : cooldown.state === "active" ? `Telegram rejected ${cooldown.methods.join(", ")}. Retry deadline: ${until}.`
    : cooldown.state === "awaiting_recovery" ? `Waiting for successful ${cooldown.methods.join(", ")} after the last rejection. The retry deadline alone does not confirm recovery.`
    : "Capture active. No unrecovered rate limit observed in this capture.";
  if ((data.failed_methods || []).length) $("#traffic-status").textContent += ` Repeated API failures: ${data.failed_methods.join(", ")}.`;
  if (data.alerts_state === "unavailable" || data.alerts_state === "backlog")
    $("#traffic-status").textContent += " Alert publication is unconfirmed; retained incidents will be reconciled.";
  const trafficMetric = (label, value) => metric(label, String(value), undefined, null);
  $("#traffic-metrics").replaceChildren(trafficMetric("Attempts · 24h", data.totals.requests), trafficMetric("Telegram limits · 24h", data.totals.rate_limited),
    trafficMetric("Deferred locally · 24h", data.totals.deferred_local || 0),
    trafficMetric("Body volume · 24h", `${((data.totals.request_bytes + data.totals.response_bytes) / 1024).toFixed(1)} KiB`), trafficMetric("API failures · 24h", data.totals.failures || 0), trafficMetric("Dropped records", data.dropped_records));
  const byMinute = new Map(data.minutes.map((m) => [m.at, m]));
  const current = Math.floor(data.observed_at / 60) * 60;
  const minutes = Array.from({length:60}, (_, i) => byMinute.get(current - (59-i)*60) || {at:current-(59-i)*60,requests:0,rate_limited:0});
  const peak = Math.max(0, ...minutes.map((m) => m.requests));
  $("#traffic-chart").setAttribute("aria-label", `Requests per minute; peak ${peak}. ${data.totals.rate_limited} rate limits in 24 hours. Empty time before capture is unobserved.`);
  $("#traffic-chart").replaceChildren(...minutes.map((m) => {
    const bar = document.createElement("div");bar.className = "traffic-bar";bar.style.height = `${Math.max(1, m.requests / Math.max(1, peak) * 60)}px`;
    bar.dataset.limited = String(m.rate_limited > 0);bar.title = `${new Date(m.at*1000).toLocaleTimeString()} · ${m.at < data.capture_started-60 ? "unobserved" : m.requests+" requests"}`;return bar;
  }));
  const row = (name, value) => {
    const line = document.createElement("div"), label = document.createElement("span"), count = document.createElement("span");
    line.className = "traffic-row";label.textContent=name;count.textContent=value;line.append(label,count);return line;
  };
  $("#traffic-sources").replaceChildren(...data.sources.map((r) => row(`${r.source} · ${r.method}`, `${r.requests}${r.rate_limited ? " · "+r.rate_limited+" limited" : ""}`)));
  $("#traffic-topics").replaceChildren(...(data.topics.length ? data.topics.map((r) => row(r.name, `${r.requests}${r.rate_limited ? " · "+r.rate_limited+" limited" : ""}`)) : [row("No attributed conversations", "—")]));
  $("#traffic-coverage").textContent = `Capture since ${new Date(data.capture_started*1000).toLocaleString()}. Retained endpoint traffic only. Unknown origin: ${data.totals.unknown_source}; no topic: ${data.totals.unknown_topic}; incomplete metadata: ${data.totals.incomplete}. Message content is not recorded.`;
  if (data.alert_history_incomplete) $("#traffic-coverage").textContent += " Alert history exceeded its backlog bound; inspect retained traffic.";
}
async function refresh(force = false) {
  if (refreshing || !authenticated || (document.hidden && !force)) return;
  if (conversationMode) return conversationView?.refresh(force);
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
  const trafficRead = api("traffic").then(traffic).catch(() => traffic({state:"unavailable"}));
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
    await Promise.all([operations, trafficRead]);
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
  const background = typeof getComputedStyle === "function"
    ? getComputedStyle(document.documentElement).getPropertyValue("--bg").trim() : "#090e17";
  for (const method of ["setHeaderColor", "setBackgroundColor", "setBottomBarColor"]) {
    try {
      tg?.[method]?.(background || "#090e17");
    } catch (_) {
      // Older clients may expose an SDK method without supporting its bridge.
    }
  }
}
theme();
window.addEventListener?.("home-style:resolved", theme);
tg?.onEvent?.("themeChanged", theme);
tg?.onEvent?.("activated", () => {
  schedule();
  refresh();
});
tg?.onEvent?.("deactivated", () => clearInterval(timer));
tg?.ready?.();
const conversationView = window.Conversation?.mount({
  api,
  headers: () => ({"Content-Type":"application/json", "X-Dashboard-CSRF":csrf || "",...authorizationHeaders()}),
  setCsrf: (value) => { csrf = value; },
  modeChanged: (enabled, initial) => { conversationMode = enabled; if (!initial) refresh(true); },
  telegram: tg,
  readOnly: monitorOnly,
});
(async () => {
  if(monitorOnly){
    const feedback=$('#feedback'),label=el('label','Open a monitor credential '),file=document.createElement('input');
    file.type='file';file.accept='.json,application/json';file.setAttribute('aria-label','Monitor credential');label.append(file);feedback.replaceChildren(label);
    async function openCredential(value){
      if(value?.scope!=='monitor_readonly' || value.origin!==location.origin || value.prefix!==location.pathname.replace(/\/$/,'') || !/^Companion [A-Za-z0-9_-]{43}$/.test(value.authorization) || !(value.expires>Date.now()/1000))throw Error('Select a current monitor credential for this receiver');
      monitorAuthorization=value.authorization;
      const source=await api('interaction/inputs');
      const sourceLabel=source.viewer?.label || 'Selected receiver';
      document.title='Routing monitor · '+sourceLabel;
      sessionStorage.setItem('routing-monitor',JSON.stringify(value));
      authenticated=true;conversationView?.authenticated();feedback.replaceChildren(label,el('p','Source: '+sourceLabel),el('p','Read-only access until '+new Date((source.viewer?.expires || value.expires)*1000).toLocaleString()+'. Inputs, playback and session controls are disabled.'));await refresh(true);schedule();
    }
    file.addEventListener('change',async()=>{
      try{const selected=file.files?.[0];if(!selected || selected.size>4096)throw Error('Select the small operator-issued monitor credential');await openCredential(JSON.parse(await selected.text()));}
      catch(error){monitorAuthorization=undefined;authenticated=false;document.title='Routing monitor';sessionStorage.removeItem('routing-monitor');feedback.replaceChildren(label,el('p',error.message));}
      finally{file.value='';}
    });
    try{const saved=sessionStorage.getItem('routing-monitor');if(saved)await openCredential(JSON.parse(saved));}
    catch(_){monitorAuthorization=undefined;authenticated=false;document.title='Routing monitor';sessionStorage.removeItem('routing-monitor');}
    return;
  }
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
    $("#feedback").textContent = "";
    conversationView?.authenticated();
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
