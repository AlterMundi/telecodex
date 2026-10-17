"use strict";
window.Conversation = (() => {
  const $ = (selector) => document.querySelector(selector);
  const node = (tag, text, cls) => {
    const value = document.createElement(tag);
    if (text !== undefined) value.textContent = text;
    if (cls) value.className = cls;
    return value;
  };
  function mount({api, headers, setCsrf, modeChanged, telegram}) {
    const cards = new Map();
    const audio = $("#conversation-audio");
    audio.controls = false;
    audio.hidden = true;
    const toggle = node("button", "Play audio");
    toggle.type = "button";
    toggle.id = "conversation-play";
    audio.before(toggle);
    let enabled = false, ready = false, reading = false, binding, canListen = false;
    let newest = {delivery:0, native:0}, oldest = {delivery:0, native:0};
    let initialized = false, active, generation = 0, requestAbort, blobUrl;
    const audioState = (text) => { $("#conversation-audio-state").textContent = text; };
    const initialMode = () => new URL(location.href).searchParams.get("view") === "conversation"
      || new URLSearchParams(telegram?.initData || "").get("start_param") === "conversation";

    function stop(text = "Audio stopped.") {
      generation++;
      requestAbort?.abort(); requestAbort = undefined;
      active = undefined;
      audio.pause(); audio.muted = true;
      audio.removeAttribute("src"); audio.load();
      if (blobUrl) URL.revokeObjectURL(blobUrl);
      blobUrl = undefined;
      toggle.disabled = true;
      audioState(text);
      $("#conversation-player").hidden = true;
    }
    function pause() {
      if (!active) return;
      generation++;
      requestAbort?.abort(); requestAbort = undefined;
      active.generation = generation;
      active.auto = false;
      audio.pause(); audio.muted = true;
      toggle.textContent = "Play audio";
      toggle.disabled = !blobUrl;
      audioState("Paused. Your replies remain available.");
    }
    function setMode(value, initial = false, navigate = true) {
      enabled = value;
      document.body.classList.toggle("conversation-mode", enabled);
      $("#conversation").hidden = !enabled;
      $(".identity h1").textContent = enabled ? "Conversation" : "Sessions";
      document.title = `${enabled ? "Conversation" : "Sessions"} · Telecodex`;
      if (navigate) {
        const url = new URL(location.href);
        if (enabled) url.searchParams.set("view", "conversation");
        else url.searchParams.delete("view");
        history.replaceState(null, "", url);
      }
      try { telegram?.BackButton?.[enabled ? "show" : "hide"]?.(); } catch (_) {}
      if (!enabled) pause();
      modeChanged(enabled, initial);
    }
    function failed(error) {
      if (error.status === 401 || error.status === 403) {
        stop(); cards.clear(); initialized = false;
        $("#conversation-answers").replaceChildren();
        $("#conversation-results").replaceChildren();
      } else pause();
      $("#conversation-state").textContent = error.name === "TypeError" ? "Connection unavailable. Refresh to reconnect."
        : error.message || "Replies are unavailable. Refresh to continue.";
    }
    function part(source, offset) {
      const query = new URLSearchParams({source:source.id, revision:source.revision, offset:String(offset)});
      return api(`interaction/part?${query}`);
    }
    function upsert(source) {
      let card = cards.get(source.id);
      if (!card) {
        const container = $(source.kind === "integrator_answer" ? "#conversation-answers" : "#conversation-results");
        const box = node("article", undefined, "reply");
        const title = node("h3");
        const status = node("p", "", "reply-status");
        const body = node("div", source.preview, "reply-text");
        const actions = node("div", undefined, "reply-actions");
        const read = node("button", "Read reply"); read.type = "button";
        const listen = node("button", "Listen"); listen.type = "button";
        const error = node("p", "", "reply-error"); error.setAttribute("role", "status");
        actions.append(read, listen); box.append(title, status, body, actions, error);
        box.dataset.sequence = String(source.sequence);
        const following = [...container.children].find((item) => Number(item.dataset.sequence) < source.sequence);
        container.insertBefore(box, following || null);
        card = {source, box, title, status, body, read, listen, error, offset:0, expanded:false};
        cards.set(source.id, card);
        read.addEventListener("click", async () => {
          read.disabled = true; error.textContent = "";
          const selected = card.source;
          try {
            const result = await part(selected, card.offset);
            if (cards.get(selected.id) !== card) return;
            if (card.source.revision !== selected.revision) throw new Error("Reply changed. Refresh before continuing.");
            body.textContent = (card.expanded ? body.textContent : "") + result.text;
            card.expanded = true;
            card.offset = result.next_offset;
            read.hidden = card.offset === null;
            read.textContent = "Continue reading";
          } catch (e) { error.textContent = e.message; if (e.status === 401 || e.status === 403) failed(e); }
          finally { read.disabled = false; }
        });
        listen.addEventListener("click", () => listenTo(card.source));
      } else if (card.source.text_sha256 !== source.text_sha256) {
        card.body.textContent = source.preview;
        card.offset = 0; card.expanded = false; card.read.hidden = false;
        card.read.textContent = "Read reply";
      }
      if (active?.source.id === source.id && active.source.revision !== source.revision)
        stop("This reply changed. Refresh before listening.");
      card.source = source;
      card.title.textContent = source.kind === "integrator_answer" ? "Conversation reply" : source.label;
      card.status.textContent = source.kind === "integrator_answer" ? "Reply from this conversation."
        : source.correction_pending ? "A later correction is pending."
        : !source.native_binding_current ? "From an earlier session."
        : !source.current ? "Instruction coverage has not been confirmed."
        : source.completed ? "Session reports this request complete." : "Session progress; work may continue.";
      card.box.dataset.pending = String(!!source.correction_pending || source.native_binding_current === false);
      card.listen.hidden = !canListen;
    }
    async function page(cursor, direction, rememberHistory = true) {
      const value = await api(`interaction?${new URLSearchParams({...cursor, direction})}`);
      if (binding && value.binding !== binding) {
        stop(); cards.clear(); initialized = false;
        newest = {delivery:0,native:0}; oldest = {delivery:0,native:0};
        $("#conversation-answers").replaceChildren(); $("#conversation-results").replaceChildren();
      }
      binding = value.binding;
      setCsrf(value.csrf);
      canListen = value.can_listen;
      for (const source of value.items) upsert(source);
      for (const card of cards.values()) card.listen.hidden = !canListen;
      if (direction === "before" && rememberHistory) {
        oldest = value.cursor;
        $("#conversation-older").hidden = !value.more;
      }
      // Recent metadata may jump ahead of a backlog still being read forward.
      if (direction === "after" || (rememberHistory && !initialized)) {
        newest.delivery = Math.max(newest.delivery, value.newest.delivery);
        newest.native = Math.max(newest.native, value.newest.native);
      }
      return value;
    }
    async function refresh(force = false) {
      if (!enabled || !ready || reading || (document.hidden && !force)) return;
      reading = true;
      $("#conversation-refresh").disabled = true;
      $("#conversation-older").disabled = true;
      try {
        if (!initialized) {
          newest = {delivery:0,native:0};
          await page({delivery:0,native:0}, "before");
          initialized = true;
        } else {
          let more = true;
          for (let n = 0; n < 4 && more && enabled && !document.hidden; n++) {
            const value = await page(newest, "after");
            if (!initialized) { newest = {delivery:0,native:0}; await page(newest,"before"); initialized = true; break; }
            more = value.more;
          }
          if (enabled && !document.hidden) await page({delivery:0,native:0}, "before", false);
        }
        if (active && enabled && !document.hidden) {
          const selected = active, owned = generation;
          try { await part(selected.source, selected.offset); }
          catch (_) {
            if (active === selected && generation === owned)
              stop("This reply is no longer current. Refresh before listening.");
          }
        }
        $("#conversation-state").textContent = cards.size ? `${cards.size} replies available.`
          : "No replies yet. Work can continue while this view is closed.";
      } catch (e) { failed(e); }
      finally { reading = false; $("#conversation-refresh").disabled = false; $("#conversation-older").disabled = false; }
    }

    async function startPlayback() {
      const selected = active;
      if (!selected || !blobUrl || document.hidden || !enabled) return;
      const owned = selected.generation;
      const position = audio.currentTime;
      selected.verifying = true;
      audio.muted = true;
      // Unlock this element on a user gesture; source validation happens muted.
      try {
        const started = audio.play();
        const check = part(selected.source, selected.offset);
        await Promise.all([started, check]);
        if (active !== selected || generation !== owned) return;
        audio.pause(); audio.currentTime = position;
        selected.verifying = false;
        selected.auto = true;
        audio.muted = false;
        await audio.play();
        if (active !== selected || generation !== owned) return;
        toggle.textContent = "Pause audio";
        audioState("Playing this reply.");
      } catch (e) {
        if (active !== selected || generation !== owned) return;
        selected.verifying = false;
        audio.pause(); audio.muted = true;
        if (e.name === "NotAllowedError") {
          toggle.disabled = false; toggle.textContent = "Play audio";
          audioState("Audio ready. Press Play audio to listen.");
        } else stop("This reply changed or is unavailable. Refresh before listening.");
      }
    }
    async function listenTo(source, offset = 0) {
      stop();
      const owned = generation;
      active = {source, offset, generation:owned, auto:false};
      const selected = active;
      $("#conversation-player").hidden = false;
      $("#conversation-playing").textContent = source.kind === "integrator_answer" ? "Conversation reply" : source.label;
      audioState("Preparing audio…"); toggle.disabled = true;
      requestAbort = new AbortController();
      const controller = requestAbort;
      const deadline = setTimeout(() => controller.abort(), 145000);
      try {
        const response = await fetch("api/interaction/audio", {method:"POST",credentials:"same-origin",
          headers:headers(),signal:controller.signal,body:JSON.stringify({source:source.id,revision:source.revision,offset})});
        if (!response.ok) {
          const value = await response.json();
          const error = new Error(value.detail || "Audio unavailable. Continue reading or refresh.");
          error.status = response.status; throw error;
        }
        if (response.headers.get("x-presentation-source") !== source.id
            || response.headers.get("x-presentation-revision") !== source.revision
            || response.headers.get("x-presentation-offset") !== String(offset)) throw new Error("Audio source mismatch");
        const stream = response.body.getReader(); const chunks = []; let length = 0;
        while (true) {
          const {value, done} = await stream.read(); if (done) break;
          length += value.byteLength;
          if (length > 8 * 1024 * 1024) { await stream.cancel(); throw new Error("Audio exceeds playback limit"); }
          chunks.push(value);
        }
        const blob = new Blob(chunks, {type:"audio/wav"});
        const hash = [...new Uint8Array(await crypto.subtle.digest("SHA-256", await blob.arrayBuffer()))]
          .map((byte) => byte.toString(16).padStart(2,"0")).join("");
        if (hash !== response.headers.get("x-audio-sha256")) throw new Error("Audio integrity check failed");
        if (active !== selected || generation !== owned) return;
        const next = response.headers.get("x-presentation-next-offset");
        selected.next = next === "" ? null : Number(next);
        if (selected.next !== null && (!Number.isSafeInteger(selected.next) || selected.next <= offset)) throw new Error("Invalid audio continuation");
        blobUrl = URL.createObjectURL(blob);
        audio.src = blobUrl;
        toggle.disabled = false;
        await startPlayback();
      } catch (e) {
        if (active !== selected || generation !== owned) return;
        stop();
        if (e.status === 401 || e.status === 403) failed(e);
        $("#conversation-state").textContent = e.name === "AbortError" ? "Audio preparation ended. Your text replies remain available."
          : e.message || "Audio unavailable. Text replies remain available.";
      } finally { clearTimeout(deadline); if (requestAbort === controller) requestAbort = undefined; }
    }
    audio.addEventListener("ended", () => {
      if (!audio.ended || !active || active.verifying || !active.auto || document.hidden || !enabled) return;
      if (active.next !== null) listenTo(active.source, active.next);
      else { audioState("End of reply."); toggle.textContent = "Play audio again"; }
    });
    toggle.addEventListener("click", () => {
      if (audio.paused) { if (audio.ended) audio.currentTime = 0; startPlayback(); }
      else pause();
    });
    $("#conversation-stop").addEventListener("click", () => stop());
    $("#conversation-open").addEventListener("click", () => setMode(true));
    $("#conversation-back").addEventListener("click", () => setMode(false));
    $("#conversation-refresh").addEventListener("click", () => refresh(true));
    $("#conversation-older").addEventListener("click", async () => {
      if (reading) return;
      reading = true;
      $("#conversation-older").disabled = true;
      try { await page(oldest, "before"); }
      catch (e) { failed(e); }
      finally { reading = false; $("#conversation-older").disabled = false; }
    });
    document.addEventListener("visibilitychange", () => { if (document.hidden) pause(); });
    window.addEventListener("pagehide", () => stop());
    window.addEventListener("offline", () => {
      pause(); $("#conversation-state").textContent = "Offline. Replies already loaded remain readable.";
    });
    window.addEventListener("online", () => { if (enabled && ready && !document.hidden) refresh(true); });
    telegram?.onEvent?.("deactivated", pause);
    telegram?.BackButton?.onClick?.(() => setMode(false));
    setMode(initialMode(), true, false);
    return {refresh, pause, stop,
      authenticated() { ready = true; $("#conversation-open").disabled = false; }};
  }
  return {mount};
})();
