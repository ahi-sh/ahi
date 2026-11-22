---
name: compare-lab-harness
description: Compare headless Pi and ahi on the same synthetic read, coding or firmware task. Use scripted loopback to verify harness wiring first; one live turn requires a named model, authorized endpoint, budget and disposable host.
---

# Compare harnesses on work you can check

Use `probe.py` from this skill directory. It runs the **actual client** selected by `--binary`, with fresh state, in a temporary workspace. It does not install or update either client, connect to ahi serve, use operator sessions, or load your normal extensions or skills. This is a narrow tool-use profile, **not** the stock TUI, a skill-catalog stress test, or a security sandbox.

## Prove the wiring without inference

Requires POSIX, Python 3.9+, installed ahi and/or Pi with the flags in `probe.py`, Node for Pi, `cc` for the C check. Use the real ahi binary (for example `cargo build --locked --bin ahi` followed by the absolute `target/debug/ahi`), not a shim that needs the operator's home. From this directory:

```sh
python3 probe.py --client ahi --binary "$AHI_BIN" --case cases/base.json
python3 probe.py --client pi --binary "$PI_BIN" --case cases/base.json
python3 probe.py --client ahi --binary "$AHI_BIN" --case cases/coding.json --checker checks/python-bounds.sh
python3 probe.py --client pi --binary "$PI_BIN" --case cases/firmware.json --checker checks/c-mask.sh
```

For the six client/workflow combinations and negative controls, run `python3 smoke.py --ahi-bin "$AHI_BIN" --pi-bin "$PI_BIN"`. It checks correct prose without a tool call, a checker that actually exits 7, and round exhaustion. This command never uses a live endpoint.

Swap clients without changing the case or checker. The scripted endpoint requests real file tools and, for a checker, an actual process. The base case needs a correlated read result **and** the requested identifier. Coding checks the byte-level edit and three Python boundary cases. Firmware checks the header edit and compiles C11 static assertions with the **host** compiler; it never flashes, emulates, or claims target-board behavior. No checker means no command tool. A case passes only if the client finishes, the answer matches, the file and untouched neighbor match, the read result reached the provider, and any requested checker both exited zero and returned its fresh marker. Prose saying “tests passed” cannot satisfy that last gate.

## Longer workloads for slow serving

Generate a fixed pair of cases into a **new** directory; run the same bytes on both clients. These commands are local scripted rehearsals:

```sh
RUN=$(mktemp -d)
python3 make_long_cases.py "$RUN/cases"
python3 probe.py --client ahi --binary "$AHI_BIN" --case "$RUN/cases/catalog-lookup.json" --timeout 900
python3 probe.py --client pi --binary "$PI_BIN" --case "$RUN/cases/catalog-manifest.json" --timeout 900 --max-tokens 4096
```

Both tasks put a 384-entry sensor catalog in the initial prompt. Lookup requires reading the requested sensor from disk and returning its code: longer input, short answer. Manifest requires reading an ordered 112-sensor assembly list and writing all 112 matched codes: long input **and** a substantial generated tool argument, graded byte-for-byte. They exercise different work; never pool their times. The receipt's `prompt_bytes` counts the user prompt, not the full wire request; `expected_write_bytes` counts required file bytes, not decode tokens. Per-response `request_usage` captures token and cached-prefix counts only if returned. Null is not zero. A cached prefix is not proof of model residency or that a whole multi-request turn was warm.

The long examples select a **15-minute whole-turn budget**, not a prediction of DGX performance or a new live default. No silent 25-second proxy cutoff: every round shares the time remaining. Use the candidate-built ahi binary: the released 0.27.1 gateway client still has a 120-second per-request ceiling even with a longer `exec --timeout`. Other gateways/clients may impose their own limits. To check the actual path, `python3 -B slow_loopback.py --client ahi --binary "$AHI_BIN"` deliberately spends 126 seconds in the first local request before the tool round completes; repeat with Pi. `--delay 10 --timeout 3` checks that the deadline still stops the turn. Neither uses a model. A timeout records an incomplete trial; it cannot diagnose loading, prefill or decode by itself.

## One authorized live trial

First obtain authorization for the exact existing route, model, workflow, maximum trials, requests, tokens, timeout, and credential source. **Run inside a disposable VM/container** containing only synthetic inputs, the clients and toolchain. No host mounts, SSH agent, device access or admin credentials. Restrict egress externally to the approved endpoint. `--disposable-host` acknowledges this setup; it does not create it. Pi's `bash` is not an argv allowlist. The ahi policy and temporary HOME are not host isolation either. Prefer an external scoped authenticating proxy. If the route needs a bearer token, use `HILLCLIMB_API_KEY` over HTTPS with a short-lived benchmark-only token; never put durable credentials in a guest running model-generated commands.

```sh
# Variables supplied by the operator. Append ONE receipt; do not retry on failure.
python3 probe.py --client pi --binary "$PI_BIN" --case cases/firmware.json \
  --checker checks/c-mask.sh --disposable-host --live-url "$CHAT_BASE_URL" \
  --model "$SERVED_ID" --route "$MANIFEST_ID" --thinking high \
  --rounds 5 --max-tokens 768 >> "$LEDGER"
```

Use the **Chat Completions base URL including its prefix**, e.g. `https://example.invalid/v1`, not the discovery URL or the full `/chat/completions` path. The temporary provider forwards only to this route with no redirect. It checks the client actually sent the requested model, tools, effort and output cap. `off` requires *absence* of wire effort, not proof of the server's internal behavior. Five requests, 768 output tokens/request and 90 seconds per turn by default; use an explicitly approved longer `--timeout` (up to 3600 seconds). A provider request can continue server-side after disconnect; do not compensate with an automatic replay. A gateway error or timeout is evidence too. Exit 0 = task passed; exit 1 with JSON = observed failure. A setup error without a receipt is **not a completed trial**; record it separately. Use `--help` for limits.

## Bring a workflow; keep the oracle outside the workspace

Copy `cases/coding.json` and a checker. `input` is one plain filename; `before` seeds its bytes; `marker` is evidence expected in the read result; `answer` is the exact final identifier; `after` requests a write and specifies its exact UTF-8 bytes. The runner appends an identifier-only response instruction. Use the narrow byte oracle for surgical changes; don't use it to grade open-ended designs. `--checker /absolute/reviewed-test.sh` runs a **trusted POSIX sh script** in the fixture's cwd as `check-fixture`. Pin its tools in the guest image. Start with a checker that fails on the original fixture and passes on the correction; also try a deliberately wrong change. Do not source real repo setup, upload proprietary firmware, expose a debugger, or let the model author the grader. For many-file design work, add a new case type and invariants in an independent candidate rather than claiming this oracle covers it.

Each invocation writes one `harness-trial/1` JSON receipt to stdout, no raw messages or bearer values. `elapsed_s` measures the **whole client turn**, including startup, tools and a bounded, buffering proxy. It is not server TTFT or tok/s. Keep the ledger private. Compare with `hillclimb-serving/compare.py` in inference-lab. For TUI evidence use ahi's `verify-live-harness` instead; for ahi-specific Git history or skill preloads use `scripts/eval-core-prompt.py`. Neither is a substitute for a Pi run.
