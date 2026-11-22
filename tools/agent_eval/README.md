# Provider-backed agent experiment executor

`executor.py` is a Python-standard-library Responses API adapter, raw-evidence
normalizer and launch gate. It is **prepared and offline-tested; no live
qualification has succeeded in the current environment**. A synthetic fixture,
a CLI turn aggregate or a smoke attempt with no model response cannot authorize
a participant run.

## Recorded contract

Every provider attempt writes `request.json`, `response.json` and `capture.json`.
The capture binds exact file bytes, experiment freeze, executor source and model
policy; records the client request ID, provider `x-request-id`, provider response
ID, monotonic interval, HTTP status and retry policy; and preserves the provider's
actual `usage` object. There is one HTTP attempt per call, with no automatic retry
or redirect. Failures are retained and stop the participant loop.

The adapter distinguishes `requested_model` from the exact identifier returned
in `response.model`. That identifier is the provider's report, not a claim about
undisclosed serving weights. The declared expected identifier must match every
response. Supplied sampling parameters are checked against reported parameters.
An omitted parameter is recorded as `provider_default`; it is never silently
filled with an invented temperature or reasoning effort. The provider must report
temperature, top-p, reasoning effort, service tier and output limit. These resolved
settings must remain identical to qualification throughout the run.

Input, output and total usage are mandatory nonnegative integer counters; booleans,
inconsistent sums and impossible cache/reasoning subsets fail. Optional cached,
cache-write and reasoning details remain `null` when absent. Output usage includes
reasoning tokens; adding the reasoning subset again would double-count it.
`usage_complete` means the required input/output/total counters are complete.
Monetary cost remains `null`: no pricing schedule or absent cache detail is guessed.

Only the declared `lab` function is supplied to the model. Hosted tools, extra
function names, parallel calls and unsupported output-item types stop the run.
Recorded tool counts cover this declared function surface; they do not claim to
measure invisible provider infrastructure. The controller fixes the run ID and
executes one operation with an argv array and UTF-8 stdin:

```text
perl <packet>/tools/lab.pl RUN OP ARG...
```

There is no shell expansion. The controller invokes `init` after qualification;
the model may invoke `doc`, `write`, `read`, `build`, `public`, `run`, `submit` and
`seal`. A successful, well-formed `seal` ends collection without an extra final
model request. The default outer lab deadline is 180 seconds, allowing the inner
120-second compiler deadline to terminate its child process group and preserve
its result before the outer controller intervenes. Each process has an output cap. Requests, responses,
source-affecting lab calls and returned output are archived separately. This is a
protocol/evidence boundary; deployment still supplies filesystem/network isolation.

## Configure, freeze and qualify

Copy `config.example.json` to the **experiment packet root** as
`EXECUTOR_CONFIG.json`. The shipped null model/freeze fields intentionally block
network activity and lab initialization. Supply the chosen model alias, expected
reported model identifier, provider parameters and resource limits first.

The only supported inference endpoint is the official
`https://api.openai.com/v1/responses`. Authentication uses an already-authorized
`OPENAI_API_KEY` environment variable. The adapter does not read browser/ChatGPT
credentials, move `HOME` or `CODEX_HOME`, change permissions, or add a login flow.
It never records authorization headers and does not pass the API key to the lab,
compiler or submitted program. Do not put a key in configuration or a prompt.

Before creating the final experiment freeze, compute the fixed policy hashes:

```sh
python3 tools/agent_eval/executor.py binding --config "$packet/EXECUTOR_CONFIG.json"
```

The frozen JSON must contain `experiment_id`, `model_settings_sha256`,
`executor_sha256` and `identity_files[ABSOLUTE_PACKET_LAB_PATH]`. The model-policy
hash covers provider parameters, fixed wire settings, function schema and all
lab/provider limits. It excludes `freeze.sha256`, avoiding a hash cycle. The full
configuration file must not be an input to its own freeze. After creating the
freeze, set its exact digest in `EXECUTOR_CONFIG.json`. The packet's own freeze
and lab code must additionally bind compiler, task, documentation and judge inputs.

Run the bounded qualification chain only on the authorized execution host:

```sh
python3 tools/agent_eval/executor.py qualify \
  --config "$packet/EXECUTOR_CONFIG.json" \
  --out "$packet/executor-qualification"
```

This makes two provider requests around exactly one neutral read-only operation,
`probe doc meter-probe 1`. The root-controlled probe run cannot access task or
language documentation or create participant files. The second provider request
must contain the exact function-call result, and its response must say `READY`.
Each response needs positive input/output usage, unique provider request/response
IDs, the expected model and all required reported settings. A zero-tool `READY`
response is insufficient. The output directory must be new. Missing configuration
produces a blocked receipt without a network request; missing API credentials
records zero HTTP attempts. Configured model, reasoning/sampling and output budget
remain the same for qualification and participant requests.

`qualification.schema.json` describes the receipt shape. A consumer must also run
the verifier: a JSON `status` string alone is insufficient.

```sh
python3 tools/agent_eval/executor.py verify \
  --config "$packet/EXECUTOR_CONFIG.json" \
  --qualification "$packet/executor-qualification/qualification.json"
python3 tools/agent_eval/executor.py run \
  --config "$packet/EXECUTOR_CONFIG.json" --run full-a \
  --prompt PROMPT.md \
  --qualification "$packet/executor-qualification/qualification.json" \
  --out "$packet/executor-runs/full-a"
```

Verification rehashes both raw provider captures and the neutral tool operation,
checks that the continuation consumed the exact recorded result, and independently
reproduces all observed qualification fields. Selected participant prompt files
must also be present in the frozen identity map. Before every provider request and lab operation, the driver
rechecks the fixed configuration, freeze, executor policy and lab identity. Before
the next participant operation it validates the returned model, settings, usage
and tool surface. Request and response IDs cannot repeat within a run.

`YAVO_QUALIFICATION_RECEIPT` points the lab at the verified receipt. Run summaries
separate provider attempts, provider responses, controlled participant operations
and controller initialization. Qualification usage is outside participant totals
and remains separately archived. API intervals include transport and serving time;
they are not labeled model reasoning time. HTTP timeout is the transport socket
timeout; the lab has a process deadline and process-group termination on POSIX.

## Offline validation

```sh
python3 -m unittest discover -s tools/agent_eval -p 'test_*.py' -v
```

The tests cover missing and inconsistent usage, requested-vs-reported identity,
unknown sampling defaults, hash tampering, response/tool completeness, duplicate
JSON keys, frozen input changes, launch refusal, credential isolation, process
limits, and stopping before a participant step after an unmetered response.
Synthetic captures are tagged `fixture`; even a consistent one only produces
`fixture_only`, which the verifier refuses for live launch.

## Current environment evidence and interface scope

The current Codex CLI is `0.172.0-alpha.2`. Its documented stdio app-server startup
fails while opening `/home/agent/.codex/state_5.sqlite`. The redacted doctor command
confirmed configured ChatGPT authentication and a successful Responses WebSocket
handshake, but no background server; the control-socket query was denied. Neither
`OPENAI_API_KEY` nor `CODEX_API_KEY` was present. No model request was submitted by
this audit. We did not repeat the prior failed `exec` probes or override credential
homes, permissions or immutable mounts.

The exact installed protocol schema can be exported successfully. It exposes
thread start model/provider/effort and thread cumulative plus last token usage.
Its token update has thread/turn IDs, not a provider response ID. CLI JSON likewise
documents turn-completed aggregate usage. These interfaces do not independently
satisfy this experiment's per-provider-request evidence contract.

Official references, checked 2026-11-22:

- [Responses create reference](https://developers.openai.com/api/reference/resources/responses/methods/create): returned model, response ID, usage and settings.
- [Token counting](https://developers.openai.com/api/docs/guides/token-counting): output counters include generated tokens beyond visible response text.
- [Codex non-interactive mode](https://learn.chatgpt.com/docs/non-interactive-mode): JSON events and turn aggregate usage.
- [Codex app server](https://learn.chatgpt.com/docs/app-server): thread metadata, usage notifications and item lifecycle.

The adapter has not yet made a successful live provider call; an authorized API
environment, final model configuration, freeze and successful smoke receipt remain
the launch prerequisites. Offline test success does not replace them.
