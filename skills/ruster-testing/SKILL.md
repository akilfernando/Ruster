---
name: ruster-testing
description: Select and run proportionate verification for Ruster changes, prevent redundant test growth, and record evidence before handoff or PR. Applies to Ruster code, tooling, and documentation changes.
---
# Ruster testing

Read `docs/TESTING.md` for the single testing policy and runnable commands. Use this skill together with ruster-development; do not rediscover models or spawn agents merely to run checks.

## Before editing

Identify the behavior at risk and the cheapest test that would catch a meaningful failure. Inspect existing tests before adding one. Extend a relevant case or table instead of duplicating the same contract at multiple layers. Pure documentation, formatting, mechanical or reversible low-impact changes normally need checks, not new regression tests.

## During work

Use a targeted test while implementing. A bug fix should demonstrate failure before the fix when practical; record when this cannot be done. Choose externally observable contracts and invariants over implementation mirrors, source-text assertions, incidental snapshots or mock-only success. Put logic tests at the lowest useful layer and reserve a few integration tests for actual boundary behavior.

For unsafe/resource lifetime, concurrency, persistence/migrations, protocol/permission or rendering changes, select the additional evidence from the policy; a green default runner is insufficient. Do not add heavyweight suites to every PR because one subsystem needs them.

## Before handoff

Run `python3 scripts/check.py --base <target-ref>` with the actual PR target when available; use `--dry-run` to inspect the plan. Default HEAD covers local changes, not a whole multi-commit PR. With no initial commit the runner conservatively uses full scope. Invalid explicit bases are errors. Review the selected scope for semantic dependencies that metadata cannot express and use `--full` or supplemental commands where justified.

Reuse verification already run on the final relevant content, dependencies, configuration and environment by a trusted worker/CI. Inspect its evidence and diff; do not rerun just to have the root execute it. If integration changes those inputs, rerun affected checks. A previous result is not evidence for a changed merge result. A model handoff or specialist review alone does not invalidate existing verification. When a repair changes inputs, identify the affected contracts and rerun their checks under docs/TESTING.md. Consolidate review around the relevant evidence package and link raw logs instead of repeatedly copying them into agent context. Use Cargo's build cache; do not create a separate stale-pass cache.

Record scope/base, commands, result, approximate wall time, material exclusions and reasons in the task handoff/status. Never report dry-run as verification. Missing tool/hardware, skipped checks, failures and flaky outcomes remain explicit; do not retry until green, silently drop tests, or weaken assertions to meet a timing target.

## Maintain suite health

For each new test, be able to name its unique risk, correct layer, and why existing coverage is insufficient. Use representative boundary cases instead of every input combination. Remove/merge obsolete or duplicate tests only with evidence that the protected contract is preserved or intentionally retired. Review slow tests when they affect the feedback budget; improve fixture setup or move them to the right lane, without hiding regressions.

Quarantine requires an issue/record with owner, reproducible evidence, expiry or review date, and explicit visibility. Critical coverage needs a reliable replacement before its gate is removed. Do not adopt test-count or coverage-percentage quotas. Performance comparisons require representative workloads and controlled environments, not noisy hard gates on shared runners.
