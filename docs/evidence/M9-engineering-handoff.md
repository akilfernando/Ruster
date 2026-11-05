# Local M9 engineering handoff — 2026-11-05

This document owns disposable artifact/desktop continuation details. [STATUS](../STATUS.md) owns current priorities; the [candidate matrix](../ALPHA-CANDIDATE.md) owns acceptance. No M9 completion or publication is implied.

## Artifacts and unchanged verification

Corrected source snapshot `m9-eval-2` is frozen at `c0eb45130d27f63da17e8128da11f4bb53f474f8`, with source manifest/inventory/checksums and separately assembled stock example packages beneath `/home/akilf/.local/state/ruster-candidates/m9-eval-2`. Preserve immutable eval1 and eval2. [Archive adoption](M9-source-archive-adoption.md), [Yard](M9-adopter-yard.md), [Motion](M9-adopter-motion.md) and [stock packages](M9-eval2-stock-packages.md) own exact provenance and boundaries. Subsequent work through `be5a482` changed documentation/evidence only.

The host code gate passed176Rust/22Python tests, strict lint/format/docs54.55s. The actual read-only Gitless eval2 Ubuntu extraction passed the same checks62.09s. Each ignored1existing GPU test. Reuse those results for unchanged code; documentation/evidence integration passed complete Markdown links/whitespace, diff whitespace and five safe-member/checksum/privacy archive checks. The runner conservatively selected full scope for unrecognized evidence tarballs; no redundant Rust gate was run for those documentation payloads.

`target/release/ruster-editor` is the default release,31,540,048bytes/SHA256 `872694182582b80c2af42048ea42298d5e8a31113ee78e270f0db4b06f3e6d6f`. Frozen inspection release `/tmp/ruster-m9-inspection-release-a083654` is32,816,904bytes/SHA256 `6d40f91897522f1d0a7ccb040d6cd80ad39f3180a95e7270fa390792ba2e16d2`; GUI/runtime source is `a083654`, preceding a help-copy-only change. MCP adapter `target/debug/ruster-mcp` SHA256 `44182096cf08f38d4f28b471d9e3ff5e75ccfd87b72d1fe6056fbe07ed1391a9`. Revalidate live files before launching. Pinned Cargo bin directory is `/home/akilf/.rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin`; bare Cargo was absent from the tool PATH. No global profile modification is needed.

## Preserved user work and desktop state

The user closed both human review windows; PIDs717774/771692 were absent, with Save/Discard and exit codes unobserved. Preserve `/tmp/ruster-m9-human-20261105` and `/tmp/ruster-m9-review-latest-yckrhqj5`, including state/recovery. Never infer saved work or target reused PIDs. Follow AGENTS.md: silent workspace2 during active desktop use, required focused tests permitted while away, original workspace/window restored, clipboard protected. Session lock must remain intact until normal authenticated unlock.

[Focused-test blocker](M9-focused-test-blocker.md) records two probes that stopped before input/sampling. Owned editors are absent; workspace1/original Zen were restored. Last normal close was requested but its exit code was unobserved. The prepared-only `/tmp/ruster-m9-focused-controller-unlocked.py`, SHA256 `3df2438701f1761075d642e8c161e530e405aa2308e96b7e62f6879bd67d45fa`, adds logind/Noctalia lock preflight through the real compositor session and accurate cursor observations. Syntax compilation passed; it has not executed or produced focused results. User has been asked to report normal unlock. A new root must review/revalidate this temporary controller before execution; user away-test authorization persists.

## Partial recovery continuation

[Partial recovery evidence](M9-recovery-partial.md) owns exact outcomes and failed controller steps. Private real fixture is `/home/akilf/.local/state/ruster-m9-recovery-adopter`. Original scene SHA256 remains `7c07a328725c6b45b327cf718054c883e3f46c7566458f8735984133faf90c5c`. Preserve the original `checkpoint.json` raw copy, initial `result.json`, `continuation/` and actual journal beneath `fixture/recovery/`.

Sequence1/revision3 contains invalid authored source plus a distinct unapplied draft; intentional owned SIGKILL exit-9 was observed after checkpoint verification. A fixed-port TIME_WAIT preflight interrupted restart before app execution. Reviewed fresh-port continuation restored the draft as an unsaved copy, applied repaired source through GUI and completed guarded Reset with diagnostic cleared. Sequence2/revision4 contains repaired source and no draft. Root independently verified both raw document checksums and viewed3frames. The original file stayed unchanged.

SaveAs selector had no match after File click; no post-click tree exists, so visibility/cause is unresolved. Continuation process/window are absent with exit code unobserved; config equal/workspace1. SaveAs/normal-close/reopen remain unverified for this integrated journey. No further background replay: next plan must observe menu state and durable process lifetime, revalidate the current journal, and run after normal unlock. Do not recreate checkpoints to claim successful recovery. No native/Cargo/performance job remained running at handoff.
