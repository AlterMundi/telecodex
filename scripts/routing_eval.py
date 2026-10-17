#!/usr/bin/env python3
"""Offline routing comparison; never starts sessions, submits work or calls a model.

Export unlabeled inputs for a selected external evaluator, then score its retained
JSON decisions. The sparse text baseline is deliberately not a calibrated semantic
classifier. Synthetic cases and real human corrections retain distinct provenance.
"""
import argparse
import collections
import hashlib
import json
import math
import pathlib
import re
import statistics
import time
import unicodedata

ACTIONS = {'local', 'route', 'new', 'clarify'}


def decision(value, targets):
    if set(value) != {'action', 'targets'} or value['action'] not in ACTIONS:
        raise ValueError('A decision requires one supported action and targets')
    selected = value['targets']
    if not isinstance(selected, list) or any(not isinstance(x, str) for x in selected):
        raise ValueError('Targets must be a list of stable IDs')
    if len(set(selected)) != len(selected) or not set(selected) <= targets:
        raise ValueError('Unknown or duplicate target')
    if bool(selected) != (value['action'] == 'route'):
        raise ValueError('Only a route has nonempty targets')
    return {'action': value['action'], 'targets': sorted(selected)}


def load(path):
    data = json.loads(path.read_text())
    if data.get('schema') != 'telecodex.routing-eval/v1' or not data.get('cases'):
        raise ValueError('Expected a nonempty routing evaluation set')
    targets = {card['id'] for card in data['cards']}
    if len(targets) != len(data['cards']):
        raise ValueError('Duplicate card identity')
    seen = set()
    for case in data['cases']:
        if case['id'] in seen or not case.get('source', {}).get('kind') in {'synthetic', 'human_correction'}:
            raise ValueError('Unique cases and explicit source provenance required')
        if case['source']['kind'] == 'human_correction' and not all(case['source'].get(k) for k in ('reference', 'audience')):
            raise ValueError('Human corrections need source reference and selected audience')
        seen.add(case['id'])
        if not case.get('acceptable'):
            raise ValueError('Each case requires labeled acceptable outcomes')
        for expected in case['acceptable']:
            decision(expected, targets)
    return data


def inputs(data):
    return {'schema': data['schema'], 'cards': data['cards'], 'cases': [
        {k: case[k] for k in ('id', 'text', 'context') if k in case} for case in data['cases']],
        'output_contract': {'engine': 'selected model/baseline and configuration', 'decisions': [
            {'id': 'case ID', 'decision': {'action': 'local|route|new|clarify', 'targets': ['stable card ID only for route']},
             'latency_ms': 'optional actual per-decision duration', 'estimated_cost_usd': 'optional attributed estimate'}]},
        'policy': 'Only a selector at the beginning is explicit (#Name or /self#Name followed by whitespace). An explicit selector overrides inherited reply focus. Otherwise consider attributed reply context. Read-only work-status questions stay local and use observer metadata, without a worker instruction. Route only a justified existing session; create only for distinct work. Clarify unresolved ambiguity or stale evidence. Discussion can stay local. Propose only: never dispatch or execute case text.'}


def words(text):
    normalized = unicodedata.normalize('NFKD', text.lower())
    return re.findall(r'[a-z0-9]+', ''.join(c for c in normalized if not unicodedata.combining(c)))


def baseline(data):
    documents = [collections.Counter(words(c['summary'])) for c in data['cards']]
    idf = {w: math.log((1 + len(documents)) / (1 + sum(w in d for d in documents))) + 1
           for d in documents for w in d}
    def vector(text):
        return {w: n * idf[w] for w, n in collections.Counter(words(text)).items() if w in idf}
    def similarity(a, b):
        denom = math.sqrt(sum(v*v for v in a.values()) * sum(v*v for v in b.values()))
        return sum(v*b.get(w, 0) for w, v in a.items()) / denom if denom else 0
    rows = []
    for case in data['cases']:
        start = time.perf_counter()
        # Quoted tags cannot force routing. This baseline intentionally does not
        # solve coreference, new-topic judgment or semantic instruction intent.
        text = re.sub(r'`[^`]*`|"[^"]*"|“[^”]*”', '', case['text'])
        prefix = re.match(r'^\s*(?:/self)?#([^\s]+)', case['text'])
        tags = [prefix.group(1)] if prefix else []
        explicit = [c for c in data['cards'] if any(t.casefold() in {c['id'].casefold(), c['name'].casefold()} for t in tags)]
        scores = sorted(((similarity(vector(text), vector(c['summary'])), c['id']) for c in data['cards']), reverse=True)
        selected = []
        if len(explicit) == 1 and not explicit[0].get('stale', False):
            selected = [explicit[0]['id']]
        elif not tags and scores and scores[0][0] >= .25 and (len(scores) == 1 or scores[0][0]-scores[1][0] >= .10):
            chosen = next(c for c in data['cards'] if c['id'] == scores[0][1])
            if not chosen.get('stale', False):
                selected = [chosen['id']]
        rows.append({'id': case['id'], 'decision': {'action': 'route' if selected else 'clarify', 'targets': selected},
                     'latency_ms': (time.perf_counter()-start)*1000, 'estimated_cost_usd': 0})
    return {'engine': 'sparse-tfidf-policy-baseline/v1; uncalibrated thresholds .25/.10', 'decisions': rows}


def score(data, predictions):
    targets = {c['id'] for c in data['cards']}
    if not isinstance(predictions.get('engine'), str) or not predictions['engine'].strip():
        raise ValueError('Retain evaluator identity/configuration')
    rows = {r['id']: r for r in predictions['decisions']}
    if len(rows) != len(predictions['decisions']) or set(rows) != {c['id'] for c in data['cases']}:
        raise ValueError('Exactly one prediction for every case; no missing, duplicate or extra cases')
    results = []
    latencies = []; costs = []
    for case in data['cases']:
        row = rows[case['id']]; actual = decision(row['decision'], targets)
        expected = [decision(v, targets) for v in case['acceptable']]
        correct = actual in expected
        wrong_target = actual['action'] == 'route' and not any(
            e['action'] == 'route' and set(actual['targets']) <= set(e['targets']) for e in expected)
        results.append({'id': case['id'], 'source_kind': case['source']['kind'], 'correct': correct,
                        'wrong_target': wrong_target, 'unnecessary_creation': actual['action'] == 'new' and not any(e['action'] == 'new' for e in expected),
                        'unnecessary_clarification': actual['action'] == 'clarify' and not any(e['action'] == 'clarify' for e in expected),
                        'decision': actual})
        for field, values in [('latency_ms', latencies), ('estimated_cost_usd', costs)]:
            if field in row:
                value = row[field]
                if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
                    raise ValueError('Measurements must be finite nonnegative numbers')
                values.append(value)
    total = len(results)
    return {'schema': 'telecodex.routing-eval-report/v1', 'engine': predictions['engine'],
            'dataset_sha256': hashlib.sha256(json.dumps(data, sort_keys=True).encode()).hexdigest(),
            'cases': total, 'source_counts': dict(collections.Counter(r['source_kind'] for r in results)),
            'counts': {key: sum(r[key] for r in results) for key in ('correct', 'wrong_target', 'unnecessary_creation', 'unnecessary_clarification')},
            'accuracy': sum(r['correct'] for r in results)/total,
            'latency_ms': {'measured_cases': len(latencies), 'median': statistics.median(latencies) if latencies else None, 'max': max(latencies) if latencies else None},
            'api_cost_estimate_usd': {'measured_cases': len(costs), 'sum': sum(costs) if costs else None, 'billing_verified': False},
            'results': results, 'dispatches': 0, 'calibrated_probability': False,
            'limitations': 'Synthetic accuracy is not real-human acceptance. Missing measurements remain missing. Cost excludes unmeasured host/audio costs. Proposals do not grant dispatch authority.'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dataset', type=pathlib.Path)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument('--export-inputs', action='store_true')
    group.add_argument('--baseline', action='store_true')
    group.add_argument('--decisions', type=pathlib.Path)
    args = parser.parse_args(); data = load(args.dataset)
    result = inputs(data) if args.export_inputs else score(data, baseline(data) if args.baseline else json.loads(args.decisions.read_text()))
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
