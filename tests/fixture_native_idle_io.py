#!/usr/bin/env python3
"""A bounded connect timeout must not expire an idle synthetic native worker.

Use a short connection timeout and real controller/socket traffic so this
regression stays fast. All native peers, accounts and outputs are synthetic.
"""
import json
from pathlib import Path
import sys
import time

from conversation_cold_entry_http_io import ColdEntry, Browser


def qualify(binary):
    journey = ColdEntry(binary)
    browser = None
    try:
        proxy = journey.root / 'codex'
        proxy.write_text(proxy.read_text().replace('timeout=30)', 'timeout=0.2)'))
        journey.prepare()
        browser = Browser(journey)
        original, _ = browser.text('#Alpha Keep this work active while the caller is idle.')
        journey.wait(lambda: journey.instructions(7) and journey.instructions(7)[0][1] == 'accepted')
        first = json.loads(journey.instructions(7)[0][2])
        # More than three times the proxy's connection deadline, without another
        # event to keep it alive. Fixture-owned cleanup still bounds the process.
        time.sleep(0.7)
        assert journey.row('SELECT busy FROM sessions WHERE thread_id=7') == [(1,)], 'Idle proxy expired an ongoing worker'
        correction, _ = browser.text('#Alpha Incorporate this later input in the same running worker.')
        journey.wait(lambda: len(journey.instructions(7)) == 2 and journey.instructions(7)[1][1] == 'accepted')
        second = json.loads(journey.instructions(7)[1][2])
        assert second['native_turn'] == first['native_turn']
        assert len(journey.worker_packets['native-topic-7']) == 2
        assert browser.status(original)['routing']['phase'] == 'accepted'
        assert browser.status(correction)['routing']['phase'] == 'accepted'
        journey.answer_worker(7, 'Retained both inputs after idle.')
        journey.wait(lambda: journey.row('SELECT count(*) FROM interaction_deliveries') == [(2,)])
        journey.idle()
        print('PASS short connect deadline, idle worker survival, same-turn steer and correlated results; zero inference')
    finally:
        if browser:
            browser.client.close()
        journey.close()


if __name__ == '__main__':
    qualify(str(Path(sys.argv[1]).resolve()))
