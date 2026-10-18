"""Host adapter between approved ExecutionStore cycles and the selected Matrix SDK.

No client, journal, scheduler, inbox reader or model is created by this module.
Only a trusted host passes the immutable approved task and authenticated SDK
client to the existing ReviewController. This object is never a model tool or
an OS isolation boundary. SDK native journals own send identity and receipts;
ExecutionStore owns approval, admission, fuel and uncertainty. Provenance text
never authorizes receiving inference. A paid cross-body trial still requires its
selected participants, qualified runner, provider accounting and human approval.
"""

import hashlib
import json
import math
import re
import time
import uuid
from dataclasses import fields, is_dataclass, replace

TASK_SCHEMA = "telecodex.matrix-task/v1"
RELAY_SCHEMA = "telecodex.matrix-relay/v1"
MAX_TEXT_BYTES = 16384


class MatrixExecutionError(ValueError):
    pass


def canonical(value):
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
        allow_nan=False,
    )


def strict_json(text):
    def pairs(rows):
        result = {}
        for key, value in rows:
            if key in result:
                raise MatrixExecutionError("Duplicate JSON member")
            result[key] = value
        return result

    try:
        return json.loads(
            text,
            object_pairs_hook=pairs,
            parse_constant=lambda _: (_ for _ in ()).throw(ValueError()),
        )
    except (ValueError, TypeError) as error:
        raise MatrixExecutionError("Closed JSON required") from error


def closed(value, names):
    if not isinstance(value, dict) or set(value) != set(names):
        raise MatrixExecutionError("Closed routing selection required")
    return value


def identifier(value):
    try:
        valid = isinstance(value, str) and str(uuid.UUID(value)) == value
    except ValueError:
        valid = False
    if not valid:
        raise MatrixExecutionError("Canonical UUID required")
    return value


def text(value, maximum=MAX_TEXT_BYTES):
    if not isinstance(value, str) or not value.strip() or len(value.encode()) > maximum:
        raise MatrixExecutionError("Bounded nonempty text required")
    return value


def destination(value):
    if not isinstance(value, dict) or value.get("kind") not in ("being", "body"):
        raise MatrixExecutionError("Select an explicit Matrix recipient")
    closed(
        value,
        ("kind", "being_ref", "embodiment_id")
        if value["kind"] == "body"
        else ("kind", "being_ref"),
    )
    if not isinstance(value["being_ref"], str) or not re.fullmatch(
        r"dm:being:v1:[A-Za-z0-9_-]{43}", value["being_ref"]
    ):
        raise MatrixExecutionError("Signed being reference required")
    if value["kind"] == "body":
        text(value["embodiment_id"], 240)
    return dict(value)


def parent_from_verified_sdk(message):
    """Call only on an SDK-authenticated selected message, never wire/model JSON.

    The SDK retains/verifies signatures and audience. Extracting these fields is
    not independent signature validation or execution approval.
    """
    selected = message["payload"]["body"].get("destination")
    if not isinstance(selected, dict):
        raise MatrixExecutionError("A directed parent is required for a private return")
    selected = {
        key: selected[key]
        for key in (
            ("kind", "being_ref", "embodiment_id")
            if selected.get("kind") == "body"
            else ("kind", "being_ref")
        )
    }
    destination(selected)
    return {
        "destination": selected,
        "message_id": identifier(message["event_id"]),
        "message_hash": message["content_hash"],
        "sender_being_ref": message["being_ref"],
        "sender_embodiment_id": message["origin"]["embodiment_id"],
        "thread_id": identifier(message["payload"]["intent"]["thread_id"]),
    }


def validate_task(task):
    parsed = strict_json(task)
    names = ("schema", "question", "input_ref", "scope", "destination", "parent")
    value = closed(
        parsed,
        names
        + tuple(
            key
            for key in ("origin", "context", "input_kind")
            if isinstance(parsed, dict) and key in parsed
        ),
    )
    text(value["question"])
    kinds = ("human_text", "committed_machine_transcript", "session_authored")
    if value.get("input_kind", "session_authored") not in kinds:
        raise MatrixExecutionError("Explicit input authorship kind required")
    context = value.get("context", [])
    if not isinstance(context, list) or len(context) > 4:
        raise MatrixExecutionError("Bounded selected parent context required")
    size = len(value["question"].encode())
    seen = set()
    for row in context:
        closed(row, ("request", "binding", "text", "input_kind"))
        identifier(row["request"])
        if row["request"] in seen or row["input_kind"] not in kinds:
            raise MatrixExecutionError("Distinct attributed parent context required")
        seen.add(row["request"])
        if not isinstance(row["binding"], str) or not re.fullmatch(
            "[a-f0-9]{64}", row["binding"]
        ):
            raise MatrixExecutionError("Frozen parent binding required")
        size += len(text(row["text"]).encode())
    if size > 24000:
        raise MatrixExecutionError(
            "Selected source context exceeds the bounded package"
        )
    if any(key in value for key in ("context", "origin", "input_kind")):
        # Same serialized-content bound as transactional Telecodex selection.
        # The remaining 1024 bytes cover the closed relay schema/execution ref.
        content = {
            "text": value["question"],
            "context": context,
            "input_kind": value.get("input_kind", "session_authored"),
        }
        if len(canonical(content).encode()) > MAX_TEXT_BYTES - 1024:
            raise MatrixExecutionError(
                "Selected context exceeds serialized relay limit"
            )
    if value.get("origin") is not None:
        origin = closed(
            value["origin"], ("schema", "request", "binding", "source_sha256")
        )
        if origin["schema"] != "telecodex.matrix-origin/v1":
            raise MatrixExecutionError("Unsupported conversation origin")
        identifier(origin["request"])
        if origin["request"] != value["input_ref"]:
            raise MatrixExecutionError(
                "Conversation request identity differs from native send"
            )
        for key in ("binding", "source_sha256"):
            if not isinstance(origin[key], str) or not re.fullmatch(
                "[a-f0-9]{64}", origin[key]
            ):
                raise MatrixExecutionError("Frozen conversation origin digest required")
        if (
            hashlib.sha256(value["question"].encode()).hexdigest()
            != origin["source_sha256"]
        ):
            raise MatrixExecutionError("Question differs from frozen original")
    if value["schema"] != TASK_SCHEMA:
        raise MatrixExecutionError("Unsupported task schema")
    text(value["question"])
    identifier(value["input_ref"])
    scope = closed(value["scope"], ("channel", "thread", "tool", "action"))
    identifier(scope["thread"])
    # Current receiving profiles use one membership channel. Archived bilateral
    # channels cannot become a fallback for an unavailable selected recipient.
    if scope["channel"] != "tribe":
        raise MatrixExecutionError("Selected membership-group channel required")
    if (scope["tool"], scope["action"]) == ("messaging_send", "send"):
        destination(value["destination"])
        if value["parent"] is not None:
            raise MatrixExecutionError("Send cannot override a reply parent")
    elif (scope["tool"], scope["action"]) == ("messaging_reply", "reply"):
        parent = closed(
            value["parent"],
            (
                "message_id",
                "message_hash",
                "sender_being_ref",
                "sender_embodiment_id",
                "thread_id",
                "destination",
            ),
        )
        destination(parent["destination"])
        identifier(parent["message_id"])
        if not isinstance(parent["message_hash"], str) or not re.fullmatch(
            "[a-f0-9]{64}", parent["message_hash"]
        ):
            raise MatrixExecutionError("Verified parent digest required")
        if parent["thread_id"] != scope["thread"] or value["destination"] is not None:
            raise MatrixExecutionError("Reply must inherit its exact parent route")
        destination(
            {
                "kind": "body",
                "being_ref": parent["sender_being_ref"],
                "embodiment_id": parent["sender_embodiment_id"],
            }
        )
    else:
        raise MatrixExecutionError("Only one selected request or reply is supported")
    return value


def make_task(
    question,
    input_ref,
    scope,
    *,
    selected_destination=None,
    verified_parent=None,
    origin=None,
    context=None,
    input_kind=None,
):
    task = canonical(
        {
            "schema": TASK_SCHEMA,
            "question": question,
            "input_ref": input_ref,
            "scope": {
                k: getattr(scope, k) for k in ("channel", "thread", "tool", "action")
            },
            "destination": selected_destination,
            "parent": verified_parent,
        }
    )
    extras = {
        key: value
        for key, value in (
            ("origin", origin),
            ("context", context),
            ("input_kind", input_kind),
        )
        if value is not None
    }
    if extras:
        value = strict_json(task)
        value.update(extras)
        task = canonical(value)
    validate_task(task)
    return task


def make_conversation_task(status, scope):
    """Trusted host only: consume an authenticated Telecodex matrix status.

    No input is captured or rewritten. The native request is a child of the
    frozen retained original; private human return coordinates stay in Telecodex.
    This prepares an exact task, not approval, sending or receiving inference.
    """
    if status.get("current") is not True or status.get("phase") != "awaiting_execution":
        raise MatrixExecutionError(
            "Selected conversation request is not current and pending"
        )
    request = status["request"]
    if request.get("schema") != "telecodex.matrix-request/v1":
        raise MatrixExecutionError("Authenticated retained Matrix request required")
    original = request["original"]
    source = text(original["text"])
    if (
        original["human_id"] != request["human"]
        or original["origin"]["chat_id"] != request["entry"]["chat_id"]
        or original["origin"]["thread_id"] != request["entry"]["thread_id"]
    ):
        raise MatrixExecutionError(
            "Original human/return scope differs from retained entry"
        )
    bound = {key: value for key, value in request.items() if key != "binding"}
    if hashlib.sha256(canonical(bound).encode()).hexdigest() != request["binding"]:
        raise MatrixExecutionError(
            "Retained conversation binding differs from its original"
        )
    return make_task(
        source,
        identifier(request["id"]),
        scope,
        selected_destination=destination(request["destination"]),
        context=request.get("parents", []),
        input_kind="committed_machine_transcript"
        if original.get("transcription")
        else "human_text",
        origin={
            "schema": "telecodex.matrix-origin/v1",
            "request": request["id"],
            "binding": request["binding"],
            "source_sha256": hashlib.sha256(source.encode()).hexdigest(),
        },
    )


def make_received_task(message, scope):
    """Prepare one selected SDK-verified request for independent human approval.

    Peer content is data. This does not approve/reserve a cycle, run a model,
    register a listener, read an inbox or widen the existing reply audience.
    """
    parent = parent_from_verified_sdk(message)
    relay = strict_json(message["payload"]["body"]["text"])
    names = ("schema", "via", "kind", "input_ref", "execution_ref", "text")
    closed(
        relay, names + tuple(key for key in ("context", "input_kind") if key in relay)
    )
    if (
        relay["schema"] != RELAY_SCHEMA
        or relay["via"] != "interaction-controller"
        or relay["kind"] != "session_request"
    ):
        raise MatrixExecutionError("Selected attributed session request required")
    if scope.tool != "messaging_reply" or scope.action != "reply":
        raise MatrixExecutionError(
            "Receiving execution must preserve the verified reply route"
        )
    return make_task(
        text(relay["text"]),
        identifier(relay["input_ref"]),
        scope,
        verified_parent=parent,
        context=relay.get("context", []),
        input_kind=relay.get("input_kind", "session_authored"),
    )


class MatrixExecutionBroker:
    def __init__(self, instruction, task, native_client):
        value = validate_task(task)
        if (
            instruction.mode != "manual"
            or instruction.max_cycles != 1
            or len(instruction.scope) != 1
            or hashlib.sha256(task.encode()).hexdigest() != instruction.task_sha256
        ):
            raise MatrixExecutionError(
                "One approved manual task and exact digest required"
            )
        parent = value["parent"]
        if parent is not None:
            selected = parent["destination"]
            if (
                selected["being_ref"] != instruction.being
                or selected["kind"] == "body"
                and selected["embodiment_id"] != instruction.embodiment
            ):
                raise MatrixExecutionError(
                    "Reply parent was addressed to another source"
                )
        scope = instruction.scope[0]
        if value["scope"] != {
            k: getattr(scope, k) for k in ("channel", "thread", "tool", "action")
        }:
            raise MatrixExecutionError(
                "Task route differs from approved execution scope"
            )
        if not is_dataclass(native_client) or "deadline_ns" not in {
            f.name for f in fields(native_client)
        }:
            raise MatrixExecutionError(
                "Selected SDK must support an absolute client deadline"
            )
        server = getattr(getattr(native_client, "config", None), "expected_server", {})
        if server.get("embodiment_id") != instruction.embodiment:
            raise MatrixExecutionError(
                "Selected client differs from approved source body"
            )
        self._instruction, self._scope, self._task, self._client = (
            instruction,
            scope,
            value,
            native_client,
        )
        # Independent of recipient, native session generation and instruction
        # renewal. The SDK rejects changed parameters under this same send ID.
        role = "request" if scope.tool == "messaging_send" else "result"
        self.send_id = str(
            uuid.uuid5(
                uuid.NAMESPACE_URL,
                "telecodex.matrix/" + role + ":" + value["input_ref"],
            )
        )

    def controller(self, store, runner, **options):
        """Create the only supported host controller, pinned to this approval.

        Scope alone omits the recipient. Never reuse this broker with an
        unpinned ReviewController or a cycle from another approved task.
        """
        from daimon_matrix.execution_instruction import ExecutionDenied
        from daimon_matrix.review_runner import ReviewController

        broker = self
        instruction = self._instruction
        if store.store_id != instruction.store_id:
            raise MatrixExecutionError("Execution store differs from approved root")

        class PinnedController(ReviewController):
            def run_due_once(self, identity, revision, task):
                if (
                    identity != instruction.instruction_id
                    or revision != instruction.revision
                    or hashlib.sha256(task.encode()).hexdigest()
                    != instruction.task_sha256
                ):
                    raise ExecutionDenied(
                        "Controller is pinned to another approved task"
                    )
                try:
                    retained = self.store.get_instruction(identity, revision)
                except ExecutionDenied:
                    return super().run_due_once(identity, revision, task)
                if retained.sha256 != instruction.sha256:
                    raise ExecutionDenied(
                        "Controller instruction differs from approved root"
                    )
                return super().run_due_once(identity, revision, task)

        return PinnedController(store, runner, broker, **options)

    def authorize(self, scope):
        # ExecutionStore rechecks signed approval/window/cancellation. The SDK
        # separately rechecks current group membership and transport authority.
        return scope == self._scope

    def _invoke(self, method, params, timeout):
        if (
            type(timeout) not in (int, float)
            or not math.isfinite(timeout)
            or not 0.05 <= timeout <= 60
        ):
            raise MatrixExecutionError("Bounded native timeout required")
        deadline = time.monotonic_ns() + int(timeout * 1_000_000_000)
        client = replace(self._client, timeout_seconds=timeout, deadline_ns=deadline)
        try:
            if method in ("messaging.send", "messaging.reply"):
                # Same authenticated client and absolute deadline for both
                # observations. Origin is bound before any authored effect.
                _, status = client.invoke("runtime.status", {})
                source = status.get("result", {}) if status.get("ok") is True else {}
                if (
                    source.get("schema") != "dm.runtime.status/v1"
                    or source.get("being_ref") != self._instruction.being
                    or source.get("local_origin", {}).get("embodiment_id")
                    != self._instruction.embodiment
                ):
                    raise MatrixExecutionError(
                        "Authenticated source differs from approved root"
                    )
            _, response = client.invoke(method, params)
            if not isinstance(response, dict) or response.get("ok") is not True:
                raise MatrixExecutionError("Native response not successful")
            return self._validate_result(response["result"])
        except Exception:
            # CycleContext retains/charges ambiguity. Never mint another send,
            # retry, substitute a recipient or interpret this as nondelivery.
            raise MatrixExecutionError(
                "Native outcome unconfirmed; inspect the same send ID"
            ) from None

    def _validate_result(self, result):
        if (
            not isinstance(result, dict)
            or result.get("schema") != "dm.tribe.direct-delivery/v1"
            or result.get("send_id") != self.send_id
            or result.get("thread_id") != self._scope.thread
            or result.get("author")
            != {
                "being_ref": self._instruction.being,
                "embodiment_id": self._instruction.embodiment,
            }
        ):
            raise MatrixExecutionError("Native outcome does not match the fixed route")
        identifier(result.get("message_id"))
        if not isinstance(result.get("message_hash"), str) or not re.fullmatch(
            "[a-f0-9]{64}", result["message_hash"]
        ):
            raise MatrixExecutionError("Native message digest required")
        parent = self._task["parent"]
        wanted_parent = (
            None
            if parent is None
            else {
                k: v for k, v in parent.items() if k not in ("thread_id", "destination")
            }
        )
        target = self._task["destination"]
        if parent is not None:
            target = {
                "kind": parent["destination"]["kind"],
                "being_ref": parent["sender_being_ref"],
            }
            if target["kind"] == "body":
                target["embodiment_id"] = parent["sender_embodiment_id"]
        actual = result.get("destination")
        if (
            not isinstance(actual, dict)
            or any(actual.get(k) != v for k, v in target.items())
            or result.get("response_context") != wanted_parent
        ):
            raise MatrixExecutionError(
                "Native recipient or parent differs from approved route"
            )
        return result

    def submit(self, scope, payload, timeout):
        if not self.authorize(scope):
            raise MatrixExecutionError("Outside selected scope")
        closed(payload, ("text",))
        if (
            self._task.get("origin") is not None
            and scope.tool == "messaging_send"
            and payload["text"] != self._task["question"]
        ):
            raise MatrixExecutionError(
                "A relayed original cannot become model-rewritten human speech"
            )
        body = canonical(
            {
                "schema": RELAY_SCHEMA,
                "via": "interaction-controller",
                "kind": "session_request"
                if scope.tool == "messaging_send"
                else "worker_result",
                "input_ref": self._task["input_ref"],
                "execution_ref": {
                    "id": self._instruction.instruction_id,
                    "revision": self._instruction.revision,
                },
                "text": text(payload["text"]),
            }
        )
        if scope.tool == "messaging_send":
            relay = strict_json(body)
            if self._task.get("context"):
                relay["context"] = self._task["context"]
            if self._task.get("input_kind"):
                relay["input_kind"] = self._task["input_kind"]
            body = canonical(relay)
        text(body)
        params = {"channel_id": scope.channel, "send_id": self.send_id, "text": body}
        if scope.tool == "messaging_send":
            params.update(thread_id=scope.thread, destination=self._task["destination"])
        else:
            params.update(
                received_channel_id=scope.channel,
                message_id=self._task["parent"]["message_id"],
            )
        return self._invoke("messaging." + scope.action, params, timeout)

    def inspect(self, timeout=1.0):
        """Explicit host observation, not a model operation or execution recovery.

        Does not refund an ambiguous cycle, send again or create another root.
        Call only under human-authorized status inspection, never an intake hook.
        """
        return self._invoke(
            "messaging.delivery",
            {"channel_id": self._scope.channel, "send_id": self.send_id},
            timeout,
        )

    def conversation_return(self, request_result, message):
        """Owner-local callback after SDK verification; no delivery claim.

        Recording this through the authenticated Telecodex owner retains the
        canonical external reply. It does not authorize another native turn.
        """
        origin = self._task.get("origin")
        if origin is None:
            raise MatrixExecutionError("Frozen conversation origin required")
        answer = self.accept_verified_reply(request_result, message)
        selected = request_result["destination"]
        recipient = {
            key: selected[key]
            for key in (
                ("kind", "being_ref", "embodiment_id")
                if selected["kind"] == "body"
                else ("kind", "being_ref")
            )
        }
        submission = {
            key: request_result[key]
            for key in ("send_id", "message_id", "message_hash", "thread_id", "author")
        }
        submission["destination"] = recipient
        return {
            "request": origin["request"],
            "binding": origin["binding"],
            "submission": submission,
            "message_id": identifier(message["event_id"]),
            "message_hash": message["content_hash"],
            "author": {
                "being_ref": message["being_ref"],
                "embodiment_id": message["origin"]["embodiment_id"],
            },
            "text": answer,
        }

    def accept_verified_reply(self, request_result, message):
        """Correlate an already SDK-verified selected event, without inference."""
        if self._scope.tool != "messaging_send":
            raise MatrixExecutionError("Outgoing request required")
        self._validate_result(request_result)
        body = message["payload"]["body"]
        parent = body.get("response_context")
        wanted = {
            "message_id": request_result["message_id"],
            "message_hash": request_result["message_hash"],
            "sender_being_ref": self._instruction.being,
            "sender_embodiment_id": self._instruction.embodiment,
        }
        target = self._task["destination"]
        if (
            request_result.get("send_id") != self.send_id
            or request_result.get("thread_id") != self._scope.thread
            or parent != wanted
            or message["payload"]["intent"]["thread_id"] != self._scope.thread
            or message["being_ref"] != target["being_ref"]
            or target["kind"] == "body"
            and message["origin"]["embodiment_id"] != target["embodiment_id"]
        ):
            raise MatrixExecutionError(
                "Reply is not from the selected request and recipient"
            )
        relay = closed(
            strict_json(body["text"]),
            ("schema", "via", "kind", "input_ref", "execution_ref", "text"),
        )
        if (
            relay["schema"] != RELAY_SCHEMA
            or relay["via"] != "interaction-controller"
            or relay["kind"] != "worker_result"
            or relay["input_ref"] != self._task["input_ref"]
        ):
            raise MatrixExecutionError(
                "Reply provenance does not match the original input"
            )
        reference = closed(relay["execution_ref"], ("id", "revision"))
        text(reference["id"], 240)
        if type(reference["revision"]) is not int or reference["revision"] < 1:
            raise MatrixExecutionError("Positive execution revision required")
        return text(relay["text"])
