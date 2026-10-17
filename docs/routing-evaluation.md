# Routing evaluation

`scripts/routing_eval.py` compares retained routing proposals without dispatching
messages or calling providers. It covers target selection, clarification,
justified creation and local conversation. It does not establish correction
incorporation, native delivery or production latency.

Start with the public synthetic contrast set; it is not a transcript or a set of
real human corrections. Later selected private correction sets use the same schema
and must retain a source reference and audience. Keep private cases and outputs
outside the repository. Do not import unrelated sessions into this dataset.

```sh
python3 -B scripts/routing_eval.py tests/fixtures/routing-eval-synthetic.json --baseline
python3 -B scripts/routing_eval.py tests/fixtures/routing-eval-synthetic.json --export-inputs > /private/unlabeled-inputs.json
python3 -B scripts/routing_eval.py tests/fixtures/routing-eval-synthetic.json --decisions /private/retained-decisions.json
```

The export excludes ground-truth labels. Give that projection to the selected
model/classifier and retain its exact engine/configuration and one decision per
case. Do not show labels to the evaluator. External execution is explicit and
separate; this script never starts a native session, downloads a model, reads
credentials, retries an inference or routes its own output. Use the same input
set for the general model and classifier/embedding candidates.

The sparse TF-IDF policy baseline is an inexpensive reference. Its thresholds are
uncalibrated; it intentionally lacks semantic/coreference/intent judgment and is
not a production selector. Busy workers do not justify creation. Quoted tags do
not force routing. Production reply/selector precedence remains controller-owned.

Reports separate exact acceptable decisions, wrong destinations, unnecessary
creation and clarification burden. Missing/duplicate/unknown predictions reject
rather than inflate accuracy. Optional per-decision latency and attributed API
cost estimates remain missing when unmeasured; a batch duration must not be
reported as individual decision latency. Cost estimates are not verified billing
and exclude unmeasured host/audio costs. Result reports identify the dataset hash
and synthetic/human source counts; they do not assert calibrated probabilities
or real human acceptance.

Run `python3 -B tests/routing_eval.py` for scoring-integrity checks. This is the
measurement surface for #115, not completion of its human-correction corpus,
model comparison or receiving acceptance. Parent: AlterMundi/daimon-matrix#313.
