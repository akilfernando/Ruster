# Fable orchestration handoff — 2026-11-12

User-selected model: `claude-fable-5-2`, high effort, invoked through local Claude Code 2.1.322. Authentication reports existing OAuth login; no Anthropic key variable is exposed to the Codex process. This is an external orchestration session; the Codex root model was not switched.

Session: `d9b6d27e-3ce6-4c7c-a901-c10e2328ced2`. Resume from this repository with `claude --resume d9b6d27e-3ce6-4c7c-a901-c10e2328ced2 --model claude-fable-5-2`. The initial assignment allowed only Read/Glob/Grep, with external MCP disabled. No product writes, commands, tests, desktop actions or recursive delegation were permitted. Future worker assignments must explicitly set ownership and permissions.

Completed in approximately 228 seconds, 65 turns, no permission denials. CLI reports a list-price estimate of $2.472045, not proof of a charge or subscription consumption. Only the requested model appears in usage.

The following is the model’s read-only assessment, not acceptance evidence. Root independently confirms Scene 18 versus the export mapping ending at 17; other findings remain review inputs pending affected checks. Its Claude worker aliases describe a proposed external workflow, not models exposed by the Codex collaboration tool.

# Rusterize orchestration plan: E1 (current) and F4a early gate (next)

Nothing was run, built or edited. Every claim below comes from reading source and docs, so compile and test state of the working tree is unverified.

## Source versus documented state

| Topic | Documents say | Source shows |
| --- | --- | --- |
| Project state | `docs/STATUS.md` (2026-11-11): implementation paused, no E1 entry | `docs/plans/E1-EDITOR-FEEDBACK.md` started 2026-11-12; HEAD `510b00c` ships theme and logo work |
| Theme and logo v2 | E1 table: "In progress" | `ThemeChoice` with OLED variant in `crates/ruster-editor/src/editor_theme.rs:11`, View → Theme at `scene_editor.rs:4800`, dark/light marks in `branding.rs:9-13`. I found no evidence document or delegation-log entry for it |
| Primitives | E1 table: "Designed" | Scene and render layers are implemented but uncommitted: `Primitive`, Scene 18, `set_primitive`, pre-18 rejection (`crates/ruster-scene/src/lib.rs:152`, `:15`, `:3390`, `:1626`); generators, draw grouping, pick/Fit (`crates/ruster-render/src/primitives.rs`, `lib.rs:1033`, `:1201`, `:2019`) |
| Geometry counts | 561 and 594 vertices | The test asserts 559 and 592, payload 60,640 bytes (`primitives.rs:186-187`, `:251`) |
| Editor and MCP | Create menu, `scene_primitive` tool | Still cube-only: `scene_editor.rs:2054`, `:3079`, `:3297`, `:3422`, `:4643`, `:4727`; `crates/ruster-mcp/src/main.rs:364` |

Two defects in the uncommitted tree:

- **Export is broken for every scene.** Saving now writes Scene 18, and export requires the current version (`export_commands.rs:44`). `creation_export.rs:1153-1167` maps only Scene 1–17, and `scripts/package_creation.py:1070`, `:1091-1104`, `:1218`, `:1244` stop at schema 13. No `builtin_primitives` capability exists anywhere.
- **Texture admission disagrees between layers.** The renderer treats sphere and capsule as texturable (`ruster-render/src/lib.rs:1215`). `ruster-app/src/lib.rs:129-139`, `material_commands.rs:179`, `scene_editor.rs:2408` and `material_ui.rs:267/420/669/676` still require a mesh asset.

## Priorities from user feedback (STATUS lines 13–21)

1. Sphere and capsule through a Unity-style Create menu (half built, currently blocks export).
2. Viewport selection highlight. Today it is a 0.35 mix toward white (`lib.rs:1061`, `scene.wgsl:57`, `surface.wgsl:102`).
3. Wide-window spacing, then selective icons.
4. Theme and logo closeout; keeping OLED is the user's decision.
5. Terminal-free MCP onboarding stays a proposal (`docs/plans/E2-MCP-ONBOARDING.md`).

## Current phase: E1 slices

Builds, tests and native sessions are serialized under the root. One writer owns `scene_editor.rs` at a time.

| Slice | Owner | Write scope | Acceptance | Depends on |
| --- | --- | --- | --- | --- |
| P1 Scene 18 delivery closure | `opus`, medium | `creation_export.rs`, `ruster-app/src/creation.rs` and `lib.rs`, `scripts/package_creation.py` and its tests | Scene 18 → content 14, exact `builtin_primitives` profile, Descriptor stays 5; old kit rejected before staging; packager validates `primitive` including history and exclusivity; one shared texture rule | Baseline check |
| P2 Editor and MCP authoring | `opus`, medium | `scene_editor.rs`, `commands.rs`, material UI files, `ruster-mcp/src/main.rs`, both crate READMEs | Create menu in the four places E1 lists; unbound remappable actions; inspector shape kind; `scene_create` optional `primitive` plus `scene_primitive`; one Undo entry; stale and draft-pending rejections; Enter/Escape/keyboard traversal | P1 |
| P3 Native qualification | Root | `docs/evidence/` only | Vulkan smoke with shadows; keyboard Create → save → reopen → export → run, both shapes, on workspace 2 silent; fresh runtime kit | P2 |
| P4 Selection highlight | Root design, then `opus` | `ruster-render` shaders and `lib.rs` | Selected object distinguishable on white, black and mint objects in all three themes, with multi-select, in legacy and HDR lanes; exported pixels unchanged; cost measured on/off | P1–P3 committed |
| P5 Spacing and icons | `sonnet` or `opus`, low | Top bar and viewport tool groups in `scene_editor.rs`, `editor_theme.rs` | Root inspects wide (per the user screenshot) and compact widths; tooltips and accessible names; icon source licence recorded | P2 |
| P6 Closeout | Root | `STATUS.md`, E1 plan, delegation log, evidence | Corrected counts, theme and logo verification recorded, final gate once | All |

P1 and P2 are the two slices the skill requires metrics for in `docs/workflow/DELEGATION-LOG.md`.

Scope boundaries for E1:

- No cube textures (open user question in the E1 plan).
- No new primitive kinds.
- No recovery-draft or Descriptor version change.
- No E2 work.
- `.codex-attachments/` stays untouched and uncommitted.

## Next phase: F4a, early gate only

`docs/plans/F4A-ASSEMBLY-GROUPS.md` still says approval is pending, and STATUS line 39 calls it unapproved. I read your authorization as covering orchestration, not F4a itself or the four decisions in `docs/plans/F4A-EARLY-GATE-DESIGN.md:52-57`.

- **Preconditions:** E1 accepted and committed, so Groups take Scene 19 and content 15; your explicit go-ahead plus those four answers.
- **Stage 0:** flat baseline measurements, no code.
- **Stage 1:** one `opus` high writer for the scene model and resolver (`ruster-scene/src/lib.rs`, new `hierarchy.rs`). Then render and behavior/spatial in parallel on disjoint files. Then minimal Session/MCP, the packager and one exact runtime.
- **Review:** independent review of the resolver before any editor UI.
- **Stop:** at gate acceptance with a decision record, or at an evidence-backed hold. Shear, new lifecycle ownership or no runnable slice returns to you.

## Proposed first executable slice

1. Root runs `python3 scripts/check.py --dry-run`, then focused tests for `ruster-scene` and `ruster-render`, and records the result as the baseline for the uncommitted work.
2. Dispatch P1 with the contract above.
3. Stop and return to root if P1 needs a scene-crate semantic change or a Descriptor bump.
4. Checkpoint commit only once export passes again; the host integrates.

## Uncertainties

- **Phase reading:** "current = E1, next = F4a" is the earlier root's interpretation, recorded as correctable in the E1 plan.
- **Old-kit rejection:** making cube-only Scene 18 files require a new kit follows every earlier scene bump, but you have not confirmed it for this one.
- **Textured sphere and capsule:** this is agent-selected. The alternative is to reject textures on built-ins in this slice.
- **Unread material:** I did not read `ROADMAP.md`, `EDITOR-EXPERIENCE.md`, the testing skill, or the diffs in the four probe files and `shadow_gpu.rs`.
- **Unverified claims:** I could not confirm the E1 statement that recovery drafts stay at 15, or the model aliases in `MODEL-ROUTING.md`.
- **Still needs you:** workspace-reopen confirmation, the OLED decision and E2's four decisions.
