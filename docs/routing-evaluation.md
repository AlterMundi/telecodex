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

## Initial API comparison — 2026-10-17

The aligned 24-case synthetic set has normalized dataset SHA-256
`ab62cef4ea2b85d40e9303c5c9770c621d5570aeb3bac8b76498357f30a4d40f`.
One unlabeled batch per general model was sent directly to Groq without tools;
responses, exact requests and token usage were retained locally. The scorer made
zero dispatches. This is an initial contrast set, not a held-out production
benchmark, calibrated classifier or real-human correction corpus.

| Evaluator | Exact acceptable decisions | Wrong target | Unnecessary clarification | Complete batch wall time | Provider tokens input/output |
| --- | ---: | ---: | ---: | ---: | ---: |
| Sparse TF-IDF policy baseline | 10/24 | 1 | 12 | Not an API batch | None |
| `openai/gpt-oss-20b`, low | 18/24 | 0 | 4 | 1.438 s | 1301/1154 |
| `qwen/qwen3.8-27b`, none | 24/24 | 0 | 0 | 2.395 s | 1221/1064 |

None proposed an unnecessary new session in this aligned run. Provider output
counts include reported reasoning tokens where applicable. API dollar cost and
billing were not established; these are small bounded requests. Batch wall time
is **not** per-message latency. Cases share a prompt, have only three topic cards,
and were authored by the implementer; perfect performance here does not establish
generalization, load behavior, absence of leakage across real contexts or model
selection. A learned classifier/embedding comparison and attributed human
corrections are still required by #115.

The first exploratory set had a reply-selector conflict mislabeled as
clarification, contrary to the existing controller's explicit-selector priority,
and mixed read-only status with work routing. After checking the owning contract,
labels, selector syntax and input policy were aligned once; the earlier requests,
results and failed model-access probes remain retained. No worker work was replayed.
Do not combine scores across those dataset/prompt versions. Historical Llama model
names returned HTTP 404 on this account; the actual model listing supplied the
accessible candidates above.

The response contract and reasoning settings were checked against the official
[Groq API reference](https://console.groq.com/docs/api-reference) and
[Qwen model interface](https://console.groq.com/docs/model/qwen/qwen3.8-27b).
These text requests establish text-model access, not Qwen realtime voice access.

## Learned embedding candidate — 2026-10-18

The same normalized 24-case dataset was evaluated with the already configured
NVIDIA `nvidia/nemotron-3-embed-1b` API. Three card passages and 24 input queries
were encoded in two bounded calls, with separate `passage` and `query` input types,
2048-dimensional float outputs and no truncation. The export includes the same
unlabeled cards, text and dialogue context as the general-model comparison;
labels and source annotations are excluded. Exact requests, responses, indices,
engine, input hash, timing and provider usage were retained. No prior general-model
request was repeated, and no worker task was dispatched.

| Candidate | Exact acceptable decisions | Wrong target | Unnecessary clarification |
| --- | ---: | ---: | ---: |
| Previously retained sparse TF-IDF reference | 10/24 | 1 | 12 |
| Previously retained GPT-OSS-20B, low | 18/24 | 0 | 4 |
| Previously retained Qwen3.8-27B, none | 24/24 | 0 | 0 |
| Nemotron-3-Embed-1B cosine reference | 14/24 | 3 | 7 |

The embedding reference uses the same **uncalibrated** `.25` minimum and `.10`
margin as the sparse reference, plus deliberate prefix selection and stale-card
refusal. These thresholds were frozen before encoding and were not fitted to
this set. The encoder is learned; the decision rule is a zero-shot retrieval
baseline, **not** a trained conversation classifier. It can propose one existing
candidate or clarification; it cannot decide discussion versus new work or
multi-target work. Including context in its query does not implement instruction
intent or coreference reasoning.

The three wrong routes were a quoted selector in a discussion, a question about
whether a busy worker justifies creation, and discussion about a possible new
research topic. This specific retrieval rule is insufficient as sole dispatch
selector. Keep the current integrating dialogue responsible for semantic
selection; embeddings remain a possible candidate-ranking optimization requiring
independent evaluation. This does not establish that learned classifiers cannot
work, that Qwen generalizes, or that any production provider should change.

Encoding took 0.390 seconds for the three passages and 0.774 seconds for the
24-query batch on this one run. The API reported 138 and 912 input tokens,
respectively. These are batch timings, **not** per-message latency or verified
billing. Passage encoding could be reused until cards change; that optimization
was not measured here. Human correction examples and calibrated evaluation on a
separate dataset remain open.

The official [model description](https://docs.api.nvidia.com/nim/reference/nvidia-nemotron-3-embed-1b)
and [API reference](https://docs.api.nvidia.com/nim/reference/nvidia-nemotron-3-embed-1b-infer)
describe the embedding interface. Successful access here proves only this
small public-synthetic text experiment, not paid receiving execution or a voice
service.

To reproduce scoring without another API request:

```sh
python3 -B scripts/routing_eval.py tests/fixtures/routing-eval-synthetic.json --export-embedding-inputs
python3 -B scripts/routing_eval.py tests/fixtures/routing-eval-synthetic.json --embeddings /private/retained-embeddings.json
```

The retained artifact uses `telecodex.routing-embeddings/v1`, an engine string,
the SHA-256 of `json.dumps(export, sort_keys=True).encode()`, and `passages`/`queries`
arrays of `{id, embedding}`. Each exact input must have one finite, nonzero vector
with a common dimension. The scorer rejects changed exports, missing/duplicate
IDs and malformed vectors. It never fits thresholds, reads credentials, calls an
encoder or dispatches a proposal. Similarity values are not probabilities.
