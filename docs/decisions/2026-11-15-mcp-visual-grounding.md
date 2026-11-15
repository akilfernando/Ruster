# Capture-bound visual grounding for MCP — 2026-11-15

## Decision and authority
Akil identified image-to-engine object identity as essential for MCP editing, including selected-object and projection context. Extend the existing native observation path; do not create another editor, renderer or screenshot transport. This is an implementation contract, not a delivered capability.

## First bounded slice
- Extend the existing capture-ready response with selection/active object, projection/camera context and a versioned grounding capability/status from the same captured observation.
- Add one read-only capture-bound point query (`viewport_capture_pick`, final naming follows existing conventions) returning an actual engine object ID or an explicit miss/error.
- Coordinates are normalized to the delivered viewport image, top-left origin, and mapped through its exact crop/downsampling/DPI to the original native projection. Never use desktop coordinates or assume thumbnail aspect equals source projection.
- Reuse native renderer picking and hierarchy/mesh resolution. Color, apparent shape and names are hints, not authoritative identity. Do not auto-rename objects or synthesize aliases.
- Ground authored display initially. Runtime images remain available but explicitly report unsupported grounding; do not pick authored geometry beneath runtime pixels.
- Geometry picking excludes gizmos/HUD/light icons/selection outlines; report these limits. Do not claim exact composited-pixel ownership for antialiasing/transparency.

## Coherence, ownership and refusal
Use fresh-only validated recomputation rather than retaining another scene. Capture records must bind metadata/image, exact native scene/view/resource/matrix/layout/DPI identity, and authenticated peer/connection ownership. Keep MCP project/epoch and native authorization checks; capture IDs and model-supplied peer strings are not authority. Reuse the current capture owner/retention mechanism, extending it only as required.

Validate both completed inspection and point queries. Scene/view/resource/runtime/layout change, replacement, cancellation, authority loss or new capture invalidates the old grounding record. Never silently recapture, reinterpret coordinates against another view, reconnect, retry uncertain mutations or change selection/camera/history. Invalid geometry is not a background miss. Respect privacy/input/draft guards and existing output limits; bound extra metadata and fail explicitly on overflow.

## Deliberate deferrals
Projected object bounds/visible lists and numbered model-image annotations are follow-ons. Projected bounds are only candidates, not occlusion proof. No GPU ID-buffer project, historical scene cache, streaming, semantic renaming, browser work or runtime-grounding expansion in this slice.

## Acceptance
Behavioral RED→GREEN at production capture/renderer seams: correct IDs despite similar colors; frontmost geometry/miss/error distinction; perspective/orthographic, hierarchy, narrow/aspect/crop/scale mapping; stale and wrong-owner refusal including completed reads; immutable authored scene/selection/camera/Undo/Redo. Preserve the MCP image conversion and exact captured selection/projection pairing. Actual native/ChatGPT image-plus-point qualification remains a separate parent-owned gate after scoped review and source-matched optimized build.

Design basis: independently completed Astra read-only contract in the private native-ai-workspace audit, verified exact session/admission terminal completion. The working human trial remains frozen. Reassess the first headless vertical slice after about twenty minutes rather than expanding infrastructure or chasing unrelated historical gates.
