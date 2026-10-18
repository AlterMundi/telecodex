#!/usr/bin/env python3
"""Authenticated conversation -> signed SDK -> canonical HTTP result; zero inference.

Run with the selected SDK source on PYTHONPATH and an existing dashboard Python.
The approved instruction fixtures and native App Server are synthetic. This does
not qualify a real receiving model, human account, participant or funding grant.
"""

import importlib.util
import json
import sys
from dataclasses import replace
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from conversation_cold_entry_http_io import Browser, ColdEntry

spec = importlib.util.spec_from_file_location(
    "adapter_fixtures", Path(__file__).with_name("matrix_execution.py")
)
adapter = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = adapter
spec.loader.exec_module(adapter)
from matrix_execution import make_conversation_task, make_received_task


def qualify(binary):
    sdk = adapter.AdapterTests()
    sdk.setUp()
    j = ColdEntry(str(Path(binary).resolve()))
    b = None
    try:
        j.prepare()
        j.stop()
        target = sdk.target
        configured = (
            'matrix_targets=[{name="SelectedPeer",destination={kind="body",being_ref='
            + json.dumps(target["being_ref"])
            + ",embodiment_id="
            + json.dumps(target["embodiment_id"])
            + "}}]"
        )
        j.config.write_text(
            j.config.read_text().replace(
                "[interaction]", "[interaction]\n" + configured
            )
        )
        j.start()
        j.wait_control_socket(j.socket)
        b = Browser(j)
        original = "  Pregunta exacta para el daimon seleccionado: ¿qué cambió?  "
        ident, _ = b.text(original)
        j.wait(lambda: b.status(ident)["phase"] == "accepted")
        selected = j.call(
            "route-matrix-original",
            "matrix_request",
            {
                "input_ref": ident,
                "selector": "SelectedPeer",
                "relation": "new",
                "requests": [],
            },
        )
        request = selected["request"]
        assert request["original"]["text"] == original
        assert b.status(ident)["phase"] == "matrix_requested"
        task = make_conversation_task(selected, sdk.scope)
        broker = sdk.make_broker(sdk.sender, sdk.scope, task=task)
        sent = broker.submit(sdk.scope, {"text": original}, 1)
        message = sdk.receiver.message(sent["message_id"])
        reply_scope = replace(sdk.scope, tool="messaging_reply", action="reply")
        receiving_task = make_received_task(message, reply_scope)
        responder = sdk.make_broker(sdk.receiver, reply_scope, task=receiving_task)
        returned = responder.submit(
            reply_scope, {"text": "La respuesta exacta del daimon seleccionado."}, 1
        )
        signed_answer = sdk.sender.message(returned["message_id"])
        callback = broker.conversation_return(sent, signed_answer)
        outcome = j.read("matrix", "return", json.dumps(callback))["result"]
        assert outcome["phase"] == "reply_received" and outcome["current"]
        j.read("matrix", "return", json.dumps(callback))
        assert j.row("SELECT count(*) FROM interaction_matrix_results") == [(1,)]
        page = b.get("/api/interaction?matrix=0")
        source = next(v for v in page["items"] if v["kind"] == "matrix_result")
        assert source["label"] == "#SelectedPeer" and source["completed"] is None
        part = b.get(
            "/api/interaction/part?source="
            + source["id"]
            + "&revision="
            + source["revision"]
        )
        assert part["text"] == callback["text"]
        after = b.get("/api/interaction?matrix=" + str(page["cursor"]["matrix"]))
        assert not any(v["kind"] == "matrix_result" for v in after["items"])
        # A contextual response carries the selected canonical event, not an
        # automatic peer send or a forged local worker request.
        next_id, _ = b.text(
            "Corregí el requisito anterior.",
            {
                "source": source["id"],
                "revision": source["revision"],
                "context_only": True,
            },
        )
        j.wait(lambda: b.status(next_id)["phase"] == "accepted")
        amended = j.call(
            "route-matrix-amendment",
            "matrix_request",
            {
                "input_ref": next_id,
                "selector": "SelectedPeer",
                "relation": "amends",
                "requests": [request["id"]],
            },
        )
        assert amended["request"]["parents"][0]["text"] == original
        current = b.get("/api/interaction?matrix=0")
        old = next(v for v in current["items"] if v["kind"] == "matrix_result")
        assert old["correction_pending"] and not old["current"]
        j.call(
            "route-matrix-no-local-reuse",
            "dispatch",
            {
                "input_ref": ident,
                "destinations": [
                    {"selector": "Alpha", "relation": "new", "requests": []}
                ],
            },
            ok=False,
        )
        assert j.row("SELECT count(*) FROM interaction_instructions") == [(0,)]
        assert not j.messages
        print(
            "PASS actual HTTP/native controller/SQLite original -> signed SDK request/reply -> owner callback -> canonical HTTP result, duplicate receipt, independent cursor, explicit correction context and local-plan exclusion; zero inference."
        )
    finally:
        if b:
            b.client.close()
        j.close()
        sdk.doCleanups()


if __name__ == "__main__":
    qualify(sys.argv[1])
