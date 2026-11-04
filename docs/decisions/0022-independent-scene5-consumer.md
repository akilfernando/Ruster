# 0022 — Independent Scene5 consumer and selected application exports

Date:2026-11-04. Status: accepted implementation and recorded local reuse/native evidence. Does not complete the remaining M8 tooling/debugger gates; M9 is deferred by the user.

## Context

Yard demonstrates application-owned logistics using the shared kit. A second logistics presentation would not establish broader reuse. Motion Lab's initial Rhai experiment supplied non-game motion authoring, but expanding its separate editor would duplicate the generic editor's source, properties, history and recovery. Existing Rhai project1 compatibility must remain explicit.

## Decision

Add `--scene PATH` and an embedded `--generic-demo` to Motion Lab. The new comparison application reads the complete bounded Scene5 Session, including authoring/history, through regular-file input and detached snapshot decoding. It never gains save authority, writes a lock/cache or strips executable authoring to geometry. Legacy defaults/`--script`/`--project` remain distinct; no silent Rhai-to-Lua translation occurs.

The application opts into ruster-behavior; ruster-app/render/scene gain no mandatory interpreter. Create Engine lazily on the application's owning thread at explicit Reset, with its fixed dt matching the kit's Step. Snapshot runtime-only speed overrides without changing authored properties/history. Engine provides atomic Reset/Step and contextual errors; the consumer rejects host clock mismatch before mutation. Failed operations retain the last committed runtime. Mesh inputs use explicit bounded read-only resource mappings; missing references fail rather than becoming silent placeholders.

The app owns comparison presentation and selection/displacement metrics, without player, score or win-state concepts. Source/property/history editing belongs to the generic editor. No new source editor, recovery store or engine internals are copied into the example.

Extend the existing source exporter and binary packager with known `yard`/`motion-lab` selectors, selected crate/resource allowlists, required Lua template/vendor source, relative path dependencies and application-specific readmes/archive roots. Preserve no-overwrite staging, path/symlink guards and active normal/build dependency notices. Motion resources are exactly `generic.scene.json` and `motion.rhai`. Bundled Lua needs a C compiler/linker at build time; no system Lua is required to run the package.

## Alternatives and consequences

Keeping only the legacy prototype would leave the new generic behavior/editor mechanisms without a second consumer. Replacing its old format would break saved work. A general project exporter/plugin framework is unnecessary for two owned examples. The selected exporter retains source-based independence rather than promising universal packaging.

The default Motion binary carries both provisional interpreters for compatibility. Recorded binaries are about26.2MB(Yard) and33.1MB(Motion); these are different applications/dependency closures, not a same-workload efficiency comparison. Native graphics/system libraries remain dynamically required. This remains local Linux x86_64 development distribution, with no ABI/platform support promise or public release.

## Verification

Three focused consumer cases and strict all-target lint passed; the combined Stage2 gate passed31.57s including legacy/runtime consumers and21tooling tests. Six packaging tests include selected optional backend/vendor/template/resource boundaries. An independent developer agent built both exported workspaces, changed only app-owned behavior data/rules, rebuilt, packaged and ran relocated results from an unrelated cwd without engine edits. [Independent proof](../evidence/M8-independent-reuse.md) records commands, source exceptions, warm/cold conditions, timings/hashes and the real missing codegen-notice failure/repair. First release builds took198.563s/135.103s with4jobs and shared registry/target caches; changed-app rebuilds27.262s/29.265s, one sample each. No steady-state GPU or cross-engine claim follows.

[Native Motion comparison](../evidence/M8-motion-native.md) verified Reset/Step/Run/Pause, differing sample displacement, preserved authored positions and Reset return, normal exit0 and explicit-layer Vulkan capture. Input was app-scoped on hidden workspace2. Root inspected frames and restored compositor bytes; pilot setup/dispatch failures are preserved separately. These agent/tool checks are not human familiarity or fresh-machine adoption evidence.
