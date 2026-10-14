#!/usr/bin/env python3
"""Authenticated feedback through HTTP, SQLite and a synthetic native App Server.

These fixtures qualify native input bytes and receipts, not private Telegram
delivery or a live model's perception. No real credentials or accounts are used.
"""
import json
import pathlib
import sys

from questions_io import Journey
from reactions_io import reaction


def main(binary):
    journey = Journey(binary, 'reaction_feedback')
    observed = []

    def check_native(params):
        assert params['threadId'] == 'native-topic-7', params
        assert params['collaborationMode']['mode'] in ('default', 'plan'), params
        text = params['input'][0]['text']
        # Feedback is untrusted human data, never promoted into developer context.
        assert '[Telegram reaction feedback]' not in json.dumps(params.get('additionalContext', {}))
        observed.append(text)

    journey.check_turn_start = check_native
    journey.config.write_text(journey.config.read_text().replace(
        '[telegram]\n', '[telegram]\ncapture_message_reactions=true\n'))

    def starts():
        return sum(item.get('method') == 'turn/start' for item in journey.rpc)

    def wait_completed(count):
        # The first native request proves startup/schema initialization completed;
        # opening SQLite immediately after Popen can race CREATE TABLE.
        journey.wait(lambda: len(observed) >= count)
        journey.wait(lambda: journey.row("SELECT COUNT(*) FROM turns WHERE status='completed'") == [(count,)])
        journey.wait(lambda: journey.row('SELECT busy FROM sessions') == [(0,)])

    def emit(old, new, *, actor=100, target=None, chat=100):
        with journey.lock:
            journey.sequence += 1
            identifier = journey.sequence
            event = reaction(identifier, old, new, actor=actor, target=target or message_id, chat=chat)
            journey.updates.append(event)
        journey.wait(lambda: journey.row('SELECT status FROM incoming_updates WHERE update_id=?',
                                         (identifier,)) == [('handled',)])
        return event

    def packet(text):
        prefix = '[Telegram reaction feedback]\n'
        assert text.startswith(prefix), text
        encoded = text.split('\n', 2)[2].split('\n[/Telegram reaction feedback]\n\n', 1)[0]
        return json.loads(encoded)

    try:
        journey.start()
        journey.admit('/default implement after planning')
        wait_completed(1)
        binding = journey.row('SELECT codex_thread_id FROM sessions')
        message_id, source_turn, source_text = journey.row(
            'SELECT message_id,turn_id,text FROM handoff_messages ORDER BY message_id DESC LIMIT 1')[0]
        emoji = dict(type='emoji', emoji='👍')
        custom = dict(type='custom_emoji', custom_emoji_id='opaque-fixture-custom-id')
        first = emit([], [emoji])
        emit([emoji], [custom])
        removal = emit([custom], [])
        for actor in (101, 999, None):
            emit([], [emoji], actor=actor)
        emit([], [emoji], target=99999)
        emit([], [emoji], chat=200)
        assert journey.row("SELECT phase FROM reaction_feedback ORDER BY update_id") == [('pending',)] * 3
        assert starts() == 1
        assert journey.row('SELECT codex_thread_id FROM sessions') == binding
        assert not any(item.get('method') == 'turn/steer' for item in journey.rpc)

        journey.stop()
        with journey.lock:
            journey.updates.extend([first, removal])
        journey.start()
        # Commands leave feedback pending and cannot resume a goal through an emoji.
        journey.admit('/status')
        journey.wait(lambda: journey.row("SELECT status FROM incoming_updates WHERE update_id=?",
                                         (journey.sequence,)) == [('handled',)])
        assert journey.row("SELECT phase FROM reaction_feedback ORDER BY update_id") == [('pending',)] * 3
        journey.admit('Continue our conversation.')
        wait_completed(2)
        feedback = packet(observed[-1])
        assert [item['action'] for item in feedback] == ['added', 'changed', 'removed']
        assert feedback[0]['old_reaction'] == [] and feedback[0]['new_reaction'] == [emoji]
        assert feedback[1]['new_reaction'] == [custom]
        assert feedback[2]['old_reaction'] == [custom] and feedback[2]['new_reaction'] == []
        assert all(item['actor_user_id'] == 100 and item['chat_id'] == 100 and item['topic_id'] == 7
                   and item['assistant_message_id'] == message_id and item['assistant_turn_id'] == source_turn
                   and item['native_thread_id'] == binding[0][0]
                   and item['assistant_text_excerpt'] == source_text for item in feedback)
        assert observed[-1].endswith('Continue our conversation.'), observed[-1]
        assert journey.row('SELECT phase FROM reaction_feedback ORDER BY update_id') == [('accepted',)] * 3
        journey.stop(); journey.start()
        journey.admit('One more ordinary message.')
        wait_completed(3)
        assert '[Telegram reaction feedback]' not in observed[-1]

        # A known reaction during an armed native question remains pending. Text
        # answers the question; only a later ordinary turn carries the reaction.
        journey.admit('/plan ask a question')
        journey.wait(lambda: journey.controls(1))
        journey.admit(callback=journey.button(journey.controls(1), ':0'))
        journey.wait(lambda: journey.controls(2))
        journey.admit(callback=journey.button(journey.controls(2), ':text'))
        journey.wait(lambda: any('Send your answer as text' in item[1]['text'] for item in journey.messages))
        emit([], [emoji])
        assert not journey.answers
        assert starts() == 4
        assert not any(item.get('method') == 'turn/steer' for item in journey.rpc)
        journey.admit('Exact free text')
        journey.wait(lambda: len(journey.answers) == 1)
        assert journey.answers[0]['result']['answers']['notes']['answers'] == ['Exact free text']
        wait_completed(4)
        assert journey.row("SELECT COUNT(*) FROM reaction_feedback WHERE phase='pending'") == [(1,)]
        journey.admit('/default')
        journey.wait(lambda: journey.row('SELECT collaboration_mode FROM sessions') == [('default',)])
        journey.admit('Now continue normally.')
        wait_completed(5)
        assert len(packet(observed[-1])) == 1
        assert journey.row('SELECT codex_thread_id FROM sessions') == binding
        assert not journey.errors, journey.errors
        print('PASS: authenticated additions/changes/removals reach the same native thread as '
              'quoted human data once; restart/dedup, invalid actors/targets, commands and armed questions stay isolated')
    finally:
        journey.close()


if __name__ == '__main__':
    main(str(pathlib.Path(sys.argv[1]).resolve()))
