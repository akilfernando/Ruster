# Asset-admission experiment evidence

Started2026-11-08 under the user's “Proceed” authorization after the engineering first-phase handoff. The [experiment contract](../plans/ASSET-ADMISSION-EXPERIMENT.md) defines creator outcome, fixtures and stopping gates. Baseline product checkout is997f5a0332436b34699f4f411add91092cafcd0a. Workspace `/home/akilf/Projects/Ruster-Audits/stage2-admission-20261108` records owning source hashes in `baseline-source.json`.

## Preparation and accepted scope

Actual CreationManifest loaders validate metadata and collect complete decoded maps. Final Content then validates payloads and aggregate limits. Astra/root select checked cumulative admission before each insertion, shared locally with Content. ResourceRoot's decode validation avoids another full payload scan in the loader. Replacement-aware AssetStore accounting stays under its existing owner. Cross-kind ID collision refusal currently belongs to final Content, correcting an initial reviewer assumption about the manifest. Later resource I/O/decode errors can be superseded by an earlier aggregate crossing, while whole-manifest metadata validation remains first.

Root extracts strace7.3from the live repository package metadata into this experiment's private tool directory. Package SHA256matches `b3c67815289238f99ab6422d0cd562dbbb5becca8b49179fffad72be9f23e1a0`. Binary SHA is `af9b42502ebd9fb4ed27a702fa44ed1e369b3e4a5addae7a9799a698fda591b1`. Its version check and exact-owned `/usr/bin/true` child tracing return0. Selection, preparation and trace receipts are in `verification/strace-*.json`. This changes no global installation, security or power setting. Loader tracing remains pending and will be excluded from performance timing.

The ordinary public loading harness and deterministic PNG/scene fixture generator are in source preparation. Root starts the unmodified remapped release ruster-app library build with four jobs, locked/offline, retaining Cargo JSON/stderr. No fixture outcome, loading cost, process peak, logical overlap or candidate improvement is established at this checkpoint.

## Research checked2026-11-08

Godot documents that retrieving unfinished background work can still block. Unreal distinguishes asset references and retained ownership. Bevy's per-frame upload resource is a soft whole-asset scheduling policy. These inform keeping this correction local to cumulative admission. A streaming scheduler would require separate workflow and ownership decisions. No Ruster performance result follows from those sources. [Godot loading](https://docs.godotengine.org/en/stable/tutorials/io/background_loading.html), [Unreal loading](https://dev.epicgames.com/documentation/en-us/unreal-engine/asynchronous-asset-loading-in-unreal-engine), [Bevy upload budget](https://docs.rs/bevy/0.19.1/bevy/render/render_asset/struct.RenderAssetBytesPerFrame.html).
