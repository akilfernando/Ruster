# Tideglass — MCP creator exercise

2026-11-05. The user asked the agent to brainstorm its own concrete project and realize it through the editor's native MCP. This is a creative project using existing capabilities, not a new engine feature or M9 acceptance gate.

## Creative brief and result

Considered a clockwork city, a rectangular wave field and a kinetic flower. Selected **Tideglass**, a monochrome kinetic observatory: three rings of faceted petals around a rotating crystal, with traveling waves and a counter-running middle ring. The intended repeating cycle is20seconds at3motor revolutions/minute. The choices are agent-selected for this exercise, not user product requirements.

The first live MCP-built version had43objects on a solid stepped platform. Root reviewed actual rest/wave screenshots; the result read as a low crown. Terra's bounded read-only critique recommended stronger central emphasis, differentiated ring heights and closer framing. Root then changed the project through MCP: hollow perimeter rings, six support spokes, a taller/wider central crystal, broader petals and refined height/depth/sway. Final49objects,37animated bindings, two shared Lua sources and four original imported OBJ meshes. The ring-and-spoke silhouette makes the mechanism legible against the background.

## Saved work

Persistent project: `/home/akilf/Projects/Ruster-Creations/tideglass` (separate from the engine repository).

- `README.md`: creative brief, controls, creation method, limits and instructions.
- `Tideglass.scene.json`: complete MCP-authored Scene5 document; SHA256 `b90da8f255b07c1822f289c7ef183acb63f1a4728fe67e8acbc4ef8c00b92d12`.
- `Meshes/`, `Scripts/`: original source assets and readable mirrors of embedded scripts. Embedded scene source remains authoritative; mirrors are not automatically live-linked.
- `open-editor.sh`: opens the editable project with creator-eval-1 and restricted project/export MCP grants.
- `run.sh`, `Build/application/`: actual independently runnable creation.
- `Exports/Tideglass-linux-x86_64.tar.gz`:10,462,806bytes, SHA256 `2ab7fff6c1fb70cd30a6f921cf0af6ff69c642208509b553cce0f2f273c09c5a`;14files. Exact receipt and runtime/template/manifest identities in `Exports/receipt.json`.
- `Gallery/Tideglass.png`, `Gallery/Rest-pose.png`: unaltered native editor captures.
- `Tools/studio.py`, `.studio/`: local MCP session script and private exact call traces/reports/captures. `Tools/make_meshes.py` records model construction; `Tools/verify.py` verifies the exported application's intended motion.

No existing human project, immutable evaluation bundle or engine source was edited. No public publishing occurred.

## Actual authoring path

Used the official Python MCP SDK2.3.0 against the real local49-tool adapter/editor, not a mock. Started at the editor's empty welcome scene; its first file was created by granted `scene_save`. All object construction/duplication/naming, mesh assignment, placement, source/binding/property edits, preview, saves and export used native tools with inspected session/revision/catalog/save/configuration identities. Scene JSON was never generated or patched directly.

Generated only the original OBJ source models outside the editor, then imported them via `asset_import`. Duplicated configured petal prototypes, retaining shared source references and independent typed properties; used `scene_transform_many` for complete layout changes. The visible spindle is an object-reference clock read from the common tick-start snapshot, so every petal sees a coherent phase. No hidden global time/state or application-specific engine code was added.

Successful initial construction/preview, creative refinement and export sessions used175/155/31calls respectively and closed normally with status0. UI inspection was used only for camera angle/zoom, selecting the motor for presentation and screenshots; it did not author project content. All sessions preserved workspace/focus and restored temporary exact-process rendering rules byte-for-byte. Editor source matches the previously verified c14fd41 code with optional inspection; export uses the immutable prepared release runtime. No Cargo/build or new Rust regression suite was needed for content creation.

## Proportionate verification

Actual extracted application ran from an unrelated working directory at1,51and201fixed100ms ticks. After200ticks/20seconds all49object transforms match their earlier pose within0.0000021. Quarter-cycle Y scales match the intended opposing waves: outer1.55, middle1.15, inner0.66. All transforms are finite, all scales positive, authored scene identical across runs, and saved/runtime payload hashes unchanged. `verification.json` contains exact results and inventory.

Native standalone playback with an exact-process background-render rule reaches32ticks/34rendered frames and closes normally,49objects, five mesh uploads/61,200mesh bytes and five reported scene draw calls, no unresolved error. These renderer counters are workload observations, not total application memory or comparative performance. Captured logs pass the Vulkan scanner; validation layers were not explicitly enabled for this creative journey.

Final default-release editor was reopened from the saved project through its launcher, reset/played/paused through MCP, and left on workspace2 for review at tick154. Source file unchanged; temporary rule removed, config/workspace/focus preserved. Its intentionally live review process details are in `.studio/review.json`; revalidate PID/start ticks/executable before further control. Press Play to resume. This is app-scoped/injected observation, not physical-user validation.

Two harness issues are retained: an initial Python helper argument-name collision stopped after importing assets, before any object creation; the exact empty window was closed with disappearance observed, exit code unavailable after its parent exited. One correction resumed from the real empty saved project and accepted catalog. A native playback probe's escaped hyphen in its temporary class matcher prevented background painting (two frames/tick0); correcting that matcher produced the native32tick run. Neither was an engine/content defect. Initial model winding was corrected in the original asset generator before import.

## Findings for later product work

MCP can author this whole creation and deliver it without engine changes. It does not currently expose camera/selection/color edits. Camera presentation needed observation-only UI controls, and the chosen editor framing is not included in the standalone descriptor, so runtime starts with conservative default framing. A new scene's heading remains `untitled` after saving to a named file. These are concrete creator-exercise observations for later prioritization, not silently implemented features or new accepted architecture.

The32-entry Undo history retains recent edits; it cannot represent this entire multi-step construction. Saved state and full private MCP traces preserve continuity. The work intentionally uses monochrome geometry and existing lifecycle controls; it does not claim new input, shading, audio, physics or general project-framework support. M9 human/platform/resource gates remain as recorded in the candidate matrix.
