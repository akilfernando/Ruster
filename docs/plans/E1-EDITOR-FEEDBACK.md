# E1 — Editor feedback batch

Started 2026-11-12. The user asks the root session to "complete the current phase and then continue the next phase". The root interprets the current phase as the [2026-11-11 feedback batch](../STATUS.md) collected while development was paused, and the next phase as [F4a Assembly Groups](F4A-ASSEMBLY-GROUPS.md). This reading is the root's interpretation of that instruction, recorded so the user can correct it.

## Current bounded authorization — 2026-11-13 selection slice

The user explicitly requests the next E1 viewport-selection slice after primitive closure. One implementation owner works on `claude/e1-editor-feedback`, starting at `35ecebef6b340c30bc3bcff323c37024fb7de198`. Frequent explicit-path checkpoint commits/pushes are authorized; installation, merge/release and E2/F4a are not. Parent accepts bounded selection at `869063f7a8f260e0949612e08a88ac9affcb6937` after fresh independent review `deleg_2b4cc899` passed without identified security/logic blockers, retaining all documented injected-input, touching-silhouette and custom-WGSL qualifications. The next explicit authorization is bounded wide-window spacing/selective icons; no new architecture or runtime behavior. Earlier stop/no-publication entries are historical and superseded only within this authorization.

Primitives are committed/pushed and bounded qualification is complete, including three actual Vulkan probes and native editor/runtime with the verified user-local Khronos core/synchronization layer. Parent fresh-context review passed. [Evidence and exact artifacts](../evidence/E1-PRIMITIVES-2026-11-13.md).

## Scope

| Item | State | Notes |
| --- | --- | --- |
| Light/dark themes, optional OLED black | Implemented in HEAD; separate qualification/user decision | System/Dark/Light plus a labelled OLED variant for evaluation. Keeping OLED remains the user's decision |
| Logo v2 integration | Implemented in HEAD; separate qualification | Theme-appropriate marks, window and launcher icon, DejaVu notice review |
| Sphere and capsule primitives, Create menu | Bounded qualification complete; committed/pushed | Source/native/export and actual core/synchronization validation pass; parent review passed |
| Viewport selection highlight | Accepted bounded rendering after fresh parent review | Default-off editor opt-in, two-tone visible-fragment inner contour; source/native/Vulkan/cost/runtime compatibility pass with explicit injected-input limitations. [Evidence](../evidence/E1-SELECTION-2026-11-13.md) |
| Wide-window layout spacing | Accepted bounded qualification after fresh parent review | Actual-width responsive top bar and viewport groups. [Evidence](../evidence/E1-LAYOUT-2026-11-13.md) |
| Selective icons | Accepted bounded qualification after fresh parent review | Original vector transport icons; named focusable buttons and enabled/disabled tooltips, other labels retained |
| Terminal-free MCP onboarding | Proposed separately as [E2](E2-MCP-ONBOARDING.md) | Needs user decisions before implementation |
| Reopen lands on the focused workspace | User to confirm | Hyprland rule was already corrected on 2026-11-11 |

## Selection creator acceptance contract (agent-selected, 2026-11-13)

- Clicking a visible cube, sphere, capsule or imported mesh clearly identifies that geometry directly in the viewport; white, black and mint materials remain distinguishable. Multi-selection marks visible selected surfaces, not unselected occluders or empty/background pixels. Picking, camera and selection ownership stay unchanged.
- A static mint inner contour with a charcoal backing replaces editor built-in material whitening. Default-off transient `NavigationOptions.selection_feedback` is enabled only by the editor; application selection retains its prior appearance. Existing instance response W and scene alpha carry the depth-tested/MSAA-resolved mask into the existing composite pass. The contour is applied after display mapping, before output transfer, and scales with UI pixel density. No new targets, passes, pipelines, mesh expansion, wireframe or animation; no PBR, authored color or runtime serialization changes.
- Verify supported System/Dark/Light/OLED theme choices, Legacy SDR/Surface HDR (including exposure extremes), perspective/orthographic, compact/wide and multiple separated objects. A touching/overlapping selected silhouette is a shared visible contour, not a per-object ID outline; do not claim hidden edges or wireframe. Custom project WGSL retains its existing appearance/alpha contract rather than interpreting arbitrary custom alpha as a selection mask.
- Add one focused visible-fragment regression to the existing opt-in Vulkan probe: selected contour across white/black/mint, occlusion, unchanged interiors and exact clearing restoration, both output conventions and no selection-triggered target/mesh/texture allocations. Preserve actual RED/GREEN and all failed preparations.
- Fresh-source native before/after evidence plus actual core/synchronization validation, resize/shutdown, matched release fixed-workload measurements, and before/after unselected offscreen/runtime pixel equivalence are required. Independent parent rendering review passed at `869063f` via `deleg_2b4cc899`; bounded acceptance retains all documented qualifications. Broader hardware/human familiarity and custom-shader outline support are not inferred.

## Primitive design (agent-selected, from a read-only design pass 2026-11-12)

Add `Object.primitive: Option<Primitive>` with `Sphere | Capsule`; `None` remains the unit cube so existing objects keep their meaning and JSON. `primitive` and `mesh` are mutually exclusive. Scene 17 → 18, content schema 13 → 14, new exact runtime capability `builtin_primitives`, Descriptor stays 5, recovery drafts stay 15 because checkpoints carry session snapshot bytes. Pre-18 files containing the key are rejected, including inside history payloads. Built-ins are not assets: they bypass the 64-asset limit and the 48 MiB admission total and count as fixed renderer overhead (60,640 bytes for sphere/capsule; 61,560 bytes total including cube and constant UVs).

Rejected: reserved mesh IDs (collide with valid asset IDs, old runtimes misreport a missing mesh, special cases in about eight places) and generated catalog OBJ assets (catalog changes sit outside scene undo, three MCP calls per shape, consumes asset capacity).

Geometry: sphere radius 0.5, 32×16, 559 vertices/960 triangles, analytic normals, longitude/latitude UVs. Capsule radius 0.5, height 2 along Y (the Unity default, chosen because the user asked for a Unity-style menu), 592 vertices/1024 triangles, arc-length V. Counter-clockwise winding matching the cube.

Editor: a top-level Create menu (Cube, Sphere, Capsule, Light), the same submenu in the hierarchy header, context menu and empty state, unbound remappable Create actions, and the procedural kind in the inspector mesh panel. MCP: optional `primitive` on `scene_create` and a new `scene_primitive` tool sharing history and revision guards.

Open for the user: whether the cube should also accept textured materials now (today it shows the missing-texture marker). Not included in this slice.

## Verification plan

Per [TESTING](../TESTING.md): extend existing scene migration/history cases, render pick/Fit cases, the editor object-command and export-profile cases, MCP schema cases and the Python packager cases; one Vulkan device smoke with shadows; a native keyboard journey through Create on workspace 2 without taking focus.
