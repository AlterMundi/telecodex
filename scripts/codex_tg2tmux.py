#!/usr/bin/env python3
"""Open existing Telecodex conversations in named tmux sessions, without sending input."""
import argparse
import collections
import fcntl
import hashlib
import json
import os
from pathlib import Path
import queue
import re
import shlex
import sqlite3
import subprocess
import sys
import time
import tomllib
import uuid

from activity_indicator import Native, readonly


def topics(config, chat=None):
    """Use bridge-owned IDs. Explicit topic titles outrank text-derived summaries."""
    with readonly(config['db_path']) as db:
        chats = [r[0] for r in db.execute('SELECT DISTINCT chat_id FROM sessions WHERE thread_id>0')]
        if chat is None:
            if len(chats) != 1:
                raise ValueError('Select --chat: this store has zero or multiple topic chats')
            chat = chats[0]
        names = dict(db.execute('SELECT key,value FROM bot_state WHERE key LIKE ?',
                                (f'session_title_owner:{chat}:%',)))
        events = {}
        for row in db.execute("SELECT payload_json FROM incoming_updates WHERE json_extract(payload_json,'$.message.chat.id')=? ORDER BY update_id", (chat,)):
            message = json.loads(row[0]).get('message') or {}
            key = message.get('message_thread_id')
            for field in ('forum_topic_created', 'forum_topic_edited'):
                name = (message.get(field) or {}).get('name')
                if name:
                    events[key] = name
        result = []
        for row in db.execute('SELECT * FROM sessions WHERE chat_id=? AND thread_id>0 ORDER BY thread_id', (chat,)):
            item = dict(row)
            explicit = names.get(f'session_title_owner:{chat}:{item["thread_id"]}')
            item['title'] = explicit or events.get(item['thread_id']) or item['session_title']
            item['title_source'] = 'explicit-topic' if explicit else ('telegram-event' if item['thread_id'] in events else 'unverified-summary')
            # Never return prompts, assistant text, user ACLs or credentials.
            result.append({k: item[k] for k in ('chat_id','thread_id','codex_thread_id','force_fresh_thread','cwd','title','title_source')})
    counts = collections.Counter(x['codex_thread_id'] for x in result if x['codex_thread_id'])
    for item in result:
        item['binding_conflict'] = bool(item['codex_thread_id'] and counts[item['codex_thread_id']] > 1)
    return result


def session_names(items):
    bases = {}
    for item in items:
        name = re.sub(r'[.:\x00-\x1f\x7f]', '_', item.get('title') or f'topic-{item["thread_id"]}').strip()
        bases[item['thread_id']] = name[:100] or f'topic-{item["thread_id"]}'
    counts = collections.Counter(bases.values())
    result, used = {}, set()
    for key, base in bases.items():
        name = f'{base} [{key}]' if counts[base] > 1 else base
        while name in used:
            name += f' [{key}]'
        result[key] = name
        used.add(name)
    return result


class OperatorNative(Native):
    def rename(self, thread_id, name):
        # Deliberately expose only this additional metadata operation, never turn/start.
        self.sequence += 1
        request_id = self.sequence
        self.send({'id':request_id, 'method':'thread/name/set',
                   'params':{'threadId':thread_id, 'name':name}})
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            value = self.messages.get(timeout=max(.01, deadline-time.monotonic()))
            if value.get('id') == request_id:
                if 'error' in value:
                    raise RuntimeError('Native rename failed; no input was sent')
                return
        raise TimeoutError('Native rename receipt missing; inspect before retry')


def native_metadata(native, item):
    tid = item.get('codex_thread_id')
    if item.get('binding_conflict'):
        return {'available':False, 'reason':'native-thread-bound-to-multiple-topics'}
    if not tid or item['force_fresh_thread']:
        return {'available':False, 'reason':'no-established-binding'}
    if str(uuid.UUID(tid)) != tid:
        raise ValueError('Malformed native thread ID')
    result = native.rpc('thread/read', {'threadId':tid, 'includeTurns':False})
    thread = (result or {}).get('thread', {})
    if thread.get('id') != tid:
        return {'available':False, 'reason':'native-thread-unavailable'}
    return {'available':True, 'native_name':thread.get('name'), 'native_status':thread.get('status',{}).get('type')}


class Tmux:
    def __init__(self, socket=None):
        self.prefix = ['tmux'] + (['-L',socket] if socket else [])

    def run(self, *args, check=True):
        return subprocess.run(self.prefix+list(args), check=check, text=True, capture_output=True)

    def sessions(self):
        result = self.run('list-sessions','-F','#{session_id}\t#{session_name}\t#{@telecodex_binding}',check=False)
        if result.returncode:
            if 'no server running' in result.stderr or 'error connecting' in result.stderr:
                return []
            raise RuntimeError('Cannot inspect tmux sessions')
        return [line.split('\t',2) for line in result.stdout.splitlines()]

    def open(self, item, name, binary, binding):
        existing = self.sessions()
        matches = [s for s in existing if s[2] == binding]
        if len(matches)>1:
            raise ValueError('Multiple tmux sessions claim this binding')
        conflict = [s for s in existing if s[1] == name and s[2] != binding]
        if conflict:
            raise ValueError('tmux name belongs to another session; existing session preserved')
        if matches:
            sid, old, _ = matches[0]
            if old != name:
                self.run('rename-session','-t',sid,name)
            return {'action':'reused','tmux_id':sid,'tmux_name':name}
        cwd = Path(item['cwd'])
        if not cwd.is_dir():
            raise ValueError('Configured working directory is unavailable')
        # One safely quoted shell command; never type into an existing pane.
        argv = ['env','-u','CODEX_THREAD_ID',binary,'resume',item['codex_thread_id']]
        result = self.run('new-session','-d','-s',name,'-c',str(cwd),'-P','-F','#{session_id}',shlex.join(argv))
        sid = result.stdout.strip()
        self.run('set-option','-t',sid,'@telecodex_binding',binding)
        self.run('set-option','-t',sid,'@codex_thread_id',item['codex_thread_id'])
        self.run('set-option','-t',sid,'@telegram_topic_title',item['title'])
        return {'action':'created','tmux_id':sid,'tmux_name':name}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('list','open','sync-names'), nargs='?',default='list')
    parser.add_argument('--config',type=Path,default=Path(os.environ.get('TELECODEX_CONFIG','telecodex.toml')))
    parser.add_argument('--chat',type=int)
    parser.add_argument('--topic',action='append',default=[],help='Exact topic title or numeric topic ID; repeatable')
    parser.add_argument('--all',action='store_true',help='Select all established conversations with verified topic titles')
    parser.add_argument('--json',action='store_true')
    parser.add_argument('--tmux-socket',help='Optional alternate tmux server name')
    args = parser.parse_args()
    config = tomllib.loads(args.config.read_text())
    items = topics(config,args.chat)
    names = session_names(items)
    selected = items
    if args.topic:
        selected = []
        for selector in args.topic:
            matches=[x for x in items if str(x['thread_id'])==selector or x['title']==selector]
            if len(matches)!=1:
                raise ValueError(f'Topic selection missing or ambiguous: {selector!r}; use numeric ID')
            if matches[0] not in selected:
                selected.append(matches[0])
    elif args.action!='list':
        if not args.all:
            parser.error('Select --topic NAME or --all')
        selected=[x for x in items if x['title_source']!='unverified-summary' and x['codex_thread_id'] and not x['force_fresh_thread']]
    tmux=Tmux(args.tmux_socket)
    # Serialize our own open/rename operations without locking the bridge or Codex.
    lock_dir=Path(os.environ.get('XDG_RUNTIME_DIR',f'/tmp/telecodex-{os.getuid()}'))/'codex-tg2tmux'
    lock_dir.mkdir(mode=0o700,parents=True,exist_ok=True)
    if lock_dir.stat().st_uid != os.getuid() or lock_dir.stat().st_mode & 0o077:
        raise ValueError('Private owner-local lock directory required')
    with (lock_dir/'operator.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        native=OperatorNative(config.get('codex',{}).get('binary','codex'))
        failed=False
        try:
            for item in selected:
                record=dict(item)
                try:
                    binding=hashlib.sha256((str(Path(config['db_path']).resolve())+f':{item["chat_id"]}:{item["thread_id"]}:{item["codex_thread_id"]}').encode()).hexdigest()
                    reuse = args.action=='open' and not item.get('binding_conflict') and item['codex_thread_id'] and not item['force_fresh_thread'] and any(x[2]==binding for x in tmux.sessions())
                    if reuse:
                        record.update(available=True, availability_evidence='existing-tmux-binding')
                    else:
                        record.update(native_metadata(native,item))
                    record['tmux_name']=names[item['thread_id']]
                    if args.action=='sync-names':
                        if item['title_source']=='unverified-summary':
                            raise ValueError('Topic title is unverified; use /rename in Telegram first')
                        if not record['available']:
                            raise ValueError(record['reason'])
                        if record['native_name']!=item['title']:
                            native.rename(item['codex_thread_id'],item['title'])
                            after=native_metadata(native,item)
                            if after.get('native_name')!=item['title']:
                                raise RuntimeError('Native rename not confirmed')
                            record.update(after)
                            record['action']='renamed'
                        else:
                            record['action']='already-named'
                    elif args.action=='open':
                        if not record['available']:
                            raise ValueError(record['reason'])
                        record.update(tmux.open(item,names[item['thread_id']],config.get('codex',{}).get('binary','codex'),binding))
                except (ValueError,RuntimeError,subprocess.CalledProcessError,TimeoutError,queue.Empty) as error:
                    record['error']='native-metadata-timeout; no input sent' if isinstance(error,queue.Empty) else (str(error) or type(error).__name__)
                    failed=True
                if args.json:
                    print(json.dumps(record,ensure_ascii=False))
                else:
                    state=record.get('error') or record.get('action') or record.get('reason') or record.get('native_status')
                    mismatch=' [name differs]' if record.get('available') and 'native_name' in record and record.get('native_name')!=item['title'] else ''
                    print(f'{item["thread_id"]}\t{item["title"] or "(unnamed)"}\t{state}{mismatch}\t{item["title_source"]}')
        finally:
            native.close()
    return int(failed)


if __name__=='__main__':
    try:
        sys.exit(main())
    except (ValueError,OSError,sqlite3.Error,TimeoutError,queue.Empty) as error:
        print(f'codex-tg2tmux: {type(error).__name__}: {error}',file=sys.stderr)
        sys.exit(1)
