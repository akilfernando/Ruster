# Immutable creator engineering evaluation

Verified2026-11-05. **The approved bounded creator goal reaches its engineering handoff. M9 public-alpha acceptance remains open.** [Requirement audit](CREATOR-readiness.md), [detailed source/native evidence](CREATOR-export.md), [creator workflow](../CREATOR-WORKFLOW.md).

## Review

Local read-only evaluation directory: `/home/akilf/.local/state/ruster-candidates/creator-eval-1`.

```sh
/home/akilf/.local/state/ruster-candidates/creator-eval-1/run-editor
```

The launcher starts a blank scene with the bundled export template preconfigured. Save/edit outside the bundle. Workspace preferences and recovery live under the standard state directory's `ruster-creator-review`; the bundle stays unchanged. `REVIEW.txt` is the short walkthrough; Python3 and `/usr/bin/kill` are required for editor export, while standalone execution needs neither Python nor Rust.

For a writable copy of the actual direct-editor-created project:

```sh
ruster_review_dir=$(mktemp -d /tmp/ruster-review.XXXXXX)
tar -xzf /home/akilf/.local/state/ruster-candidates/creator-eval-1/example-project.tar.gz -C "$ruster_review_dir"
/home/akilf/.local/state/ruster-candidates/creator-eval-1/run-editor --scene "$ruster_review_dir/manual.json"
```

Optional MCP uses the bundled `ruster-mcp`, explicit save/export directory grants and the editor's private socket; follow the creator workflow. No public upload, tag or release was performed.

## Frozen identities

- Exact source `78ff9ef9674bf8094571c7355801a10c792abe7e`,725tracked source files; runtime/editor code unchanged from the verified `c14fd41` repair.
- 750bundle files including manifest: default release editor/adapter, prepared runtime kit, full source/docs, actual direct-editor project/standalone archive, compact evidence and guide.
- Archive `/home/akilf/.local/state/ruster-candidates/creator-eval-1.tar.gz`,58,687,249bytes, SHA256 `b6456ac57a45281cf7ba04022206257ae6bfe2ee2fe9e98f7ab8ce727ffb99be`.
- Manifest SHA256 `13c62b09019768120f1a5215f10e92d8fb9e0c6adff1034b612fd98ba366cf9b`. Adjacent `creator-eval-1-receipt.json` records both hashes; immutable contents are never patched by later documentation updates.
- Editor31,810,968bytes SHA256 `986dbda760b5fe986cdaa583797bd517e32e297a94328a1eda9624f1a58bf400`; adapter864,424bytes SHA256 `02ca597a657be8ac49e97c7059c4644bec3e9e4af5517b84e91876356d37b58c`; runtime-kit identity `325e3a687193297d830db7b71b89c52ce4fde07f699c452464adfc9254aea1fb`.

These are named-host Linux x86_64 builds with recorded ELF/GLIBC requirements, not universal Linux or another-machine certification. Release binaries were checked for home/checkout path leakage within the recorded scan. Registry/source dependencies and provisional scripting/platform decisions retain their existing scope.

## Extracted-bundle verification

A fresh `/tmp/ruster-creator-bundle-376ylxu7` extraction passes safe unique regular-member checks, exact750-file inventory/size/SHA validation and independent byte comparison of all725source files against `git archive` of the frozen commit. Extracted bundle files/directories are made read-only before execution.

The extracted `run-editor` launcher, copied release editor/adapter and copied read-only runtime kit then pass16actual SDK calls:49-tool discovery, owned-project load, Lua Reset, asynchronous export/receipt and extracted10tick application execution. The inspected template path is the relocated bundle's own kit. Normal editor exit0; configuration/workspace/focus preserved; no process remains. All750immutable files and inventory still match afterward; no cache/state/bytecode was written into the bundle. Captured Vulkan log scanner passes with its stated coverage limits.

The bundle's exact direct-editor standalone example also passes10ticks inside unshared bwrap namespaces with read-only app/host libraries, empty home/tmp, source/cache/toolchain/network hidden: title `Manual Beacon Direct`, expected Y rotation0.7499999403953552, every payload hash unchanged. This is the actual manual-created package, not a regenerated stock sample.

Durable private local evidence: `/home/akilf/.local/state/ruster-candidates/creator-eval-1-evidence`, including verification/release logs, native result, exact controllers, selected unaltered framebuffers and `SHA256SUMS.json`. Original temporary fixtures remain preserved. No engine build/tests were repeated during evidence curation. Final post-freeze documentation only updates this handoff and links; the source/binary checks are reused because code is unchanged.

## Explicit follow-ups

Physical keyboard/focus and hidden drag/F2 observations, external experienced-user familiarity/design/accessibility approval, unavailable Unreal perspective, broader hardware/OS, full first-run adoption and exact-candidate hosted CI remain M9 follow-ups. The process-crash journal under/tmp does not establish power-loss durability. Resource observations are bounded workload measurements, not a whole-editor budget or competitive superiority. The [candidate matrix](../ALPHA-CANDIDATE.md) owns acceptance; the engineering goal does not silently redefine it.
