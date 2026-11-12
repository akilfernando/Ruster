# E1 — Editor feedback batch

Started 2026-11-12. The user asks the root session to "complete the current phase and then continue the next phase". The root interprets the current phase as the [2026-11-11 feedback batch](../STATUS.md) collected while development was paused, and the next phase as [F4a Assembly Groups](F4A-ASSEMBLY-GROUPS.md). This reading is the root's interpretation of that instruction, recorded so the user can correct it.

## Scope

| Item | State | Notes |
| --- | --- | --- |
| Light/dark themes, optional OLED black | In progress | System/Dark/Light plus a labelled OLED variant for evaluation. Keeping OLED remains the user's decision |
| Logo v2 integration | In progress | Theme-appropriate marks, window and launcher icon, DejaVu notice review |
| Sphere and capsule primitives, Create menu | Designed, see below | Starts after the theme work so one writer owns the editor crate at a time |
| Viewport selection highlight | Not started | Existing highlight is a faint white mix in the scene shaders |
| Wide-window layout spacing | Not started | Top bar and viewport tool groups |
| Selective icons | Not started | With tooltips and accessible names |
| Terminal-free MCP onboarding | Proposed separately as [E2](E2-MCP-ONBOARDING.md) | Needs user decisions before implementation |
| Reopen lands on the focused workspace | User to confirm | Hyprland rule was already corrected on 2026-11-11 |

## Primitive design (agent-selected, from a read-only design pass 2026-11-12)

Add `Object.primitive: Option<Primitive>` with `Sphere | Capsule`; `None` remains the unit cube so existing objects keep their meaning and JSON. `primitive` and `mesh` are mutually exclusive. Scene 17 → 18, content schema 13 → 14, new exact runtime capability `builtin_primitives`, Descriptor stays 5, recovery drafts stay 15 because checkpoints carry session snapshot bytes. Pre-18 files containing the key are rejected, including inside history payloads. Built-ins are not assets: they bypass the 64-asset limit and the 48 MiB admission total and count as fixed renderer overhead (about 61 KiB).

Rejected: reserved mesh IDs (collide with valid asset IDs, old runtimes misreport a missing mesh, special cases in about eight places) and generated catalog OBJ assets (catalog changes sit outside scene undo, three MCP calls per shape, consumes asset capacity).

Geometry: sphere radius 0.5, 32×16, 561 vertices/960 triangles, analytic normals, longitude/latitude UVs. Capsule radius 0.5, height 2 along Y (the Unity default, chosen because the user asked for a Unity-style menu), 594 vertices/1024 triangles, arc-length V. Counter-clockwise winding matching the cube.

Editor: a top-level Create menu (Cube, Sphere, Capsule, Light), the same submenu in the hierarchy header, context menu and empty state, unbound remappable Create actions, and the procedural kind in the inspector mesh panel. MCP: optional `primitive` on `scene_create` and a new `scene_primitive` tool sharing history and revision guards.

Open for the user: whether the cube should also accept textured materials now (today it shows the missing-texture marker). Not included in this slice.

## Verification plan

Per [TESTING](../TESTING.md): extend existing scene migration/history cases, render pick/Fit cases, the editor object-command and export-profile cases, MCP schema cases and the Python packager cases; one Vulkan device smoke with shadows; a native keyboard journey through Create on workspace 2 without taking focus.
