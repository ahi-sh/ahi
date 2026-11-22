# Pi / ahi lab benchmark: independent design review

Reviewed 2026-11-22 UTC. These are candidate snapshots, not published releases or model-performance results.

| Design | ahi | inference-lab |
|---|---|---|
| A: use the exported gateway catalog and custom case directories | `0e988a0` (`experiment/benchmark-lab-export`) | `04f7e69` (`hill-climb-inference` on main) |
| B: enforce per-turn bounds and score effects from receipts | `befe19a` (`experiment/harness-hillclimb`) | `bdbc319` (`experiment/harness-hillclimb`) |

Claude Code reported **`claude-fable-5-1`** for a fresh no-tools review of pinned source and local observations. Verdict: **proceed with changes** on consolidation; **hold live comparisons** until offline acceptance. The authors' implementations remain separate. No server or gateway changes are needed to reproduce these observations.

## Reproduced

- A's committed smoke with Pi 1.5.2 and ahi 0.27.1: neither preflight sent a completion; both clients returned the exact scripted no-tools answer. A includes the timer-rollover grader. Its author reported separate successful rollover, custom Pi tool and explicit ahi skill trials; A's smoke does not yet replay those three claims.
- B's committed smoke with the same binaries: both clients passed base read, Python edit/check and host C11 mask edit/compile. Both rejected no-tool prose, checker exit 7 and exhausted request budgets. B's three reducer tests passed. None of these timings benchmark inference.
- **Control drift:** using `test_compare.ledger()` with nonoverlapping starts, set the nine elapsed times to `[10, 9, 20] * 3` seconds. With `--axis stack` and a 5% margin the current reducer returns `confirm-speed-gain`. The control changed by ten seconds while the candidate beat the faster control by just one. Record this as inconclusive under the lab's drift rule, not evidence for a speedup.
- **Preflight scope:** on A's `assets/ticks` with a loopback catalog, preflight exited zero with a syntactically invalid `--pi-extension` file and with an absent `--ahi-skill repo/nonexistent`. This is correct for a configuration check; it is not workflow readiness. Keep that preflight free of extension execution. Test actual invocation separately on scripted loopback and assert the correlated result.

## Consolidation recommendation

| Keep | Edit or retire |
|---|---|
| A's validated export, no-completion preflight, no-tools baseline, multi-file case layout, extension/skill adapters, independent rollover grader, server-vs-harness distinction. | Add a hash of fixture, policy, grader, extension and client build to each comparable receipt. Stage/grade outside the model workspace. A's `trial.json` is process metadata, not a passing grade. |
| B's round/field checks, effect receipts, negative controls, checker exit and neighboring-control reducer. | Express evidence gates per case: a no-tools baseline must *not* require B's `read_result`; a custom tool needs its own call/result oracle. Preserve export compatibility and capability metadata when applying the requested budgets. Don't replace the production profile with B's synthetic one and call it identical. |
| Separate upstream server telemetry and end-to-end client timings. | Record usage if reported, unknown otherwise. B buffers responses, so do not infer TTFT from it. Neither a turn timeout nor client termination proves backend cancellation. |

Before a scored live comparison, use one approved budget/deadline throughout. B's proxy cuts each upstream call at **25 s** even though its turn permits **90 s**; the reducer classifies proxy errors as `investigate-trial-errors`, **not** a model quality rejection. A limits client wall time, not request count, and its outer timeout targets the direct child only. Prove timeout handling with a synthetic slow stream and a harmless tool subprocess in disposable isolation; do not claim containment or server cancellation. Add the control-drift counterexample and the positive/negative extension/skill/rollover cases to an explicit loopback smoke. Once one runner and one receipt consumer pass these gates, retire duplicate public entry points, not their Git history. Do not deploy or run a live A/B/A from this review.
