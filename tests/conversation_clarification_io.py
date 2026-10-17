#!/usr/bin/env python3
"""Later text/voice selects a retained original; actual HTTP/SQLite, zero models."""
import json
from pathlib import Path
import sqlite3
import sys
import uuid
from conversation_cold_entry_http_io import ColdEntry, Browser


def qualify(binary):
    j = ColdEntry(str(Path(binary).resolve())); b = None
    try:
        j.prepare(); b = Browser(j)
        early, _ = b.text('Discuss the independent work while keeping originals distinct.')
        j.wait(lambda: b.status(early)['phase'] == 'accepted')
        caps = j.call('route-capabilities', 'capabilities', {})
        assert 'pending_routes' in caps['operations'] and 'resolve_route' in caps['operations']
        text = '#Unknown Keep the exact original requirement13 and its authorship.'
        pending, result = b.text(text); assert result['phase'] == 'awaiting_target'
        inventory = j.call('route-pending', 'pending_routes', {})
        assert [v['id'] for v in inventory['items']] == [pending]
        assert inventory['items'][0]['text_excerpt'] == text
        def choose(tag, original, reference, selector, ok=True):
            return j.call('route-' + tag, 'resolve_route',
                          {'pending_input': original, 'input_ref': reference, 'selector': selector}, ok=ok)
        choose('old-input', pending, early, 'Alpha', False)
        choose('invented-input', pending, str(uuid.uuid4()), 'Alpha', False)
        selected, _ = b.text('Use Alpha for that saved request, please.')
        j.wait(lambda: b.status(selected)['phase'] == 'accepted')
        choose('missing-target', pending, selected, 'Unknown', False)
        assert b.status(pending)['phase'] == 'awaiting_target' and not j.instructions(7)
        resolved = choose('text', pending, selected, 'Alpha')
        j.wait(lambda: j.instructions(7) and j.instructions(7)[0][1] == 'accepted')
        assert resolved['routing']['selection'] == 'integrator_clarification'
        assert resolved['routing']['selection_input'] == selected
        assert b.status(pending)['text'] == text
        assert text in json.loads(j.instructions(7)[0][2])['text']
        packets = sum(map(len, j.worker_packets.values()))
        choose('same-choice', pending, selected, 'Alpha')
        choose('changed-target', pending, selected, 'Beta', False)
        newer, _ = b.text('Alpha is still the intended destination.')
        j.wait(lambda: b.status(newer)['phase'] == 'accepted')
        choose('changed-reference', pending, newer, 'Alpha', False)
        assert sum(map(len, j.worker_packets.values())) == packets
        assert not j.call('route-resolved-inventory', 'pending_routes', {})['items']

        # A finalized ASR interpretation is a separately attributed human source.
        script = j.root / 'asr.py'
        script.write_text(script.read_text().replace('Continue requirement17 in this selected work.',
                                                    'Use Beta for the previous saved message.'))
        second, _ = b.text('#Unknown Work independently on requirement23.')
        spoken, _ = b.voice()
        j.wait(lambda: b.status(spoken)['phase'] == 'accepted')
        assert b.status(spoken)['voice']['transcription']['capture_id'] == spoken
        choose('voice', second, spoken, 'Beta')
        j.wait(lambda: j.instructions(8) and j.instructions(8)[0][1] == 'accepted')
        assert b.status(second)['routing']['selection_input'] == spoken
        assert b.status(spoken)['text'] == 'Use Beta for the previous saved message.'

        # Completion order never makes an earlier recording a later clarification.
        (j.root / 'hold-asr').touch()
        delayed, _ = b.voice()
        third, _ = b.text('#Unknown Later work must not use an older recording as clarification.')
        (j.root / 'release-asr').touch()
        j.wait(lambda: b.status(delayed)['phase'] == 'accepted')
        choose('late-old-asr', third, delayed, 'Alpha', False)
        final, _ = b.text('Send the latest saved request to Alpha as well.')
        j.wait(lambda: b.status(final)['phase'] == 'accepted')
        choose('busy-worker', third, final, 'Alpha')
        j.wait(lambda: len(j.instructions(7)) == 2 and j.instructions(7)[1][1] == 'accepted')
        assert len(j.worker_packets['native-topic-7']) == 2

        # A genuine retained Telegram ingress can clarify a browser original;
        # it keeps its integer reference instead of fabricating a browser UUID.
        cross_channel, _ = b.text('#Unknown Keep this original across its clarification channel.')
        j.hold_telegram = True
        telegram_choice = j.admit('Use Beta for that saved original.', topic=1)
        j.wait(lambda: j.row("SELECT status FROM incoming_updates WHERE update_id=?", (telegram_choice,)) in
               ([('steering',)], [('started',)], [('settled',)]))
        choose('telegram-clarification', cross_channel, telegram_choice, 'Beta')
        j.wait(lambda: len(j.instructions(8)) == 2 and j.instructions(8)[1][1] == 'accepted')
        assert b.status(cross_channel)['routing']['selection_input'] == telegram_choice
        j.hold_telegram = False

        fourth, _ = b.text('#Unknown Preserve this one across grant revocation.')
        last, _ = b.text('Use Alpha for this most recent saved request.')
        j.wait(lambda: b.status(last)['phase'] == 'accepted')
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            db.execute('UPDATE users SET allowed=0 WHERE tg_user_id=100')
            db.execute('UPDATE users SET allowed=1 WHERE tg_user_id=100')
        choose('retired-authority', fourth, last, 'Alpha', False)
        assert len(j.instructions(7)) == 2 and not j.instructions(77)
        assert j.topic_calls == 0
        # Actual Telegram ingress may announce the existing selected conversation.
        # It must not fabricate a routed worker result on that transport.
        assert [payload['text'] for _, payload in j.messages] == [
            'Current Codex session: <b>Cold conversation</b>'], j.messages
        j.stop(); j.start(); j.wait_control_socket(j.socket)
        assert b.status(pending)['routing']['selection_input'] == selected
        assert b.status(second)['routing']['selection_input'] == spoken
        assert len(j.instructions(7)) == 2 and len(j.instructions(8)) == 2
        print('PASS actual HTTP/controller/native/SQLite conversational target clarification: exact original preserved, later text/voice attribution, immutable choice/retry, old-ASR capture-order refusal, busy worker steer, grant ABA and restart without replay; zero models.')
    finally:
        if b: b.client.close()
        j.close()


if __name__ == '__main__':
    qualify(sys.argv[1])
