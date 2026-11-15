# Native AI workspace layout repair — 2026-11-15

## Outcome and scope

The production `Host::ui` CPU composition reproduces the human-reported blank
window and vertically centered controls. The repair bounds the controls with an
egui horizontal row; it does not impose a fixed panel height or change the scene,
camera, connection, enrollment, permission, persistence or browser contracts.

**Source/headless verified; graphical acceptance pending.** The frozen human trial,
its checkout, binaries, launcher, registry and running native process are untouched.
No GUI/GPU launch, desktop capture/input, activation or installation occurred.

## Cause and behavioral RED → GREEN

`SceneEditor::ai_header` used `with_layout(right_to_left(Center))` directly in the
remembered `scene_top` panel. Unlike `horizontal`, this consumes the available
previous panel height. On wide windows the tools row below adds another 38 points
per frame. This feedback grows the panel until it consumes the entire window.
The centered controls then sit in the middle and the renderer allocation is below
the window, clipped away. This is a layout defect, not missing native approval.

At 1280×805, the unmodified production path initially allocates the viewport at
`[8,81]–[1272,762]`. By frame21 it settles at `[8,813]–[1272,814]`, with the
Full editor text at y394–410. `red-settled.log` retains that exact symptom.
With the row wrapper, frames0–39 keep the header at y9–25 and viewport at
`[8,81]–[1272,762]`. `green-qualified.log` records all representative sizes.

The new owning regression runs the actual `Host::raw_input_hook` and `Host::ui`,
including connection chrome, with a disposable saved, clean revision0 three-object
demo fixture. It asserts bounded top controls, an on-screen dominant viewport,
full-width natural allocation, a matching unclipped native scene paint callback,
frame-to-frame stability and unchanged session snapshot/dirty state. It covers
wide, narrow/tall, compact and native-DPI/UI-zoom cases with physical-pixel to
point conversion. The existing asset-loading helper settles fixture initialization;
a priming pass applies egui's initial zoom transition before measurement.
The fixture has no bound client: connection/auth is deliberately not fabricated.
A final-contract mutation replacing only `horizontal` with unconstrained `scope`
fails the stability assertion on frame1 (`red-final-contract.log`).

## Why earlier tests passed

The old layout test used `SceneEditor::ui` directly, only three frames per size,
and required 80% area only at width800 (below the900-point wide-tools threshold).
Other widths checked width and label containment, not top anchoring, viewport
height, on-screen position or subsequent-frame stability. Thus it missed the
accumulating wide-header height. The actual Host composition is now covered;
no evidence suggests Host itself or DPI caused the feedback.

## Verification and evidence

Audit root (private, not shipped):
`/home/akilf/Projects/Ruster-Audits/cumulative-editor-20261115/native-ai-workspace/native-layout-repair/`.
Owned reflinked target, affected editor/render artifacts invalidated before use.

- Focused production-Host regression: PASS; original RED and settled RED retained.
- Editor default suite:194 passed,5 existing ignores.
- Editor inspection suite:199 passed,7 existing ignores.
- Strict inspection/all-target Clippy: PASS.
- Default change-gate runner, format and documentation checks: see `change-gate.log`.
- Intermediate fixture failures remain in `green.log` and `green-final.log`: saved
  asset loading needed the existing settling helper; initial zoom needed a priming
  pass. These were harness preparation failures, not additional product repairs.
- The supplied screenshot attachment path was unavailable to the image tool in
  this worker; reproduction uses the exact geometry/symptom in the parent brief.

## Remaining guarded proof

Parent owns source review, replacement optimized build and enrolled identity
migration; do not replace the frozen candidate in place. Under fresh appropriate
user/desktop guards, qualify the replacement ordinary native Host in the actual
ChatGPT connected project: header remains top-anchored over subsequent frames,
all three objects are genuinely rendered at natural aspect, narrow/wide/scale
layouts remain usable, native camera/selection and Full editor preserve state.
CPU callback evidence is not GPU pixels or human visual acceptance. No additional
client connection or live activation is authorized by this repair handoff.
