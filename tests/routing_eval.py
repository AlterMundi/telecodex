#!/usr/bin/env python3
"""Evaluation integrity: missing rows or bad metadata cannot become good scores."""
import copy
import importlib.util
import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('routing_eval', ROOT/'scripts/routing_eval.py')
evaluation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(evaluation)


class Integrity(unittest.TestCase):
    def setUp(self):
        self.data = evaluation.load(ROOT/'tests/fixtures/routing-eval-synthetic.json')
        self.answers = {'engine': 'human-labeled oracle for scoring checks only', 'decisions': [
            {'id': c['id'], 'decision': copy.deepcopy(c['acceptable'][0])} for c in self.data['cases']]}

    def test_exact_oracle_and_missing_measurements(self):
        report = evaluation.score(self.data, self.answers)
        self.assertEqual(report['accuracy'], 1)
        self.assertEqual(report['counts']['wrong_target'], 0)
        self.assertIsNone(report['latency_ms']['median'])
        self.assertIsNone(report['api_cost_estimate_usd']['sum'])
        self.assertEqual(report['source_counts'], {'synthetic': 24})
        self.assertFalse(report['calibrated_probability'])
        self.assertEqual(report['dispatches'], 0)

    def test_missing_duplicate_extra_predictions_reject(self):
        for rows in [self.answers['decisions'][:-1], self.answers['decisions']+[self.answers['decisions'][0]],
                     self.answers['decisions']+[{'id': 'unexpected', 'decision': {'action': 'local', 'targets': []}}]]:
            with self.assertRaises(ValueError):
                evaluation.score(self.data, {'engine': 'fixture', 'decisions': rows})

    def test_wrong_target_creation_and_clarification_are_distinct(self):
        rows = self.answers['decisions']
        rows[0]['decision'] = {'action': 'route', 'targets': ['field']}
        rows[1]['decision'] = {'action': 'new', 'targets': []}
        rows[2]['decision'] = {'action': 'clarify', 'targets': []}
        report = evaluation.score(self.data, self.answers)
        self.assertEqual(report['counts'], {'correct': 21, 'wrong_target': 1, 'unnecessary_creation': 1, 'unnecessary_clarification': 1})

    def test_bad_labels_and_fabricated_measurements_reject(self):
        for value in [{'action': 'route', 'targets': ['unknown']}, {'action': 'local', 'targets': ['field']},
                      {'action': 'route', 'targets': ['field', 'field']}]:
            with self.assertRaises(ValueError):
                evaluation.decision(value, {'field'})
        for value in [True, -1, float('nan'), float('inf')]:
            altered = copy.deepcopy(self.answers)
            altered['decisions'][0]['latency_ms'] = value
            with self.assertRaises(ValueError):
                evaluation.score(self.data, altered)

    def test_export_has_no_ground_truth_and_baseline_is_complete(self):
        projection = evaluation.inputs(self.data)
        self.assertTrue(all('acceptable' not in c and 'source' not in c for c in projection['cases']))
        report = evaluation.score(self.data, evaluation.baseline(self.data))
        self.assertEqual(report['cases'], len(self.data['cases']))
        self.assertEqual(report['latency_ms']['measured_cases'], report['cases'])


if __name__ == '__main__':
    unittest.main()
