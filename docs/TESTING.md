# Testing policy

Adopted 2026-11-04 under the user's request for diligent verification with minimal overhead. This is the single policy; [the testing skill](../skills/ruster-testing/SKILL.md) applies it during development.

## Principles

Protect behavior, not test counts. Select checks by changed behavior and dependency impact. Keep frequent feedback fast; isolate expensive evidence without making it optional when relevant. A failed or missing required check is never a pass. Avoid duplicate verification of unchanged final inputs.

## Verification lanes

| Lane | Trigger | Required evidence |
| --- | --- | --- |
| Documentation | Only recognized docs/skills changes | Local links/whitespace, human review of accuracy; skill validator when skill metadata changes. No Rust build. |
| Focused development | Iterating on a known behavior | The relevant existing or newly justified regression test; expand only when results or changes warrant it. |
| Change gate | Ready for handoff/PR | Formatting, strict Clippy, tests for affected workspace packages plus reverse dependents; docs checks. Shared build/configuration, unknown paths or uncertain scope select full workspace. |
| Risk-specific | Change touches a sensitive boundary | Additional checks from the matrix below, required for that change before acceptance. |
| Broad validation | Release candidate, toolchain/dependency changes, structural changes; periodic CI once introduced | Full workspace plus supported platform/feature/hardware coverage relevant to release. Avoid exhaustive feature cross-products. |

There is currently no hosted CI, PR gate, merge queue, GPU suite, scheduled job, or release matrix configured. The local runner implements documentation/change/full-workspace checks only. Future CI should call the same runner against the actual target/merge result; it must not rerun overlapping jobs by default. Full validation is not automatically a GPU/performance/platform audit.

## Commands

```sh
# Local edits, including staged/unstaged/untracked files
python3 scripts/check.py --dry-run
python3 scripts/check.py

# Whole PR: use the fetched, actual target reference (example only)
python3 scripts/check.py --base origin/main --dry-run
python3 scripts/check.py --base origin/main

# Conservative full workspace verification
python3 scripts/check.py --full

# Runner regression checks when selection/execution tooling changes
python3 -m unittest discover -s scripts/tests -v
```

Default base is HEAD. The runner includes changes since the merge base plus working-tree/index/untracked changes, including deleted paths. Before the first commit it selects full scope. An explicitly supplied invalid base fails. It computes Rust reverse dependencies from Cargo metadata; it cannot infer semantic dependencies, external consumers, feature-gated integration or GPU behavior. Review the plan and broaden manually when these matter. A printed dry plan does not execute checks.

The runner uses Cargo's normal incremental cache and `--locked` for builds/tests. Missing Cargo/toolchain, stale lockfiles and nonzero child exits are visible failures. Bootstrap commands for a deliberately changed dependency/lockfile must be explicit in the task evidence. No pass-result cache or silent retry loop exists.

## Risk-specific evidence

| Change | Additional evidence, when applicable |
| --- | --- |
| Arithmetic or simulation ordering | Invariants, boundary/overflow cases, fixed input replay; release-profile test when optimization/overflow semantics are relevant |
| Serialization, save formats, migrations | Roundtrip plus old-version fixtures and failure atomicity; corrupt/truncated inputs; keep only supported-version fixtures |
| Commands, undo, MCP and permissions | Valid/invalid transactions, stale revisions, authorization boundaries, bounded output; one small transport smoke for changed wire behavior |
| Unsafe code, FFI, resource ownership | Targeted safety review and relevant sanitizer/Miri checks where supported; Miri does not validate Vulkan driver behavior |
| Concurrency | Adversarial ordering or model checking where feasible, bounded stress; repeated success alone does not prove race freedom |
| Vulkan/rendering | Validation layers and synchronization diagnostics, representative device smoke, resize/shutdown; image comparisons only for meaningful visual contracts with documented tolerance |
| Performance claims | Release build; fixed workload, hardware/configuration, warmup/repeats and noise reported; compare before/after and verify correctness |
| UI/UX | Exercise the changed user journey, error/recovery and keyboard interaction; use targeted visual inspection, not screenshots of every widget |
| Tooling/test selection | Temporary-repository tests for classification, dependency closure, base resolution and failure propagation; runner changes conservatively trigger full scope |

Several later subsystems do not exist yet. Add executable checks alongside the feature they protect; do not prebuild speculative infrastructure. Unavailable required hardware/tools must be recorded as an unresolved validation gap and resolved before claiming that support/release criterion.

## Feedback budgets and suite growth

Initial engineering targets, not measured service guarantees: warm local change gate under 30 seconds, typical hosted PR gate under 2 minutes when CI exists. Record cold builds separately. Exceeding a budget triggers investigation, not automatic failure, test deletion or skipped checks. Prefer fixture reuse, less process startup, narrower valid dependency scope and elimination of duplication before adding parallel jobs.

Each added test must protect a named failure mode. Prefer one table-driven invariant test over many nearly identical examples. End-to-end tests should protect wiring, exit behavior or workflow contracts that lower layers cannot. Do not duplicate a complete input matrix at every layer. No universal coverage threshold, compulsory test for every edit, or mandatory benchmark per PR.

Use focused tests during iteration, then the selected final gate once. Root and worker must not both run the same final suite unless integration changed its inputs or evidence is inadequate. CI may independently validate remote changes; its authoritative result can satisfy review without another local replay on identical inputs.

## Failures and maintenance

Fix relevant failures. Classify environmental failures separately and report them. Flaky tests are defects; do not hide them with retries. A quarantined check needs a tracked record containing owner, failure evidence, review date and affected protection. Critical coverage needs a reliable replacement before removing its blocking role.

Review slow or redundant cases when they impair feedback, when replacing an implementation, and before release. Preserve regression value when consolidating. Keep timing evidence in handoffs; avoid a permanent append-only log of every green command.

## Evidence template

Scope/base and why; commands; pass/fail/counts; wall time and cold/warm context where measured; supplemental checks; exclusions/limitations. A short task or PR paragraph is sufficient. Never copy secrets or enormous test logs into documentation.

## Native editor checks

The editor quickstart in `crates/ruster-editor/README.md` is the manual journey. Its optional `inspection` feature supports app-scoped widget/input/framebuffer checks without global desktop input. Keep it default-off and loopback-only. Test on disposable scenario copies. Record platform/window context and distinguish model tests, native interaction, framebuffer inspection and external human usability studies.

On the current workstation, follow AGENTS.md’s workspace-2 isolation rule. Hidden Wayland windows may stop painting, so an inspection timeout is an environment observation rather than evidence that a UI action succeeded. A temporary, editor-scoped Hyprland `render_unfocused=true` rule can permit background verification; remove it after testing and record the altered rendering condition. Do not compare those measurements with normal occluded idle behavior or switch the user’s workspace for convenience.
