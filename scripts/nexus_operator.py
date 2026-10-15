#!/usr/bin/env python3
"""Owner-local Nexus API adapter. JSON stdin; no bot consumer or model client."""
import argparse
import json
import os
from pathlib import Path
import stat
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workspace', type=Path, required=True)
    parser.add_argument('--state', type=Path, required=True)
    parser.add_argument('operation', choices=['binding', 'configure', 'pending', 'inspect', 'dispatch', 'receipt', 'capture-input', 'current-visits'])
    args = parser.parse_args()
    os.umask(0o077)
    directory = args.state.lstat()
    if not stat.S_ISDIR(directory.st_mode) or directory.st_uid != os.getuid() or directory.st_mode & 0o077:
        raise ValueError('private_existing_nexus_directory_required')
    database = args.state / 'nexus.db'
    info = database.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError('private_existing_nexus_state_required')
    raw = sys.stdin.read(131073)
    if len(raw) > 131072:
        raise ValueError('operator_input_too_large')
    body = json.loads(raw)
    sys.path.insert(0, str(args.workspace))
    from backend.conversation import ConversationStore
    from backend.store import Store
    from backend.evidence import context
    store = Store(database)
    bridge = ConversationStore(store)
    if args.operation == 'binding':
        if set(body) != {'table_id', 'author_ref'}:
            raise ValueError('binding_scope_required')
        result = bridge.binding(body['table_id'], body['author_ref'])
    elif args.operation == 'configure':
        result = bridge.configure(body)
    elif args.operation == 'pending':
        if set(body) != {'listener_id'}:
            raise ValueError('listener_scope_required')
        result = bridge.pending(body['listener_id'])
    elif args.operation in {'dispatch', 'inspect'}:
        if set(body) != {'listener_id', 'request_id'}:
            raise ValueError('request_scope_required')
        result = getattr(bridge, args.operation)(body['request_id'], body['listener_id'])
        # Only explicitly selected source/version anchors, never native history.
        request = result.get('request')
        if request and args.operation == 'dispatch' and result.get('dispatch'):
            excerpts, remaining = [], 24000
            for anchor in request['contribution']['anchors']:
                try:
                    selected = store.evidence_sources(request['table_id'], anchor['source_id'], anchor['transcript_version'])
                    end = min(anchor['end_ms'], anchor['start_ms'] + 300000)
                    passage = context(selected[0], anchor['start_ms'], end, 0) if selected else None
                except Exception:
                    # Dispatch is already committed. Losing optional evidence must
                    # not hide its durable identity or authorize a repeated send.
                    passage = None
                if passage is None:
                    excerpts.append({'anchor': anchor, 'available': False})
                    continue
                kept = []
                for segment in passage['segments']:
                    size = len(segment['text'])
                    if size <= min(remaining, 4000):
                        kept.append(segment); remaining -= size
                limited = len(kept) != len(passage['segments']) or end != anchor['end_ms']
                passage.update(segments=kept, excerpt_limited=limited)
                excerpts.append(passage)
            request['selected_evidence'] = excerpts
    elif args.operation in {'capture-input', 'current-visits'}:
        from backend.visits import VisitStore
        visits = VisitStore(store)
        result = visits.capture(body) if args.operation == 'capture-input' else visits.latest_native(body)
    else:
        result = bridge.receipt(body)
    print(json.dumps(result, ensure_ascii=False))


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        # Private input and backend traceback stay out of application logs.
        print(json.dumps({'error': type(error).__name__}), file=sys.stderr)
        raise SystemExit(1)
