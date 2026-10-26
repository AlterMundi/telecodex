#!/usr/bin/env python3
"""Finite safe incident upsert through the existing scoped Alerts client.

The backend retains immutable payloads/receipts. This helper owns no scheduler,
recovery authority, Telegram consumer, or delivery/acknowledgment behavior.
"""
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from dashboard.alerts import Client, identifier
from dashboard.alerts_contract import validate
from dashboard.private_state import read_json


def publish(config, payload):
    validate(payload, 'IncidentInput')
    # The operator binds this producer to one receiving scope and resource.
    for key in ('producer_id', 'receiving_scope', 'resource_ref'):
        if identifier(payload[key]) != identifier(config[key]):
            raise ValueError('projection_binding_changed')
    return Client(config).call('PUT', '/v1/incidents/' + payload['producer_id'] + '/' + payload['incident_id'], 'Acceptance', payload)


def main():
    try:
        if len(sys.argv) != 3 or sys.argv[1] != '--config':
            raise ValueError('private_configuration_required')
        path = Path(sys.argv[2])
        config = read_json(path)
        raw = sys.stdin.buffer.read(16385)
        if len(raw) > 16384:
            raise ValueError('projection_too_large')
        result = publish(config, json.loads(raw))
        print(json.dumps(result, separators=(',', ':')))
    except Exception:
        # No provider/credential-bearing exception text enters logs or context.
        print('Alerts acceptance unavailable; immutable projection retained', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
