#!/usr/bin/env python3
"""One selected technical Matrix send; private owner binding, bounded JSON stdin.

No runtime is loaded outside its daemon, inbox is read, approval is signed, model
is invoked or uncertain native send retried. The controller owns its reservation.
"""

import argparse
import json
import sys
from dataclasses import dataclass, replace
from pathlib import Path

from matrix_execution import (
    MatrixConversationTransport,
    MatrixExecutionError,
    canonical,
    closed,
    strict_json,
)


@dataclass(frozen=True)
class NativeOwnerClient:
    """Separate existing purpose capabilities for one authenticated runtime."""

    messaging: object
    observation: object
    timeout_seconds: float = 10
    deadline_ns: int | None = None

    def __post_init__(self):
        if (
            self.messaging.config.expected_server
            != self.observation.config.expected_server
            or self.messaging.config.runtime_id != self.observation.config.runtime_id
            or self.messaging.socket_path != self.observation.socket_path
        ):
            raise MatrixExecutionError(
                "Select one authenticated runtime for both purposes"
            )

    @property
    def config(self):
        return self.messaging.config

    def invoke(self, method, params):
        if method == "runtime.status":
            client = self.observation
        elif method in ("messaging.send", "messaging.delivery", "messaging.reply"):
            client = self.messaging
        else:
            raise MatrixExecutionError("Unsupported technical host operation")
        return replace(
            client, timeout_seconds=self.timeout_seconds, deadline_ns=self.deadline_ns
        ).invoke(method, params)


def configured_client(path, source):
    from daimon_matrix.client import ClientConfig, LocalClient
    from daimon_matrix.messaging_config import protected_read

    binding = closed(
        strict_json(protected_read(path)),
        ("schema", "messaging", "observation", "source"),
    )
    if binding["schema"] != "telecodex.matrix-host-binding/v1":
        raise MatrixExecutionError("Explicit owner Matrix host binding required")
    if binding["source"] != source:
        raise MatrixExecutionError("Owner binding differs from selected source")

    def load(value):
        closed(value, ("socket", "client_config", "client_key"))
        for field in value:
            if (
                not isinstance(value[field], str)
                or not Path(value[field]).is_absolute()
            ):
                raise MatrixExecutionError(
                    "Owner-local absolute SDK reference required"
                )
        config = ClientConfig.load(
            Path(value["client_config"]),
            protected_read(Path(value["client_key"]), size=32),
        )
        return LocalClient(Path(value["socket"]), config)

    messaging, observation = load(binding["messaging"]), load(binding["observation"])
    methods = set(messaging.config.capability.methods)
    if "messaging.send" not in methods or not methods <= {
        "messaging.send",
        "messaging.reply",
        "messaging.inbox",
        "messaging.delivery",
    }:
        raise MatrixExecutionError("Existing messaging-only capability required")
    if "runtime.status" not in observation.config.capability.methods:
        raise MatrixExecutionError("Existing source observation capability required")
    return NativeOwnerClient(messaging, observation)


def send(value, client, *, timeout=10):
    """Trusted controller snapshot plus already authenticated client; no runner."""
    closed(value, ("status", "source"))
    transport = MatrixConversationTransport(value["status"], value["source"], client)
    native = transport.send(timeout=timeout)
    request = value["status"]["request"]
    # The SDK authenticates the resolved destination (including its authority
    # metadata). The controller retains only its selected social recipient plus
    # native correlation IDs; signed wire evidence remains in the SDK journal.
    submission = {
        key: native[key]
        for key in ("send_id", "message_id", "message_hash", "thread_id", "author")
    }
    submission["destination"] = request["destination"]
    return {
        "schema": "telecodex.matrix-transport-outcome/v1",
        "request": request["id"],
        "binding": request["binding"],
        "submission": submission,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binding", type=Path, required=True)
    args = parser.parse_args()
    raw = sys.stdin.buffer.read(65537)
    if len(raw) > 65536:
        raise MatrixExecutionError("Bounded controller status required")
    value = closed(strict_json(raw), ("status", "source"))
    result = send(value, configured_client(args.binding, value["source"]))
    print(canonical(result))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        # Credentials, original text, local paths and SDK diagnostics stay local.
        print(json.dumps({"error": type(error).__name__}), file=sys.stderr)
        raise SystemExit(1)
