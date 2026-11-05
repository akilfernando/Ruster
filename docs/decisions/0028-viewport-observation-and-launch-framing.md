# 0028 — Native MCP viewport observation and explicit launch framing

2026-11-05. Status: accepted and implemented under the user's authorized Tideglass gap repair. Native observation/occlusion/stale-state and direct/MCP framing checks pass within the [PumpBench record](../creations/PUMPBENCH.md)'s limits; the separate viewer focused-keyboard gate remains open.

## Context

Tideglass could be authored through native MCP, but needed developer UI inspection for visual feedback, camera framing and presentation selection. Editor navigation was not included in the standalone launch description. The user requested these gaps be repaired before a realistic industry creation exercise. Rust/Vulkan, lean implementation and human/agent interleaving remain requirements.

## Decision

Expose existing frontend-owned camera and selection paths through guarded transient commands. Scene/session and independently observed view revisions prevent an agent from replacing newer human navigation. Actual UI frames observe camera/selection changes, including changes back to an earlier value. These operations do not edit authored history or runtime transforms.

Provide an on-demand asynchronous viewport observation using eframe's owned-window screenshot callback, followed by a viewport-only crop and bounded PNG thumbnail. Do not use desktop screenshots, global input, a developer-inspection listener or arbitrary output paths. All UI is composed before screenshot admission; conservatively reject every visible floating/window/popup/tooltip layer, including egui's preceding-pass retained areas. A request pins scene generation/revision, view revision, runtime frame/run identity, catalog/shader revisions and viewport geometry. Changed state, occlusion or missing rendering yields a truthful stale/busy/pending/expired outcome.

Capture IDs are process-lifetime unique. A process-wide readback lease follows the asynchronous callback and encoder, including invalidated/timed-out requests; obsolete callbacks skip encoding. Admission is limited to8,388,608native-window pixels; returned thumbnails are at most512pixels on the longest side and40,000PNG bytes. The64KiB bridge framing limit is preserved. The MCP adapter returns an image content block without duplicating its base64 in text/structured metadata. No recurring capture runs while idle. These implementation limits are not a measured total-memory ceiling.

Standalone framing is an explicit export parameter, analogous to the title, distinct from incidental editor preferences. Direct users choose **Use current view**; MCP callers supply optional `initial_camera`. The request freezes that choice. Absence preserves legacy Fit behavior. The descriptor camera uses shared-renderer validation and initializes navigation once. Framed export requires advertised runtime-template capability and verified runtime/receipt agreement; old strict readers are not assumed to understand new optional fields. Immutable earlier runtime kits remain unchanged. A real fitted camera exposed JSON f64 roundtrip drift in receipt validation. Compare both strictly validated renderer `CameraState` values at their actual f32 precision, including absence/null semantics, rather than intermediate JSON-number equality or an arbitrary epsilon. Changed, invalid, missing and unrequested framing still reject.

## Alternatives and consequences

Developer UI inspection would preserve an MCP dependency gap. Continuous screen capture would add unnecessary work and wider information exposure. Render-target readback could avoid native UI composition but would introduce another GPU capture/ownership path. Initially reuse eframe and refuse occluded observations; measure the conservative blocking and full-owned-window readback cost in the real workflow.

Saving every camera movement into the scene would mix navigation with authored intent. A general presentation framework would exceed this repair. Explicit export framing provides a launch choice now; persistent named cameras remain future work.

Verification must cover actual rendered images, overlapping windows, stale/interleaved changes, asynchronous retirement, keyboard paths, old/new runtime compatibility and relocated launch framing. Source/unit checks alone do not establish native behavior.
