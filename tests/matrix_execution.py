"""Zero-inference adapter tests against pinned Matrix signed/HTTP fixtures.

PYTHONPATH=<matrix>/src:<matrix> python3 -B tests/matrix_execution.py
Only synthetic keys, loopback transport and synthetic approval are used.
"""

import copy
from dataclasses import dataclass, replace
import hashlib
import json
from pathlib import Path
import sys
import time
from types import SimpleNamespace
import unittest
import uuid
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from matrix_execution import (
    MatrixExecutionBroker,
    MatrixExecutionError,
    canonical,
    make_task,
    parent_from_verified_sdk,
)
from daimon_matrix.execution_instruction import Instruction, Scope
from tests.test_execution_instruction import request
from tests import test_dm025_client as client_fixtures
from tests import test_execution_store as store_fixtures
from tests import test_group_messaging_runtime as group_fixtures
from tests import test_passive_messaging_execution as runner_fixtures


@dataclass(frozen=True)
class SignedGroupClient:
    """SDK group seam; not an AF_UNIX/HMAC client qualification."""

    fixture: object
    group: object
    timeout_seconds: float = 1
    deadline_ns: int | None = None

    @property
    def config(self):
        return SimpleNamespace(expected_server=self.group.ledger.local_origin)

    def invoke(self, method, params):
        assert self.deadline_ns > time.monotonic_ns()
        assert self.timeout_seconds <= 1
        if method == "runtime.status":
            assert params == {}
            return {}, {
                "ok": True,
                "result": {
                    "schema": "dm.runtime.status/v1",
                    "being_ref": self.group.lane.being_ref,
                    "local_origin": self.group.ledger.local_origin,
                },
            }
        params = dict(params)
        assert params.pop("channel_id") == "tribe"
        if method == "messaging.delivery":
            result = self.group.inspect(client_id="test", **params)
        else:
            if method == "messaging.reply":
                assert params.pop("received_channel_id") == "tribe"
                parent = self.group.message(params.pop("message_id"))
                params["thread_id"] = parent["payload"]["intent"]["thread_id"]
                params["response_to"] = (self.group, parent["event_id"])
            else:
                assert method == "messaging.send"
            result = self.fixture.send_and_settle(
                self.group, client_id="test", **params
            )
        return {}, {"ok": True, "result": result}


class AdapterTests(unittest.TestCase):
    def setUp(self):
        # Reuse maintained SDK signing, native journals, authority resolution,
        # visibility and real HTTP handlers. No private runtime is opened.
        self.fixture = group_fixtures.GroupRuntimeTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.sender, self.receiver = self.fixture.groups
        self.sender.direct_reader_ready = self.receiver.direct_reader_ready = (
            lambda row: True
        )
        self.scope = Scope("tribe", str(uuid.uuid4()), "messaging_send", "send")
        self.input_ref = str(uuid.uuid4())
        self.target = {
            "kind": "body",
            "being_ref": self.receiver.lane.being_ref,
            "embodiment_id": self.receiver.lane.local_embodiment_id,
        }
        self.broker = self.make_broker(self.sender, self.scope, target=self.target)

    def make_broker(
        self, group, scope, *, target=None, parent=None, revision=1, input_ref=None
    ):
        task = make_task(
            "One explicitly selected technical question.",
            input_ref or self.input_ref,
            scope,
            selected_destination=target,
            verified_parent=parent,
        )
        instruction = Instruction.from_dict(
            request(
                mode="manual",
                interval=None,
                max_cycles=1,
                being=group.lane.being_ref,
                embodiment=group.lane.local_embodiment_id,
                task_sha256=hashlib.sha256(task.encode()).hexdigest(),
                scope=[
                    {
                        k: getattr(scope, k)
                        for k in ("channel", "thread", "tool", "action")
                    }
                ],
            )
        )
        if revision != 1:
            instruction = replace(instruction, revision=revision, predecessor="a" * 64)
        return MatrixExecutionBroker(
            instruction, task, SignedGroupClient(self.fixture, group)
        )

    def send(self):
        return self.broker.submit(
            self.scope, {"text": "What changed in the synthetic task?"}, 1
        )

    def test_signed_request_and_exact_parent_return_preserve_provenance(self):
        first = self.send()
        self.assertTrue(first["semantic"]["terminal"])
        self.assertEqual(len(first["audience"]), 1)
        received = self.receiver.message(first["message_id"])
        relay = json.loads(received["payload"]["body"]["text"])
        self.assertEqual(relay["via"], "interaction-controller")
        self.assertEqual(relay["kind"], "session_request")
        self.assertEqual(received["being_ref"], self.sender.lane.being_ref)
        scope = replace(self.scope, tool="messaging_reply", action="reply")
        responder = self.make_broker(
            self.receiver, scope, parent=parent_from_verified_sdk(received)
        )
        returned = responder.submit(
            scope, {"text": "The synthetic task is complete."}, 1
        )
        self.assertTrue(returned["semantic"]["terminal"])
        answer = self.sender.message(returned["message_id"])
        self.assertEqual(
            self.broker.accept_verified_reply(first, answer),
            "The synthetic task is complete.",
        )
        self.assertEqual(
            returned["destination"]["being_ref"], self.sender.lane.being_ref
        )
        for field, value in [("being_ref", self.sender.lane.being_ref)]:
            wrong = copy.deepcopy(answer)
            wrong[field] = value
            with self.assertRaises(MatrixExecutionError):
                self.broker.accept_verified_reply(first, wrong)
        wrong = copy.deepcopy(answer)
        wrong["payload"]["body"]["response_context"]["message_hash"] = "0" * 64
        with self.assertRaises(MatrixExecutionError):
            self.broker.accept_verified_reply(first, wrong)

    def test_retained_identity_prevents_renewal_reroute_and_changed_content(self):
        first = self.send()
        self.assertEqual(self.broker.inspect()["message_id"], first["message_id"])
        renewed = self.make_broker(
            self.sender, self.scope, target=self.target, revision=2
        )
        self.assertEqual(renewed.send_id, self.broker.send_id)
        # Revision is in provenance. Native reservation rejects different bytes
        # for an already authored input instead of creating another send.
        with self.assertRaises(MatrixExecutionError):
            renewed.submit(self.scope, {"text": "Different approved revision"}, 1)
        alternate = self.make_broker(
            self.sender,
            self.scope,
            target={
                "kind": "body",
                "being_ref": self.sender.lane.being_ref,
                "embodiment_id": self.sender.lane.local_embodiment_id,
            },
        )
        self.assertEqual(alternate.send_id, self.broker.send_id)
        with self.assertRaises(MatrixExecutionError):
            alternate.submit(self.scope, {"text": "Reroute"}, 1)
        self.assertEqual(len(self.receiver.page(after=0, limit=10)), 1)

    def test_lost_response_remains_inspectable_without_another_send(self):
        actual = SignedGroupClient.invoke

        def lose(client, method, params):
            outcome = actual(client, method, params)
            if method == "runtime.status":
                return outcome
            raise TimeoutError("synthetic response loss after actual receipt")

        with patch.object(SignedGroupClient, "invoke", lose):
            with self.assertRaisesRegex(MatrixExecutionError, "unconfirmed"):
                self.send()
        with patch.object(
            self.sender, "send", side_effect=AssertionError("must not replay")
        ):
            result = self.broker.inspect()
        self.assertTrue(result["semantic"]["terminal"])
        self.assertEqual(len(self.receiver.page(after=0, limit=10)), 1)

    def test_wrong_route_and_payload_overrides_never_invoke_sdk(self):
        with patch.object(
            SignedGroupClient, "invoke", side_effect=AssertionError("must not invoke")
        ):
            for payload in (
                {"text": "x", "destination": self.target},
                {"text": "x", "scope": 0},
                {"text": ""},
            ):
                with self.assertRaises(MatrixExecutionError):
                    self.broker.submit(self.scope, payload, 1)
            with self.assertRaises(MatrixExecutionError):
                self.broker.submit(
                    replace(self.scope, thread=str(uuid.uuid4())), {"text": "x"}, 1
                )
        with patch.object(
            SignedGroupClient, "invoke", side_effect=AssertionError("must not invoke")
        ):
            with self.assertRaises(MatrixExecutionError):
                MatrixExecutionBroker(
                    self.broker._instruction,
                    canonical(self.broker._task),
                    SignedGroupClient(self.fixture, self.receiver),
                )
        invoke = SignedGroupClient.invoke
        methods = []

        def wrong_source(client, method, params):
            methods.append(method)
            if method == "runtime.status":
                return {}, {
                    "ok": True,
                    "result": {
                        "schema": "dm.runtime.status/v1",
                        "being_ref": self.receiver.lane.being_ref,
                        "local_origin": self.sender.ledger.local_origin,
                    },
                }
            return invoke(client, method, params)

        with patch.object(SignedGroupClient, "invoke", wrong_source):
            with self.assertRaises(MatrixExecutionError):
                self.send()
        self.assertEqual(methods, ["runtime.status"])
        result = self.send()
        for changes in (
            {"schema": "dm.tribe.delivery/v1"},
            {"destination": {}},
            {"author": {}},
            {"response_context": {}},
            {"message_hash": "invalid"},
        ):
            with self.assertRaises(MatrixExecutionError):
                self.broker._validate_result(result | changes)

    def test_existing_signed_execution_store_owns_admission_and_cancellation(self):
        state = store_fixtures.StoreTests()
        state.setUp()
        self.addCleanup(state.doCleanups)
        instruction = replace(self.broker._instruction, store_id=state.store.store_id)
        task = canonical(self.broker._task)
        # make_task already emits canonical JSON; task hash stays identical.
        broker = MatrixExecutionBroker(instruction, task, self.broker._client)
        runner = runner_fixtures.BoundedFakeRunner()
        runner.binding = instruction.binding
        controller = broker.controller(state.store, runner, monotonic=lambda: 10)
        self.assertIsNone(controller.run_due_once(instruction.instruction_id, 1, task))
        from daimon_matrix.execution_instruction import ExecutionDenied

        # Independently approved task with identical Scope but another recipient
        # must not be admitted through this broker's original route/provenance.
        alternate_task = make_task(
            "A different approved task",
            self.input_ref,
            self.scope,
            selected_destination={
                "kind": "being",
                "being_ref": self.sender.lane.being_ref,
            },
        )
        alternate = replace(
            instruction,
            instruction_id="review-2",
            task_sha256=hashlib.sha256(alternate_task.encode()).hexdigest(),
        )
        state.approve(alternate, "approval-other")
        with patch.object(
            SignedGroupClient, "invoke", side_effect=AssertionError("must not invoke")
        ):
            with self.assertRaises(ExecutionDenied):
                controller.run_due_once(alternate.instruction_id, 1, alternate_task)
        self.assertEqual(len(runner.starts), 0)
        state.approve(instruction)
        context = controller.run_due_once(instruction.instruction_id, 1, task)
        self.assertIsNotNone(context)
        result = context.native(
            self.scope, {"text": "Single approved synthetic request"}
        )
        self.assertTrue(result["semantic"]["terminal"])
        self.assertIsNone(controller.run_due_once(instruction.instruction_id, 1, task))
        from tests.test_execution_instruction import proof

        payload = state.store.cancellation_payload(instruction.instruction_id, 1)
        state.store.cancel(
            instruction.instruction_id, 1, proof(state.key, "cancel", payload, "cancel")
        )
        from daimon_matrix.execution_instruction import ExecutionDenied

        with self.assertRaises(ExecutionDenied):
            context.native(self.scope, {"text": "must not escape cancellation"})
        self.assertEqual(len(self.receiver.page(after=0, limit=10)), 1)


class AuthenticatedSourceTests(unittest.TestCase):
    def test_wrong_being_is_rejected_by_real_local_client_before_send(self):
        fixture = client_fixtures.ClientFixture()
        fixture.setUp()
        self.addCleanup(fixture.doCleanups)
        self.addCleanup(fixture.tearDown)
        client = fixture.client()
        scope = Scope("tribe", str(uuid.uuid4()), "messaging_send", "send")
        task = make_task(
            "Synthetic local source check",
            str(uuid.uuid4()),
            scope,
            selected_destination={
                "kind": "being",
                "being_ref": "dm:being:v1:" + "A" * 43,
            },
        )
        instruction = Instruction.from_dict(
            request(
                mode="manual",
                interval=None,
                max_cycles=1,
                being="dm:being:v1:" + "B" * 43,
                embodiment=client.config.expected_server["embodiment_id"],
                task_sha256=hashlib.sha256(task.encode()).hexdigest(),
                scope=[
                    {
                        k: getattr(scope, k)
                        for k in ("channel", "thread", "tool", "action")
                    }
                ],
            )
        )
        broker = MatrixExecutionBroker(instruction, task, client)
        from daimon_matrix.client import LocalClient

        invoke = LocalClient.invoke
        calls = []

        def observed(selected, method, params):
            calls.append(method)
            result = invoke(selected, method, params)
            self.assertTrue(result[1]["ok"])
            return result

        with patch.object(LocalClient, "invoke", observed):
            with self.assertRaises(MatrixExecutionError):
                broker.submit(scope, {"text": "Must never be authored"}, 1)
        self.assertEqual(calls, ["runtime.status"])


if __name__ == "__main__":
    unittest.main()
