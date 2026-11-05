# 0027 Creator-owned content and standalone delivery

Status: accepted agent-selected implementation direction,2026-11-05, under the user's approved direct-editor/MCP creator goal. Launch/resource foundations are implemented with component checks; runtime-kit packaging and the shared editor/MCP export controller remain in progress. This record does not accept a public alpha or authorize publication.

## Context

The existing application kit, Scene5/Lua consumer and packager work, but exporters select Yard or Motion and fixed sample resources. A creator should export their own saved creation without editing engine code or knowing sample startup flags. The same operation must be available in the editor and MCP. A full project manager, plugin system or publishing service is unnecessary.

## Decision

Use a small `app.json`, format `ruster-application`, version1: creator title, relative Scene5 resource path, a bounded map of mesh IDs to relative OBJ paths, and fixed `tick_millis`. Scene5 remains the complete authored document; this descriptor adds no competing authoring/history model. Validate exact fields/version, duplicate IDs, title/size/count/timestep bounds and relative path confinement. Runtime resource loading remains read-only through ResourceRoot and Session snapshots, with no catalog/cache/lock writes. The editor's existing tick defaults to100ms; export that same step rather than silently changing preview behavior.

Add a creation entry to the existing Motion executable, reusing its GenericMotionLab behavior engine and the shared host. Explicit `--creation app.json` loads it; ordinary launch discovers `app.json` beside the executable, independent of working directory. Only absence permits the old embedded default. Corrupt or nonregular descriptors fail visibly; explicit legacy/generic modes retain their existing behavior. A creation uses its title and quiet runtime presentation with normal Run/Pause/Reset, validates Reset at dt0 and starts running. It does not show Motion comparison metrics or editor instructions. `--creation-info` provides a small versioned compatibility profile for runtime-kit preparation without opening a window.

Capture the current runtime mesh closure through a narrow AssetStore export snapshot. Capture is filesystem-free and owns the accepted catalog baseline/source hashes; a worker reads bounded confined source bytes and checks exact baseline before/after plus each accepted hash. No AssetStore::open, lock/cache writes or silent Refresh during export. Changed sources require explicit Refresh; unrelated catalog assets and historical-only mesh resources are excluded. The saved Scene5 bytes/history are preserved, while the exported artifact's supported purpose is standalone execution, not a relocated editable project.

Prepare a verified runtime kit once, containing the built executable identity, launch-profile version, selected dependency notices/ELF requirements and fixed packaging helper. Reuse existing notice generation, file manifests and atomic no-overwrite publication. Per-creation export must not rebuild or run Cargo metadata. The first slice supports built-in rendering, Lua and accepted triangle OBJ content. Custom editor shaders require explicit unsupported feedback until a matching runtime path exists; never silently change appearance.

One shared editor controller will capture clean saved scene/persistence/catalog identities and selected accepted sources, run bounded preparation off the UI thread, and return an operation ID/status/final archive identity. The editor supplies a chosen fresh destination; MCP requires explicit output-directory authority and inspected identities. The helper/executable and arguments are implementation-owned, never a shell command supplied through MCP. Publish only after revalidating applicable identity/authority; project replacement/cancellation must not publish a stale job. Preserve completed receipts for response-loss inspection. Define these job transitions and their failure tests before calling both routes implemented.

## Alternatives and consequences

A shell launcher or repeated handwritten flags would leave the creator workflow fragmented. A new general runtime or another Lua implementation would duplicate working mechanisms. Preparing notices/build metadata for every creation adds avoidable iteration cost. The descriptor and runtime kit introduce a small compatibility surface, so versions and exact artifacts remain explicit. It does not imply Windows support, a stable ABI or an arbitrary executable/project upload service.

The shared app crate directly uses serde/serde_json already present in its dependency closure; no new external package/version is selected. Exposing the existing scene ID validator avoids constructing placeholder scenes to validate mapping keys. Validation/resource reads happen on load or explicit export, not each paint. Asset source bytes are bounded and hashes retain the accepted-content boundary. Measure representative export cost after the actual controller exists rather than making a speed claim from component tests.

## Acceptance

Both editor and MCP must export one creator-owned named scene with imported OBJ, Lua behavior and an editable property; relocated no-argument/headless execution must retain authored identity/content and expected runtime results. Cover stale source/catalog/save identities, invalid behavior, unsupported shaders, output collisions, cancellation and response-loss receipts proportionately. Read-only runtime/resource and wire tests supplement that journey, not replace it. Current verification and remaining steps belong in STATUS; no product-wide familiarity/performance pass follows from this decision.
