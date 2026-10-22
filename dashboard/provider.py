"""Receiving-owned metadata and authenticated owner-local conversation controls.

No bot update consumer, independent model client or arbitrary process kill.
"""
import hashlib
from contextlib import closing
import json
import os
from pathlib import Path
import re
import socket
import stat
import struct
import threading
import time
import tomllib

from scripts.activity_indicator import Native, Rollout, linked_child, readonly, timestamp, read_topic_inventory, topic_activity_inventory, pending_requests
from scripts.speech_render import sync_directory


class ProviderUnavailable(Exception):
    pass


def receiving_authority(receiver):
    """Only dependencies that select this dashboard's actor/data/control scope."""
    telegram = receiver.get('telegram', {})
    return json.dumps({
        'db_path': receiver.get('db_path'),
        'session_control': receiver.get('session_control'),
        'interaction': receiver.get('interaction'),
        'voice': receiver.get('voice'),
        'telegram': {key: telegram.get(key) for key in
                     ('api_base', 'bot_token', 'bot_token_file')},
        'native_binary': receiver.get('codex', {}).get('binary', 'codex'),
    }, sort_keys=True, separators=(',', ':')).encode()


def control(path, nonce, command='all'):
    """One bounded connect-only request; a lost acknowledgement is not retried."""
    meta = Path(path).lstat()
    parent = Path(path).parent.lstat()
    if (not stat.S_ISSOCK(meta.st_mode) or meta.st_uid != os.getuid()
            or meta.st_mode & 0o077 or not stat.S_ISDIR(parent.st_mode)
            or parent.st_uid != os.getuid() or parent.st_mode & 0o077):
        raise ProviderUnavailable('Receiving control socket unavailable')
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(12)
        stream.connect(str(path))
        _, uid, _ = struct.unpack('3i', stream.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        if uid != os.getuid():
            raise ProviderUnavailable('Receiving control owner mismatch')
        stream.sendall(json.dumps({'command': command, 'nonce': nonce}).encode() + b'\n')
        data = bytearray()
        while b'\n' not in data:
            block = stream.recv(min(4096, 262145 - len(data)))
            if not block or len(data) + len(block) > 262144:
                raise ProviderUnavailable('Receiving operation acknowledgement unavailable')
            data.extend(block)
        reply = json.loads(data)
        if reply.get('ok') is not True:
            raise ProviderUnavailable('Receiving operation was not confirmed; inspect its status')
        return reply['result']


class Host:
    def __init__(self, storage):
        self.storage = storage
        self.previous = None

    def sample(self):
        now = time.monotonic()
        result = {'observed_at': time.time(), 'cpu_percent': None,
                  'network_rx_per_second': None, 'network_tx_per_second': None}
        try:
            cpu = list(map(int, Path('/proc/stat').read_text().splitlines()[0].split()[1:9]))
            # Guest time is already included in user/nice; do not count it twice.
            net = [line.split(':', 1) for line in Path('/proc/net/dev').read_text().splitlines()[2:]]
            rx = sum(int(values.split()[0]) for name, values in net if name.strip() != 'lo')
            tx = sum(int(values.split()[8]) for name, values in net if name.strip() != 'lo')
            current = now, sum(cpu), cpu[3] + cpu[4], rx, tx
            if self.previous:
                at, total, idle, prior_rx, prior_tx = self.previous
                elapsed = now - at
                if current[1] > total:
                    result['cpu_percent'] = round(100 * (1 - (current[2] - idle) / (current[1] - total)), 1)
                if elapsed > 0 and rx >= prior_rx and tx >= prior_tx:
                    result['network_rx_per_second'] = round((rx - prior_rx) / elapsed)
                    result['network_tx_per_second'] = round((tx - prior_tx) / elapsed)
            self.previous = current
            memory = {parts[0].rstrip(':'): int(parts[1]) * 1024
                      for line in Path('/proc/meminfo').read_text().splitlines()
                      if len(parts := line.split()) >= 2}
            result['memory_total'] = memory['MemTotal']
            result['memory_available'] = memory['MemAvailable']
            result['load_average'] = list(os.getloadavg())
            result['cpu_count'] = os.cpu_count()
            result['uptime_seconds'] = int(float(Path('/proc/uptime').read_text().split()[0]))
        except (OSError, ValueError, KeyError):
            result['metrics_unavailable'] = True
        result['storage'] = []
        for label, path in self.storage.items():
            try:
                disk = os.statvfs(path)
                result['storage'].append({'label': label, 'total': disk.f_blocks * disk.f_frsize,
                                          'available': disk.f_bavail * disk.f_frsize})
            except OSError:
                result['storage'].append({'label': label, 'unavailable': True})
        # Aggregate counters may count VPN/virtual links too; this is not ISP usage.
        result['network_scope'] = 'Aggregate non-loopback interfaces; virtual links may count traffic twice'
        return result


class Provider:
    def __init__(self, config):
        self.config = config
        self.receiver_path = Path(config['receiver_config'])
        self.receiver_bytes = self.receiver_path.read_bytes()
        self.receiver = tomllib.loads(self.receiver_bytes.decode())
        self.binding_bytes = receiving_authority(self.receiver)
        self.human = int(self.receiver['session_control']['human_id'])
        self.native_home = Path(config.get('native_home', Path.home() / '.codex'))
        self.native = None
        self.lock = threading.Lock()
        self.cache = None
        self.readers = {}
        self.host = Host(config.get('storage', {'Host': '/'}))

    def assert_binding(self):
        try:
            current = receiving_authority(tomllib.loads(self.receiver_path.read_text()))
        except (OSError, ValueError):
            raise ProviderUnavailable('Receiving configuration unavailable') from None
        if current != self.binding_bytes:
            raise ProviderUnavailable('Receiving grant changed; operator must requalify the dashboard')

    def close(self):
        if self.native:
            self.native.close()
            self.native = None

    def authorized(self, human):
        try:
            self.assert_binding()
        except (OSError, ProviderUnavailable):
            return False
        with closing(readonly(self.receiver['db_path'])) as db:
            row = db.execute('SELECT allowed FROM users WHERE tg_user_id=?', (human,)).fetchone()
            return human == self.human and bool(row and row[0])

    def bindings(self):
        self.assert_binding()
        with closing(readonly(self.receiver['db_path'])) as db:
            rows = db.execute('''SELECT s.id,s.chat_id,s.thread_id,s.session_title,s.codex_thread_id,
                s.busy,s.updated_at FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id
                WHERE s.creator_user_id=? AND u.allowed=1 ORDER BY s.id LIMIT 129''', (self.human,)).fetchall()
            if len(rows) > 128:
                raise ProviderUnavailable('Session scope exceeds qualified bound')
            return [dict(row) for row in rows]

    def presentation(self, command):
        """Use the configured controller and verify its returned entry provenance."""
        self.assert_binding()
        profile = self.receiver.get('interaction', {})
        if profile.get('human_id') != self.human or not self.authorized(self.human):
            raise ProviderUnavailable('Interaction presentation is not configured for this account')
        result = control(self.receiver['session_control']['socket_path'], None, command)
        self.assert_binding()
        if not self.authorized(self.human):
            raise ProviderUnavailable('Receiving presentation grant changed')
        items = result.get('items', []) if result.get('schema') == 'telecodex.presentation-list/v1' else [result]
        expected = {'chat_id':profile['chat_id'], 'thread_id':profile['topic_id']}
        for item in items:
            selected = item.get('entry_binding', [])
            if len(selected) != 4 or selected[0] != self.human or selected[1] != expected:
                raise ProviderUnavailable('Loaded integrating entry does not match the receiving configuration')
        return result

    def presentation_list(self, delivery, native, direction='after', matrix=None):
        if type(delivery) is not int or type(native) is not int or min(delivery, native) < 0:
            raise ValueError('Invalid presentation cursor')
        if direction not in ('after', 'before'):
            raise ValueError('Invalid presentation direction')
        operation = 'before' if direction == 'before' else 'list'
        if matrix is not None and (type(matrix) is not int or matrix < 0):
            raise ValueError('Invalid Matrix presentation cursor')
        suffix='' if matrix is None else f' {matrix}'
        return self.presentation(f'presentation {operation} {delivery} {native}'+suffix)

    def sonic_activity(self):
        """Finite read of the existing database-bound observer, never native work.

        Missing, stale or replaced observation is unknown. Limit the audience to
        the canonical routing registry and targets owned by its actual human.
        """
        self.assert_binding()
        profile=self.receiver.get('interaction',{})
        if profile.get('human_id')!=self.human or not self.authorized(self.human):
            raise ProviderUnavailable('Interaction state audience unavailable')
        selected={f"{profile['chat_id']}:{profile['topic_id']}":'Conversation'}
        # This supported read resolves configured, enrolled and discovered targets
        # through Store.interaction_profile, exactly as dispatch does. Never
        # reconstruct a competing registry from static dashboard configuration.
        # An after-maximum cursor returns metadata/choices without transporting
        # original messages. This is the existing input-list cursor contract.
        choices=self.presentation(f'input list {2**63-1} after')['routing_choices']
        if not isinstance(choices,list) or len(choices)>64:
            raise ProviderUnavailable('Routing membership unavailable')
        for target in choices:
            selector=target.get('selector')
            if not isinstance(selector,str) or not re.fullmatch(r't[1-9][0-9]{0,18}',selector):
                raise ValueError('Invalid routing membership')
            selected.setdefault(f"{profile['chat_id']}:{int(selector[1:])}",str(target['label'])[:128])
        directory=self.config.get('activity_state_dir')
        with closing(readonly(self.receiver['db_path'])) as db:
            inventory=read_topic_inventory(db,Path(directory)/'topics.json') if directory else topic_activity_inventory(db,{})
            owned={item['key']:item for item in inventory['items'] if item['human']==self.human}
            items=[]
            for key,label in selected.items():
                item=owned.get(key,{})
                chat,topic=map(int,key.split(':'))
                requests=pending_requests(db,chat,topic,self.human) if item.get('session') else None
                items.append({'id':key,'label':label,'session':item.get('session'),
                    'observed_at':item.get('observed_at'),'fresh':item.get('fresh',False),
                    'execution':item.get('execution','unknown'),'goal_state':item.get('goal_state','unknown'),
                    'waiting_for_human':item.get('waiting_for_human'),
                    'tools_running':item.get('tools_running'),'error':item.get('error'),
                    'pending':bool(requests and requests['pending']) or item.get('waiting_for_human') is True})
        self.assert_binding()
        if not self.authorized(self.human):
            raise ProviderUnavailable('Interaction state audience changed')
        from dashboard.sonic import descriptor
        return {**descriptor(self.config),'schema':'telecodex.sonic-state/v1','observed_at':time.time(),
            'configured':bool(directory),'connected':inventory['connected'],
            'scope':'canonical routing registry and entry; current owned bindings only',
            'sources':items,'truncated':inventory['truncated']}

    def presentation_part(self, source, revision, offset):
        import re
        if (not isinstance(source, str) or not re.fullmatch(r'(?:(?:delivery|input|matrix):[a-f0-9-]{36}|native:[1-9][0-9]{0,18}|request:[a-f0-9]{32})', source)
                or not isinstance(revision, str) or not re.fullmatch(r'[a-f0-9]{64}', revision)
                or type(offset) is not int or not 0 <= offset <= 10**9):
            raise ValueError('Invalid presentation selection')
        if source.startswith('input:'):
            value = self.presentation(f'input status {self.input_id(source[6:])}')
            view = self.clarification_source(value)
            if view is None or view['revision'] != revision or offset >= len(view['text']):
                raise ProviderUnavailable('Selected destination question is no longer current')
            text = view['text'][offset:offset+1600]
            end = offset+len(text)
            return {**view,'schema':'telecodex.presentation-part/v1','text':text,
                'offset':offset,'next_offset':end if end<len(view['text']) else None}
        if source.startswith('request:'):
            view = self.conversation_request_detail(source[8:], offset, revision)
            if (view.get('revision') != revision or view.get('offset') != offset
                    or view.get('kind') != 'question' or view.get('actionable') is not True):
                raise ProviderUnavailable('Selected question is no longer current')
            body = view['body']
            text = body[:1500]
            if not text.strip():
                raise ProviderUnavailable('Selected question has no spoken text')
            next_offset = offset + len(text) if len(body) > len(text) else view['next_offset']
            if offset == 0:
                text = str(view.get('label') or 'Conversation')[:200] + '. ' + text
            return {'schema':'telecodex.presentation-part/v1', 'id':source,
                'revision':revision, 'kind':'native_question', 'entry_binding':view['entry_binding'],
                'text':text, 'offset':offset, 'next_offset':next_offset,
                'current':True, 'playback':'not_observed'}
        return self.presentation(f'presentation part {source} {revision} {offset}')

    def artifacts(self):
        if not self.receiver.get('interaction', {}).get('artifact_store'):
            raise ProviderUnavailable('Retained files are not configured')
        return self.presentation('presentation artifacts list')

    def artifact_detail(self, ident):
        self.input_id(ident)
        return self.presentation('presentation artifacts detail '+ident)

    def artifact_open(self, ident):
        """Return an authenticated descriptor, never a model/client-selected path."""
        import re
        ref = self.artifact_detail(ident)
        selected = self.receiver.get('interaction', {}).get('artifact_store')
        if (not isinstance(selected, dict) or not re.fullmatch('[a-f0-9]{64}', ref.get('sha256', ''))
                or type(ref.get('bytes')) is not int or not 0 <= ref['bytes'] <= selected.get('max_file_bytes', 64*1024*1024)):
            raise ValueError('Artifact storage or reference unavailable')
        root = os.open(selected['directory'], os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        stream = None
        try:
            meta = os.fstat(root)
            if meta.st_uid != os.getuid() or meta.st_mode & 0o077:
                raise ValueError('Artifact storage unavailable')
            descriptor = os.open(ref['sha256'], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK,
                                 dir_fd=root)
            stream = os.fdopen(descriptor, 'rb')
            info = os.fstat(stream.fileno())
            if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
                    or info.st_size != ref['bytes'] or info.st_mode & 0o077
                    or hashlib.file_digest(stream, 'sha256').hexdigest() != ref['sha256']):
                raise ValueError('Artifact bytes changed or are unavailable')
            stream.seek(0)
            # Recheck the actual logical grant after disk I/O. Renewal does not
            # demand that the old physical native thread remain current.
            current = self.artifact_detail(ident)
            if ({k:v for k,v in current.items() if k != 'entry_binding'} !=
                    {k:v for k,v in ref.items() if k != 'entry_binding'} or
                    current.get('entry_binding', [])[:3] != ref.get('entry_binding', [])[:3]):
                raise ValueError('Artifact changed while opening')
            return ref, stream
        except BaseException:
            if stream is not None:
                stream.close()
            raise
        finally:
            os.close(root)

    @staticmethod
    def input_id(ident):
        import uuid
        if not isinstance(ident, str) or str(uuid.UUID(ident)) != ident:
            raise ValueError('Invalid conversation input identity')
        return ident

    def conversation_inputs(self, before=0, direction='before'):
        if type(before) is not int or not 0 <= before <= 2**63 - 1:
            raise ValueError('Invalid input history cursor')
        if direction not in ('before', 'after'):
            raise ValueError('Invalid input history direction')
        page = self.presentation(f'input list {before} {direction}')
        for item in page['items']:
            self.decorate_input(item,page.get('routing_choices'),page['entry_binding'],page['binding'])
            self.retire_voice_capture(item)
        return page

    def conversation_input_status(self, ident):
        value = self.decorate_input(self.presentation(f'input status {self.input_id(ident)}'))
        self.retire_voice_capture(value)
        return value

    def retire_voice_capture(self, value):
        """Dispose only this receiver's clip after its matching durable transcript.

        Admission/delivery may still be pending: the retained transcript is the
        input from this point. A failed cleanup never changes admission or ASR.
        Status/list reads also finish cleanup interrupted by a process exit.
        """
        try:
            voice = value.get('voice') or {}
            proof = voice.get('transcription') or {}
            ident = self.input_id(value['id'])
            digest = voice.get('audio_sha256')
            if (not isinstance(value.get('text'), str) or not value['text'].strip()
                    or proof.get('capture_id') != ident
                    or not isinstance(digest, str) or len(digest) != 64
                    or proof.get('audio_sha256') != digest
                    or not proof.get('committed_at')):
                return False
            directory = Path(str(self.receiver['db_path'])+'.voice')
            info = directory.lstat()
            if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                return False
            path = directory / ident
            fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
            with os.fdopen(fd, 'rb') as source:
                info = os.fstat(source.fileno())
                if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
                        or info.st_mode & 0o077 or not 0 < info.st_size <= 20*1024*1024
                        or hashlib.sha256(source.read(20*1024*1024+1)).hexdigest() != digest):
                    return False
                current = path.lstat()
                if (current.st_dev,current.st_ino) != (info.st_dev,info.st_ino):
                    return False
                path.unlink()
                sync_directory(directory)
            return True
        except FileNotFoundError:
            return False
        except (OSError, ValueError, KeyError, TypeError):
            # Cleanup cannot turn an already committed input into a failed or
            # replayable submission. No clip paths or content enter the log.
            import logging
            logging.getLogger(__name__).warning('Committed voice cleanup deferred')
            return False

    def conversation_input_trace(self, ident):
        return self.presentation(f'input trace {self.trace_id(ident)}')

    @classmethod
    def trace_id(cls, ident):
        if isinstance(ident,str) and ident.startswith('telegram:'):
            value=ident.removeprefix('telegram:')
            if (not value.isascii() or not value.isdigit() or str(int(value))!=value
                    or not 0 <= int(value) <= 2**63-1):
                raise ValueError('Invalid Telegram trace identity')
            return ident
        return cls.input_id(ident)

    def conversation_telegram_inputs(self, before=0, direction='before'):
        if type(before) is not int or not 0 <= before <= 2**63-1 or direction not in ('before','after'):
            raise ValueError('Invalid Telegram observation cursor')
        return self.presentation(f'input telegram-list {before} {direction}')

    def conversation_trace_text(self, ident, source, offset):
        if type(offset) is not int or not 0 <= offset <= 32768:
            raise ValueError('Invalid trace text offset')
        selected = 'original' if source == 'original' else self.input_id(source)
        return self.presentation(f'input trace-text {self.trace_id(ident)} {selected} {offset}')

    @classmethod
    def clarification_source(cls, value, choices=None, entry_binding=None, binding=None):
        if (value.get('clarifiable') is not True or value.get('phase')!='awaiting_target'
                or value.get('retired') is not False or value.get('routing')):
            return None
        ident = cls.input_id(value['id'])
        choices = value.get('routing_choices') if choices is None else choices
        if not isinstance(choices,list) or len(choices)>128:
            raise ProviderUnavailable('Destination choices unavailable')
        if any(not isinstance(item,dict) or 'label' not in item for item in choices):
            raise ProviderUnavailable('Destination labels unavailable')
        labels = [item['label'] for item in choices]
        if any(not isinstance(label,str) or not label.strip() or len(label)>200 for label in labels):
            raise ProviderUnavailable('Destination labels unavailable')
        original = value.get('text',value.get('preview'))
        if not isinstance(original,str) or not original.strip():
            raise ProviderUnavailable('Pending original unavailable')
        selected = value.get('entry_binding') if entry_binding is None else entry_binding
        generation = value.get('binding') if binding is None else binding
        import re
        if (not isinstance(selected,list) or len(selected)!=4 or not isinstance(generation,str)
                or not re.fullmatch('[a-f0-9]{64}',generation)):
            raise ProviderUnavailable('Pending conversation generation unavailable')
        excerpt = original[:240]+('…' if len(original)>240 else '')
        text = f'Where should I send your message: “{excerpt}”?'
        if labels:
            text += (' Some available conversations are: ' if len(labels)>8 else ' Available conversations are: ')+', '.join(labels[:8])+'.'
        text += ' You can name a conversation or discuss a different task.'
        view = {'id':'input:'+ident,'kind':'route_clarification','label':'Destination needed',
            'entry_binding':selected[:3]+[generation],
            'pending_input':ident,'text':text,'current':True,'playback':'not_observed'}
        view['revision'] = hashlib.sha256(json.dumps([view,choices],sort_keys=True,ensure_ascii=False,separators=(',',':')).encode()).hexdigest()
        return view

    @classmethod
    def decorate_input(cls, value, choices=None, entry_binding=None, binding=None):
        source = cls.clarification_source(value,choices,entry_binding,binding)
        if source is not None:
            value['clarification'] = {k:source[k] for k in ('id','revision','kind','label','current')}
        return value

    def conversation_input_submit(self, body):
        if not isinstance(body, dict) or not {'binding','id','text'} <= set(body) or set(body)-{'binding','id','text','reply','visual'}:
            raise ValueError('Select the conversation and provide one original message')
        self.input_id(body['id'])
        self.check_reply(body.get('reply'))
        from .visual import check_reference
        check_reference(body.get('visual'))
        self.check_visual_original(body.get('visual'),body['id'])
        import re
        if (not isinstance(body['binding'], str) or not re.fullmatch('[a-f0-9]{64}', body['binding'])
                or not isinstance(body['text'], str) or not body['text'].strip()
                or '\0' in body['text'] or len(body['text'].encode('utf-8')) > 32768):
            raise ValueError('A message must contain text and fit within 32 KiB')
        # The Rust journal owns idempotency and exact-source checks. No retry.
        return self.decorate_input(self.presentation('input submit ' + json.dumps(body, ensure_ascii=False)))

    def check_visual_original(self,value,ident):
        if value is None:return
        self.assert_binding()
        if not self.authorized(self.human):raise ProviderUnavailable('Conversation unavailable')
        from .visual import check_reference,read_png,png_dimensions,VisualSelectionInvalid
        with closing(readonly(self.receiver['db_path'])) as db:
            existing=db.execute('SELECT 1 FROM conversation_inputs WHERE id=? AND human_id=?',(ident,self.human)).fetchone()
        if not existing:
            check_reference(value,fresh=True)
            path=Path(str(self.receiver['db_path'])+'.visual')/value['image_sha256']
            try:
                data=read_png(path,value['image_sha256'])
                if png_dimensions(data)!=(value['width'],value['height']):raise ValueError('Selected dimensions changed')
            except (ValueError,OSError):raise VisualSelectionInvalid('Selected image unavailable. Select it again.') from None

    def conversation_visual_upload(self, data, expected):
        import re
        if not isinstance(expected,str) or not re.fullmatch('[a-f0-9]{64}',expected):
            raise ValueError('Select immutable image bytes')
        self.assert_binding()
        if not self.receiver.get('interaction') or not self.authorized(self.human):
            raise ProviderUnavailable('Conversation unavailable')
        from .visual import retain_png
        result=retain_png(self.receiver['db_path'],data,expected)
        self.assert_binding()
        if not self.authorized(self.human):raise ProviderUnavailable('Conversation grant changed')
        return result

    def conversation_visual_image(self, ident):
        self.input_id(ident)
        original=self.conversation_input_status(ident)
        ref=original.get('visual')
        if not isinstance(ref,dict):raise ValueError('No selected image')
        from .visual import read_png
        return read_png(Path(str(self.receiver['db_path'])+'.visual')/ref['image_sha256'],ref['image_sha256'])

    @classmethod
    def check_reply(cls, value):
        import re
        if value is None: return
        if (not isinstance(value, dict) or set(value) not in ({'source', 'revision'}, {'source', 'revision', 'context_only'})
                or 'context_only' in value and value['context_only'] is not True
                or not isinstance(value['source'], str) or not value['source'].startswith(('delivery:', 'input:', 'request:', 'matrix:'))
                or not isinstance(value['revision'], str) or not re.fullmatch('[a-f0-9]{64}', value['revision'])):
            raise ValueError('Select one retained original or worker result')
        if value.get('context_only') and not value['source'].startswith(('request:', 'delivery:', 'input:', 'matrix:')):
            raise ValueError('Context must refer to a reported conversation source')
        if value['source'].startswith('matrix:') and not value.get('context_only'):
            raise ValueError('Matrix replies are selected contextual evidence')
        ident=value['source'].split(':', 1)[1]
        if value['source'].startswith('request:'):
            if not re.fullmatch('[a-f0-9]{32}',ident):
                raise ValueError('Select one current question')
        else:
            cls.input_id(ident)

    def conversation_input_route(self, ident, body):
        import re
        self.input_id(ident)
        if (not isinstance(body, dict) or set(body) != {'binding', 'selector'}
                or not isinstance(body['binding'], str) or not re.fullmatch('[a-f0-9]{64}', body['binding'])
                or not isinstance(body['selector'], str) or not 1 <= len(body['selector']) <= 128):
            raise ValueError('Select one destination for this original message')
        return self.decorate_input(self.presentation('input route '+json.dumps({**body, 'id':ident})))

    def conversation_input_retry(self, ident, body):
        import re
        self.input_id(ident)
        if (not isinstance(body, dict) or set(body) != {'binding', 'instruction'}
                or not isinstance(body['binding'], str) or not re.fullmatch('[a-f0-9]{64}', body['binding'])):
            raise ValueError('Select one saved, definitely-unsent route')
        self.input_id(body['instruction'])
        return self.decorate_input(self.presentation('input retry '+json.dumps({**body, 'id':ident})))

    def conversation_voice_submit(self, metadata, data):
        import re
        if not isinstance(metadata,dict) or not {'binding','id','audio_sha256','mime'} <= set(metadata) or set(metadata)-{'binding','id','audio_sha256','mime','reply','visual'}:
            raise ValueError('Select one original voice recording')
        self.input_id(metadata['id'])
        self.check_reply(metadata.get('reply'))
        from .visual import check_reference
        check_reference(metadata.get('visual'))
        self.check_visual_original(metadata.get('visual'),metadata['id'])
        if (not isinstance(data, bytes) or not 0 < len(data) <= 20*1024*1024
                or any(not isinstance(metadata[k], str) or not re.fullmatch('[a-f0-9]{64}', metadata[k]) for k in ('binding', 'audio_sha256'))
                or metadata['mime'] not in ('audio/webm', 'audio/webm;codecs=opus', 'audio/ogg', 'audio/ogg;codecs=opus', 'audio/mp4', 'audio/wav', 'audio/mpeg')
                or hashlib.sha256(data).hexdigest() != metadata['audio_sha256']):
            raise ValueError('Invalid bounded voice recording')
        self.assert_binding()
        if not self.receiver.get('voice') or not self.authorized(self.human):
            raise ProviderUnavailable('Voice input unavailable')
        directory = Path(str(self.receiver['db_path'])+'.voice')
        directory.mkdir(mode=0o700, parents=False, exist_ok=True)
        info = directory.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise ProviderUnavailable('Voice recording storage unavailable')
        # Persist the newly created directory name before retaining any clip/capture.
        sync_directory(directory.parent)
        self.retain_voice_capture(directory, metadata['id'], data, metadata['audio_sha256'])
        # The controller reserves original order/authority before asynchronous ASR.
        # A retry names the same clip and only inspects existing capture state.
        value = self.decorate_input(self.presentation('input voice submit '+json.dumps(metadata)))
        self.retire_voice_capture(value)
        return value

    @staticmethod
    def retain_voice_capture(directory, ident, data, digest):
        """Bound unresolved originals without expiring or discarding any input."""
        from scripts.speech_render import lock, RenderError
        import secrets
        # Same budget for every receiving worker/process. Completed transcripts
        # release their original bytes through retire_voice_capture above.
        budget = 256 * 1024 * 1024
        try:
            with lock(directory / 'admission.lock'):
                destination = directory / ident
                try:
                    fd = os.open(destination, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                except FileNotFoundError:
                    fd = None
                if fd is not None:
                    with os.fdopen(fd, 'rb') as stream:
                        info = os.fstat(stream.fileno())
                        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
                                or info.st_mode & 0o077):
                            raise ProviderUnavailable('Voice recording storage changed')
                        if hashlib.sha256(stream.read(20*1024*1024+1)).hexdigest() != digest:
                            raise ValueError('Original voice identity cannot be replaced')
                    sync_directory(directory)  # Finish an interrupted publication barrier.
                    return  # An exact retained retry needs no new space.
                used = 0
                entries = 0
                inodes = set()
                for path in directory.iterdir():
                    try: info = path.lstat()
                    except FileNotFoundError: continue  # Confirmed cleanup may run concurrently.
                    if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
                            or info.st_mode & 0o077):
                        raise ProviderUnavailable('Voice recording storage changed')
                    if path.name != 'admission.lock':
                        entries += 1
                    key = (info.st_dev, info.st_ino)
                    if key not in inodes:
                        used += info.st_size
                        inodes.add(key)
                    if entries >= 128 or used + len(data) > budget:
                        raise ProviderUnavailable('Voice recording storage full; use text')
                temporary = directory / ('.'+secrets.token_hex(16))
                try:
                    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
                    with os.fdopen(fd, 'wb') as stream:
                        stream.write(data); stream.flush(); os.fsync(stream.fileno())
                    os.link(temporary, destination, follow_symlinks=False)
                    sync_directory(directory)
                finally:
                    temporary.unlink(missing_ok=True)
        except RenderError:
            raise ProviderUnavailable('Voice recording storage busy; check original status') from None

    @staticmethod
    def request_id(ident):
        import re
        if not isinstance(ident, str) or not re.fullmatch('[a-f0-9]{32}', ident):
            raise ValueError('Invalid native request identity')
        return ident

    def conversation_requests(self, after=0):
        if type(after) is not int or not 0 <= after <= 2**63 - 1:
            raise ValueError('Invalid native request cursor')
        return self.presentation(f'requests list {after}')

    def conversation_tasks(self, after=0):
        if type(after) is not int or not 0<=after<=2**63-1:
            raise ValueError('Invalid task cursor')
        return self.presentation(f'requests tasks list {after}')

    def conversation_task_detail(self, ident, offset=0, revision=None):
        import re
        self.request_id(ident)
        if type(offset) is not int or not 0<=offset<=10**9:
            raise ValueError('Invalid task text offset')
        if revision is not None and (not isinstance(revision,str) or not re.fullmatch('[a-f0-9]{64}',revision)):
            raise ValueError('Invalid task revision')
        if offset and revision is None:
            raise ValueError('Select the task revision before continuing')
        suffix=' '+revision if revision is not None else ''
        return self.presentation(f'requests tasks detail {ident} {offset}{suffix}')

    def goal_controls(self, before=0):
        if type(before) is not int or not 0 <= before <= 2**63 - 1:
            raise ValueError('Invalid goal-control cursor')
        return self.presentation(f'presentation goals list {before}')

    def goal_control_detail(self, ident):
        import uuid
        try:
            if str(uuid.UUID(ident)) != ident:
                raise ValueError('Invalid goal-control identity')
        except (ValueError, TypeError, AttributeError):
            raise ValueError('Invalid goal-control identity') from None
        return self.presentation(f'presentation goals detail {ident}')

    def conversation_request_detail(self, ident, offset=0, revision=None):
        import re
        self.request_id(ident)
        if type(offset) is not int or not 0 <= offset <= 10**9:
            raise ValueError('Invalid request text offset')
        if revision is not None and (not isinstance(revision, str) or not re.fullmatch('[a-f0-9]{64}', revision)):
            raise ValueError('Invalid request revision')
        if offset and revision is None:
            raise ValueError('Select the request revision before continuing')
        suffix = ' ' + revision if revision is not None else ''
        return self.presentation(f'requests detail {ident} {offset}{suffix}')

    def conversation_decision_receipt(self, ident, revision):
        import re
        self.request_id(ident)
        if not isinstance(revision, str) or not re.fullmatch('[a-f0-9]{64}', revision):
            raise ValueError('Invalid decision revision')
        return self.presentation(f'requests receipt {ident} {revision}')

    def conversation_request_decide(self, ident, body):
        import re
        self.request_id(ident)
        if not isinstance(body, dict) or set(body) != {'binding', 'revision', 'action'}:
            raise ValueError('Select one exact request decision')
        for key in ('binding', 'revision'):
            if not isinstance(body[key], str) or not re.fullmatch('[a-f0-9]{64}', body[key]):
                raise ValueError('Invalid request binding or revision')
        action = body['action']
        if not isinstance(action, dict):
            raise ValueError('Invalid request action')
        kind = action.get('kind')
        valid = (
            kind == 'answer' and set(action) == {'kind', 'text'}
            and isinstance(action['text'], str) and action['text'].strip()
            and '\0' not in action['text'] and len(action['text'].encode('utf-8')) <= 32768
            or kind == 'option' and set(action) == {'kind', 'index'}
            and type(action['index']) is int and 0 <= action['index'] < 10
            or kind == 'approval' and set(action) == {'kind', 'choice'} and action['choice'] in ('a','s','d','c')
            or kind == 'cancel' and set(action) == {'kind'})
        if not valid:
            raise ValueError('Invalid request action')
        return self.presentation('requests decide ' + json.dumps({**body, 'id':ident}, ensure_ascii=False))

    @staticmethod
    def identity(row):
        return hashlib.sha256(json.dumps([row['id'], row['chat_id'], row['thread_id'],
                                          row['codex_thread_id']]).encode()).hexdigest()[:24]

    def stop_status(self):
        with closing(readonly(self.receiver['db_path'])) as db:
            row = db.execute('''SELECT id,phase,document FROM stop_operations WHERE human=?
                AND phase!='superseded' ORDER BY rowid DESC LIMIT 1''', (self.human,)).fetchone()
            if not row:
                return {'state': 'no_operation'}
            op = json.loads(row['document'])
            return {'operation': row['id'], 'state': row['phase'],
                    'targets': [{'state': t['phase'], 'detail': t['reason']} for t in op['targets']],
                    'external_processes': 'Native inactivity does not certify host subprocess inactivity'}

    def snapshot(self):
        if not self.lock.acquire(blocking=False):
            raise ProviderUnavailable('Metadata refresh in progress; retry shortly')
        try:
            rows = self.bindings()
            scope = [(self.identity(row), row['session_title']) for row in rows]
            if self.cache and time.time() - self.cache['observed_at'] < 8 and self.cache['_scope'] == scope:
                return {key: value for key, value in self.cache.items() if not key.startswith('_')}
            started = time.monotonic()
            self.refresh_started = started
            sessions = []
            native_db = readonly(self.native_home / 'state_5.sqlite')
            try:
                if self.native is None:
                    self.native = Native(self.receiver['codex'].get('binary', 'codex'))
            except Exception:
                self.close()
            for row in rows:
                tid = row['codex_thread_id']
                node = {'id': self.identity(row), 'title': row['session_title'] or 'Untitled session',
                        'status': 'unknown', 'bridge_busy': bool(row['busy']), 'goal_state': 'unavailable',
                        'children': [], 'children_state': 'unavailable', 'elapsed_seconds': None}
                if row['chat_id'] == self.human and row['thread_id'] > 0:
                    username = self.config.get('bot_username')
                    if username:
                        node['topic_url'] = f"https://t.me/{username}"
                        node['topic_link_scope'] = 'bot_chat'
                elif str(row['chat_id']).startswith('-100') and row['thread_id'] > 0:
                    node['topic_url'] = f"https://t.me/c/{str(row['chat_id'])[4:]}/{row['thread_id']}"
                try:
                    if tid and self.native and time.monotonic() - started < 12:
                        self.describe(node, tid, native_db)
                except Exception:
                    self.close()
                sessions.append(node)
            native_db.close()
            # A binding changed during native reads must not leak a retired conversation.
            if scope != [(self.identity(row), row['session_title']) for row in self.bindings()]:
                raise ProviderUnavailable('Session bindings changed; refresh again')
            result = {'schema': 'telecodex.sessions-dashboard/v1', 'observed_at': time.time(),
                      'provider': self.config.get('label', 'This receiving bot'), 'sessions': sessions,
                      'host': self.host.sample(), 'stop': self.stop_status(), '_scope': scope}
            self.cache = result
            self.readers = {key: reader for key, reader in self.readers.items()
                            if key in {row['codex_thread_id'] for row in rows}}
            return {key: value for key, value in result.items() if not key.startswith('_')}
        finally:
            self.lock.release()

    def describe(self, node, tid, db):
        node['status'] = self.native.status(tid).get('type', 'unknown')
        response = self.native.rpc('thread/goal/get', {'threadId': tid})
        if response is not None and 'goal' in response:
            goal = response['goal']
            if goal is None:
                node['goal_state'] = 'none'
            elif goal.get('threadId') == tid:
                node['goal_state'] = 'known'
                node['goal'] = {key: goal.get(key) for key in ('status', 'tokensUsed', 'tokenBudget', 'timeUsedSeconds')}
                node['goal']['objective'] = str(goal.get('objective', ''))[:180]
            else:
                raise ProviderUnavailable('Goal binding mismatch')
        row = db.execute('SELECT rollout_path FROM threads WHERE id=? AND archived=0', (tid,)).fetchone()
        if row:
            path = Path(row['rollout_path']).resolve()
            if not path.is_relative_to((self.native_home / 'sessions').resolve()):
                raise ProviderUnavailable('Rollout outside receiving native sessions')
            reader = self.readers.get(tid)
            if reader is None or reader.path != path:
                reader = self.readers[tid] = Rollout(path, tid)
            reader.refresh()
            at = timestamp(reader.started)
            if node['status'] == 'active' and at is not None:
                node['elapsed_seconds'] = max(0, int(time.time() - at))
            node['tools_active'] = len(reader.calls) if node['status'] == 'active' else 0
            # Existing observer validates each referenced child's rooted native ancestry.
            frontier, seen, complete = [tid], {tid}, True
            for _ in range(8):
                if not frontier:
                    break
                next_frontier = []
                for parent in frontier:
                    candidates = db.execute('''SELECT id,agent_nickname FROM threads WHERE archived=0
                        AND json_extract(CASE WHEN json_valid(source) THEN source ELSE NULL END,
                        '$.subagent.thread_spawn.parent_thread_id')=? ORDER BY id LIMIT 33''', (parent,)).fetchall()
                    for candidate in candidates:
                        child = candidate['id']
                        if len(seen) > 32 or time.monotonic() - self.refresh_started >= 12:
                            complete = False
                            break
                        if child in seen or not linked_child(db, child, tid):
                            continue
                        seen.add(child)
                        next_frontier.append(child)
                        node['children'].append({'id': child, 'parent_id': parent,
                            'title': candidate['agent_nickname'] or 'Subagent',
                            'status': self.native.status(child).get('type', 'unknown')})
                frontier = next_frontier
            if frontier:
                complete = False
            node['native_id'] = tid
            node['children_state'] = 'observed_native_index' if complete else 'partial'
            node['children_limit'] = 'Receiving native index, up to 32 descendants and 8 levels; archived children excluded'

    def stop(self, nonce):
        self.assert_binding()
        if not self.authorized(self.human):
            raise ProviderUnavailable('Receiving owner grant no longer authorizes Stop')
        return control(self.receiver['session_control']['socket_path'], nonce)

    def resume(self, nonce, operation):
        self.assert_binding()
        if not self.authorized(self.human):
            raise ProviderUnavailable('Receiving owner grant no longer authorizes Resume')
        observed = self.stop_status()
        if observed.get('operation') != operation or observed.get('state') != 'stopped':
            raise ProviderUnavailable('Selected Stop is no longer the observed stopped operation')
        self.assert_binding()
        # The native coordinator rechecks owned targets, bindings and stopped state.
        return control(self.receiver['session_control']['socket_path'], nonce, 'resume ' + operation)
