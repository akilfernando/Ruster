# Status

Updated 2026-11-24. See the [archive](archive/STATUS-2026-11-17.md) for earlier status.

## Where to work

- **Trunk:** `main`, on `origin`. Branch from it and merge back to it.
- **One checkout:** `/home/akilf/Projects/rot_engine`. Older branch tips are
  `archive/<name>` tags on `origin`. Two git stashes hold
  small status notes from older sessions. The detached worktrees under
  `~/Projects/Ruster-Audits` and `~/Projects/Ruster-Creations` are retained
  evidence; leave them.
- **Push policy (user, 2026-11-17):** fully autonomous. Push, open pull
  requests, review, merge and keep branches tidy without asking.

## What to do next

The 2026-11-19 Phase 1 pass reviewed Scene, Inspector, Assets and Recovery in
Rot and Plain, confirmed all nine Transform fields at 960×1080 and 1.5× UI
scale, and verified shared Undo over STDIO. The
[human walkthrough](research/EDITOR-WORKFLOW-COMPARISON.md#proposed-editor-baseline-walkthrough)
also exercised concurrent ChatGPT editing. Native tests covered script errors
and Play/Pause/Stop. The full UI checklist remains open.

The [C-01 design](plans/C-01-OBJECT-COMPONENT-MODEL.md) guides the v23 cutover.
C-02 preserves authored content during migration, supports grouped object
edits and Undo/Redo, and resolves a 5,000-object chain over 1 MiB. The Project
Hub's Empty template creates v23. The editor can inspect objects, edit name,
enabled state and Transform, add Mesh Renderers and Colliders, create,
move or delete objects together, and save. Inspector edits shared Transform
and component values. Viewport settings stay local. Automatic upgrades remain open.
Assets → Move to Folder moves a registered OBJ, Lua or WAV source with its
sidecar and keeps references on the same ID. A moved vehicle reopened and
kept its mesh, script and sound through approved MCP and a relocated Linux
build. Source, sidecar and packaged asset hashes were verified.
In a 604-asset release project, request-to-catalog p50/p95 was 76/83 ms
for Move and 61/66 ms for Rename. The first accessibility tree reply
after submission was 32 ms p95 for Move. Settled editor CPU stayed within
5–10 ms/s before and after the paired runs.
Hierarchy search finds names or stable IDs, shows parent paths, and reveals
the exact object. Approved MCP provides revision-pinned pages. In two
5,000-object release runs, first results appeared in 21–23 ms, warmed
keystroke-to-results p95 was 49–50 ms, and reveal took 53–65 ms through
the native accessibility round trip. With the query field focused,
later idle intervals used 20–25 editor CPU ms/s versus 0–10 with search
closed. The first interval after typing used 70–75 CPU ms/s.
Hierarchy branches fold locally. Arrow keys and Shift selection follow the
visible rows. Search and Find Uses open the ancestors needed for the reveal.
The station fixture verified selection and draft retention with identical
saved bytes and scene revision. Approved Claude inspected a folded renderer
and copied the seven-object wing. Native Undo restored it. The disclosure
focus ring was verified in Rot and Plain. At 5,000 objects, release action
to updated accessibility tree p95 was 46 ms for collapse and 54 ms for
expand across three runs. Idle editor CPU remained about 7 ms/s in both states.
File → New Scene and the command palette create a named, saved v23 sibling.
The new scene has its own history and MCP session. Same-project switches keep
Assets and the startup setting. Competing files and cancelled creation leave
the original workspace intact. Approved Claude reused an imported Blender
model across two independently saved scenes, and relocated Vulkan loaded
the new Title. Release Create reached the first editable tessellated frame
in 0.867 ms p95. Same-project Open kept the loaded catalog and reduced
equivalent sibling Open from 5.711 to 0.811 ms p95.
Project Settings can designate a saved v23 sibling as the startup scene.
The Hub opens it and Linux Build packages its saved dependency closure.
Explicit file Open uses the chosen document, and Play runs the active scene.
Approved MCP reports each route. A relocated Title build retained its texture
while an unsaved Rail Yard remained open in the editor.
Project Settings → Player controls the Linux window title and initial
logical size. Build and approved MCP show effective values and provenance.
Rot and Plain authoring, a refused stale edit, an old default package and
a relocated 1280 × 720 window were verified. Start retained its title and
size, and manual resize worked. At matched 1100 × 760, 20 release launches
per build reached first presentation in 205/225 ms p50/p95 before and
210/241 ms with settings. Steady GPU scene p95 held at 0.049 ms.
A configured Start Button can enter one saved v23 sibling at tick zero in
Play and relocated Linux Build. Approved MCP reports separate authored
and committed runtime scene identities. The built Title and Rail Yard
carry verified asset closures. A failed activation keeps the prior frame.
A 5,000-object destination is prepared off the standalone render update, cutting
the worst Start update from 35.819 to 0.108 ms in release. The first
completed destination frame arrived in 35.0–51.5 ms while Title continued
rendering.
File → Open Scene and the Start destination share a searchable Project
Scenes picker. Open, Startup and Start destination markers identify the
exact saved siblings. A replaced file is rejected before Open or Set
Transition. An admitted Open drains the prior AI connection before switching.
In release tests, the first results appeared in 13.4 ms p95 on the sample
project and 43.3 ms p95 with 480 extra scenes. A 5,000-object destination
validated in 61.6 ms p95, including the native inspection round trip.

**Copy and Paste Local Transform** reuses an authored Position, Rotation,
Scale or all three on selected v23 objects in one Undo. The station fixture
used four Blender spotlight housings. After the source rotated again,
native Paste Rotation applied the earlier snapshot to three lamps under
differently rotated arches. Position, scale, IDs and components stayed
intact. Approved Claude copied the source's current rotation by exact IDs.
Stale, missing-target and pending-draft requests left the scene untouched.
Paste added only the prefab lamp's Rotation override and retained its
existing Scale override. Source asset bytes were unchanged. After save
and reopen, Vulkan displayed the three expected beam directions in editor
Game view and in the relocated Linux player. Plain Light was checked for
menu focus and readability.
At 5,000 objects, three paired release runs measured command to first CPU
tessellation p50/p95 at 4.169/4.925 ms for one existing batch edit and
4.267/4.483 ms for one paste. For 100 objects, batch measured 4.756/5.235 ms
and paste 4.649/5.286 ms. With the menu closed, idle median was 0.625 ms
before Copy and 0.630 ms after. The stored snapshot performs no idle
polling or scene scans. Native release command to a revision-matched
rendered capture p50/p95 was 126.8/138.5 ms for one batch edit and
127.2/135.8 ms for one paste. For 100 objects, batch measured
129.0/144.8 ms and paste 127.4/141.3 ms. Each group had 12 samples.
The native metric includes MCP and GPU readback.

**Number Properties** let v23 objects sharing a file-backed Lua script
use different finite values. The Inspector shows the exact Lua property
read. Native keyboard entry set two conveyor carriers to 1 and 3 units
per second, producing 0.2 and 0.6 units in 12 fixed ticks. Nonfinite and
duplicate names left the prior values intact. Approved Claude changed
only the first carrier to 2 and preserved its existing sound cue. Native
Undo, Redo and removal behaved as individual history entries. The Lua
source hash stayed identical through authoring, Save and Build.
After reopen, 12 ticks moved the carriers by 0.4 and 0.6 units. The
relocated Linux build measured 4 and 6 units over 120 ticks. Vulkan
validation passed, and the visible markers moved 130 and 194 pixels over
the same sampled interval. Plain Light was checked for numeric field focus.
A native window close preserved the open draft, and Escape discarded the
draft without changing the scene.
Three paired release runs with 240 file-backed Object bindings measured
warm Step p95 at 2.843–2.874 ms with a constant and 2.905–2.930 ms with
one Number Property. Median Play preparation was 0.538–0.539 ms and
0.580–0.583 ms, respectively. Projection through ready runtime was
3.754–3.759 ms and 3.904–3.908 ms at the median. Each run timed 750
post-warmup Steps. The original Lua file was offline for Step, and
the compile cache recorded zero misses.

**Borderless Fullscreen** is available in Project Settings → Player for
Linux builds. Windowed remains the default for older settings and packages.
Approved MCP read the configured mode, its source and the published Build
receipt. A relocated Vulkan build opened at 1920×1080 on private Xwayland,
kept the 4:3 camera framing and fullscreen mode through the HUD Start
transition, and restored the manually resized 1120×680 window after F11.
Gameplay ticks continued, and the gameplay action bound to F11 received no
observable press through the press, repeat and release sequence. The older
package started at its original 960×720 size. Private native Wayland also
confirmed the fullscreen → windowed → fullscreen round trip. Plain Light
was checked for selector focus, and Vulkan validation passed for the
current and older package runs. The editor reopened the saved mode at
1920×1080. On private Xwayland, 12 warmed launches
per mode reached first Present at 199.265/219.421 ms p50/p95 windowed and
188.729/201.991 ms fullscreen. The first Present on all fullscreen
launches still reported 960×720. The first Present at the confirmed
1920×1080 fullscreen size was 226.237/240.563 ms. Across 12 warm round
trips, F11-to-Present at the expected size was 40.259/44.726 ms entering
fullscreen and 40.473/57.311 ms leaving. Three native Wayland release runs
per mode used matching 1920×1080 drawables and 1385×1039 scene targets,
with one draw and five mesh uploads in each. Windowed GPU scene p95 was
0.455–0.572 ms and fullscreen was 0.584–0.901 ms. CPU scene preparation
p95 was 0.089–0.092 ms windowed and 0.087–0.091 ms fullscreen. The
windowed compositor had a larger virtual display to allow the matching
client size with decorations.

**WAV audition** plays a ready v23 WAV once from the Linux Assets Inspector
and exposes its duration and sample rate in the editor and approved MCP.
Selection opened no output stream. The 0.370 s, 32 kHz bell completed
naturally and returned to Preview. Private stereo capture verified Stop,
selection change, accepted reimport, missing source, project switch,
close and entering Play. An absent device gave an actionable error.
Preview and replay opened neither WAV source nor its cache and preserved
the scene revision and simulation tick. Approved Claude assigned the
shorter bell by metadata. Shared Undo, save and reopen preserved the
intended assignment. The relocated Vulkan player played the exact WAV for
0.37 s and moved the signal. The FFT peaks were within 0.51 Hz of the
editor preview. The Preview control's focus was checked in Plain Light.
With a WAV selected in the 5,000-object release scene, idle CPU p95 was
0.561–0.578 ms before and 0.559–0.568 ms after. During audition of the
10-second WAV it was 0.632–0.646 ms. Request-to-first-private-PCM p95 was
60.533 ms cold and 69.418 ms warm for the short clip, and 43.361 ms cold
and 65.915 ms warm for the 10-second clip. The cold groups had three
samples each. One device stream was present during audition and zero
before preview and after Stop.

**Place Selection on Ground** moves a v23 assembly to world Y=0 through
the Hierarchy context menu, keyboard command palette or guarded MCP. The
Blender pump and a separate crate moved down 2.523369 units together, with
their horizontal positions, orientation, scale and spacing preserved. The
selected pump descendant moved once with its parent. Approved Claude moved
the below-ground copy up 2.476631 units. Native Undo and Redo reversed and
restored the whole edit. After save and reopen, the relocated Vulkan build
matched the editor's side-view landmarks. The pump met the floor within
one pixel and the crate retained its 0.791631-unit gap. A fresh placement used
the reimported foot. Stale asset guards, missing geometry and an empty assembly
left the scene intact. Disabled renderer geometry counted, and a grounded
assembly added no history. MCP reports the source minimum and displacement.
The menu was checked in Plain Light. In the 5,000-object release scene,
the small two-root placement took 3.508–3.908 ms p50 and 3.599–6.342 ms p95
through the first tessellated frame across three paired runs. Three small-only
repeats measured 3.789–4.130 ms p95. The 4,999-renderer subtree, including 500
instances of a 23,552-triangle mesh, took 4.165–4.372 ms p50 and 4.410–4.754 ms
p95. Idle CPU p95 was 0.642–0.643 ms before and 0.630–0.641 ms after, with
zero idle geometry scans.

**Fit to Mesh** sets a v23 Box Collider's local Center and Size from the
selected same-object Mesh Renderer. The parented loading-bay barrier's
center was (1.25, 0.5, 0.25) and Size was (1, 2, 3). All eight world corners
matched the rotated, scaled outline. A flat sheet used the minimum
thickness and kept its disabled Sensor state. The picker used the exact
chosen renderer. Missing geometry and an old asset revision were refused.
A fresh fit used the reimported wider mesh. Approved Claude fit the original
barrier and shared native Undo. Save and reopen preserved the fit. The
kinematic sphere stopped at x = 2.250100 in editor and relocated Vulkan Play,
compared with x = 1.000100 for the original box. The relocated Build contained
the exact accepted OBJ bytes. The picker and focus were checked in Plain Light.
In three 5,000-object release runs, time through the updated tessellated
outline was 3.278–3.548 ms p50 and 3.358–3.860 ms p95 for the 108-triangle
mesh. At 23,552 triangles it was 3.614–4.127 ms p50 and 3.920–4.230 ms p95.
The warmed idle outline scanned zero mesh vertices. Idle CPU frame p95
was 0.718–0.720 ms before and 0.722–0.733 ms after.

**Game HUD Action Buttons** give a named Button action an on-screen control
through Window → Game HUD. A completed activation supplies one tap to the next
simulation Step. The Blender drawbridge matched its keyboard-input pose at
tick 13. Releasing outside the button or losing focus kept the bridge still.
Holding E through a HUD click during Play preserved the held action and did
not fire a separate Primary Mouse binding. Native edits shared Undo and Redo.
Approved Claude authored a second button, activated it through guarded MCP
and verified the applied Step. Missing and Axis targets named the rejected
action. The saved game reopened, and click and Tab → Enter both moved only
the intended bridge in a relocated Vulkan build. The editor form's focus
ring was inspected in Rot Dark and Plain Light. Across three warmed release
runs, the 5,000-object static-label HUD measured 16.68–17.05 µs CPU p95
before and 17.07–17.46 µs after. Idle buttons measured 20.35–20.62 µs and
successful release frames measured 20.79–21.05 µs. Lua Step with the tap
merged measured 20.96–21.29 µs p95. HUD input and paint reused one prepared
layout within each frame. The warmed HUD cache reformatted zero strings.

**Orthographic GLB cameras** place editable v23 Camera components on the
original nodes. The Blender workshop kept its rolled camera and a second
node sharing the lens under their imported parent. Explicit Reimport added
both lenses from the previously accepted bytes. Place kept the active Game
Camera until approved Claude inspected and designated the workshop camera.
A separate edit to the second camera left the first lens intact. Native Undo,
Redo, save and reopen kept the expected designation and lens values. Editor
Play and the relocated Vulkan player matched the Blender 3:2 framing at
16:9 and 4:3 window sizes. Near and far rulers had equal pixel spans, and
both clipping witnesses were absent. A zero near plane omitted the lens
with an actionable warning. Invalid magnification kept the last good model
and the saved active camera. Across three paired release runs, median p95
first import was 0.589 ms with or without the lenses. Placement was
0.051 ms with them and 0.048 ms without. At 5,000 authored objects,
CPU preparation p95 was 0.390 ms for the imported camera and 0.388 ms for
the matching hand-authored camera. GPU scene p95 was 0.313 ms in both cases.
Draw and upload counts matched.

**Restore Missing Source** returns previously accepted Lua and WAV files to
their original paths through Assets. The signal box restored both exact source
digests with the same sidecars, IDs, scene references and history. Approved
Claude inspected the recovery candidates and ran a guarded Step that moved the
signal and emitted one bell cue. The reopened scene built successfully, and a
relocated Vulkan player moved the signal and played the restored bell into an
isolated audio sink. Corrupt cached bytes were refused in the editor. Worker
tests covered changed sidecars, unsafe folders and a competing destination.
The selected idle Inspector and five MCP inspections produced zero watched
cache reads. The new action received focus and restored the same file in the
targeted Rot Dark and Plain Light checks. Median warmed release p50/p95 to
Ready was 2.638/2.966 ms for a 42-byte Lua file and 12.978/13.909 ms for a
1,920,044-byte WAV. Normal watcher reimport after an atomic backup put-back
took 37.498/38.277 and 45.517/46.532 ms. Concurrent editor CPU frame p95
was 0.974 and 0.871 ms during restore and 0.890 and 0.839 ms during reimport.

**Sphere Collider radius fitting** extends Scene view Edit Collider. The
parented robot shell grew from 0.55 to 0.90 in perspective and orthographic,
with its local offset and resolved center intact. Escape preserved the prior
radius, and one Undo reversed a committed drag. A nonuniform parent made the
sphere unavailable in exact MCP inspection. Approved Claude adjusted the
flat kinematic robot from 0.5 to 1.0. At the same wall, two deterministic
Steps moved its stop from 0.850100 to 1.350100. The saved scene reopened with
the same result, and the relocated Vulkan player reproduced the stop. The
guide stayed in Scene view. Focus and handle contrast were checked in
Rot Dark and Plain Light. At 5,000 objects, median release CPU p95 through
tessellation was 0.719 ms with the sphere guide off, 0.742 ms visible and
0.777 ms while dragging. The previous Box path measured 0.724/0.753/0.778 ms,
and the current Box path measured 0.719/0.752/0.780 ms. The offscreen Vulkan
test held one target and four scene draws, with zero further mesh or texture
uploads over 120 warmed frames in each sphere state.

**Lua error navigation** connects v23 Game Error Details to the configured
editor. Two same-named files reproduced a Step error at line 8 with the
previous committed poses intact and a startup syntax error at line 4. Isolated
Neovim verified the path and cursor. After an Assets rename, the captured run
opened the renamed file at line 8. Changed or missing contents withheld the
jump. File-only editor configuration opened the source at its beginning.
Approved Claude identified the file, binding and line from runtime inspection.
After repair, Play advanced past the old failure. File watching recorded zero
source reads across 23 healthy ticks and a paused idle interval. The new action
was checked for keyboard focus in Rot Dark and Plain Light. At 32 owners,
three warmed release runs gave median Reset preparation p95 of 67.6 to 67.0 µs
with one shared source and 302.6 to 308.4 µs with 32 sources. Failed Reset to
the first assembled inspection report was 160.8 to 171.7 µs and 791.1 to
785.7 µs, respectively.

**Save Scene As** creates a v23 sibling with a fresh scene ID, stable object
and component IDs, and a clean Undo history. Market Night retained the linked
stalls and their local override, Game Camera and textured banner. It included
Claude's unsaved move and a valid Inspector name draft. Cancel, invalid name,
collision and a refused write preserved Market Day's edit context. The original
scene, project routing and asset bytes and modification times matched after
publication. Approved Claude reconnected, inspected the fresh ID and edited the
retained banner. Both scenes reopened independently. The relocated Vulkan build
rendered the variant after it was chosen as startup. Buffered staging brought
the first paired 5,000-object dispatch from 464.380 to 45.229 ms p95.
Across three warmed release runs, Save As reached the first editable frame
at 49.8–50.2 ms p50 and 51.5–52.0 ms p95. Save then Open reached it at
53.4–54.1 ms p50 and 54.7–56.2 ms p95. Peak process RSS was 79.5–80.3 MiB
and 74.4–75.0 MiB, respectively.

**Unpack Prefab Instance** detaches one v23 root while retaining its
materialized hierarchy and local edits. The middle cottage kept its roof
width, local textured material and IDs. It accepted a new chimney. Source
roof changes reached its two linked neighbors. Approved Claude unpacked the
exact root, stale replay was refused, and native Undo restored the link and
overrides. Missing-source recovery, saved reopen and the relocated Vulkan
player were verified. Unpack and Undo kept the source and asset revision
intact. The shared confirmation was checked for keyboard focus and readability
in Rot Dark and Plain Light. In a 5,000-object release scene, median p95 over
three runs was 2.795 ms to unpack and 2.125 ms to undo for four objects, and
2.749/2.140 ms for 200 objects. Idle editor CPU through tessellation moved from
0.461 to 0.453 ms p95 for four objects and 0.483 to 0.488 ms for 200.

**Primary Mouse** can share a v23 Button action with Space in Project
Settings. Approved Claude added the mouse source while retaining Space.
A stale edit was refused, native Undo restored the original binding, and
an approved logical Step matched the same input contract. The saved Signal
Box verified matching press and release counts in editor Play and a relocated
Linux player with Vulkan validation. The ownership claim, HUD button,
letterbox and Project Settings produced no gameplay press. An outside release
and focus loss cleared held input. The new checkbox was checked in Rot Dark
and Plain Light, with one editor gameplay journey. At 32 actions, three warmed
release runs gave median p95 of 2.923/2.550 µs for keyboard-only/mixed admission,
latching and snapshot, and 24.267/24.268 µs for fixed Step with the same
logical frames.

**Scene Background** gives v23 Scene Display a solid color picker and sRGB
hex entry. Approved Claude changed the background under a scene guard, and
native Undo restored it. The saved title and yard cleared correctly in Scene,
Game and a relocated Vulkan player. Default Game images matched exactly.
The authored clear kept the tested grid coverage, cube lighting, HUD and
camera bars. At 5,000 objects, warmed GPU scene p95 was 0.364 ms for the
previous and current default and 0.362 ms with the solid clear. Each used one
target and two scene draws.

**Play Checks** support saved flat v23 scenes in the editor and approved MCP.
The 30-tick conveyor check passed both ways and kept the live preview paused.
A saved scene edit and a Lua speed change each made the check out of date.
Recapture kept the expectation at 0 and reported an actual position of 1,
matching guarded normal Steps. A 640-object static scene reached tick 120
and verified its last object. A 120-tick release check with 240 objects
and 41 Lua attachments completed in 118 ms median p95 across three runs.
With the dialog open, editor accessibility inspection p95 was 42 ms idle and
49 ms during checks. Cancellation stopped the isolated run.

**Material Tiling, Offset and Wrap** work in v23 Edit Material. Base Color
and Roughness share the sampled UVs. Compatible Blender scale and offset
import directly. An earlier accepted file reopened pixel-identically and
Reimport exposed the new mapping for subsequent placements. Approved Claude
changed only the offset and native Undo restored it. Repeat and Clamp matched
independent baked UV frames, and 25 sampled landmarks matched in a relocated
Linux Vulkan build. At 5,000 visible placements, the median release instance
upload p95 was 0.156 ms before and 0.168 ms with tiling support. Instance
payload rose from 860 to 940 KB. Scene GPU p95 was 0.383 ms before and
0.371–0.381 ms with identity or tiled materials, with one draw and zero warmed
texture uploads.

**Animator Button triggers** start an imported clip on a committed press.
The Inspector and approved MCP share the setting and Undo. Two Blender
drawbridges verified independent starting, holding, restarting, looping and
final poses, with rollback on a rejected Step. Save and reopen, a native repair
for a missing action, and relocated Linux playback passed. With 16 placements,
median release Step p95 was 0.40 µs waiting, 194 µs with one active and 200 µs
with On Play. Waiting placements reused the draw snapshot.

**Linux archive (.tar.gz)** packages an ordinary Linux Build as one
file. Folder stays the default and can launch through **Build and Run**.
System tar extracted the same 11 files and modes as the folder build.
The relocated player started the rail yard, ran Lua movement and played
one WAV cue. Cancel during compression cleaned staging and a collision
preserved the existing file. Approved Claude read the result. In three
warmed release runs per format, median Build time was 0.309 s for the
50.90 MiB folder and 0.609 s for the 24.45 MiB archive. Sampled RSS
increased by 40–49 MiB across both formats. The editor inspection
round-trip p95 ranges overlapped.

The built-in **Cylinder** is centered, 2 units tall along local Y and
0.5 units in radius. A numbered conveyor roller verified the side wrap,
both flat cap textures, end-on cap picking and its cast shadow. An
approved Claude edit placed a second roller in its parent frame and
shared Undo. A relocated Vulkan build used one texture and no external
mesh. At 1,000 instances, median release extraction p95 was 0.233 ms
built-in and 0.377 ms for the imported reference with the same triangles.
GPU scene p95 was 0.340 and 0.337 ms. Shadow p95 was 0.079 and
0.077 ms. Both used one scene draw and no mesh uploads in warmed frames. The importer expanded its vertex buffer.

After an unexpected exit, **Recover as New Scene** opens the latest
completed v23 checkpoint as a sibling. Native and approved Claude edits
survived a private editor crash and the recovered script ran after opening
the copy. The original and its startup choice kept their exact bytes. A
5,000-object scene recovered and a verified Save retired its checkpoint.
In two warmed 5,000-object release runs per build, UI-thread CPU from
rename submission to commit was 16.66–18.98 ms p95 before recovery and
18.59–19.21 ms with it. A 4.10 MB checkpoint appeared 2.06–2.27 s after
the last observed edit, including the quiet period. No writes were observed
during four-second clean and settled-dirty idle checks.

On Linux, Assets → Create → Lua Script creates a starter file in the chosen
folder and selects its stable asset once ready. Open in Code Editor opens
that source using the saved editor configuration. The conveyor fixture
kept 0.5 units per second after a disk save and used 1.5 on the next Play,
after reopening and in a relocated Vulkan build. Approved MCP attachment
shared native Undo and left the source intact. In a warmed release
session with 600 mesh assets, p95 was 211 ms from Create to Ready and
164 ms for the equivalent guarded import. A source save reached Ready in
333 ms p95 through the watcher and in 235 ms with immediate explicit refresh.

v23 Transform Game Cameras can use an Orthographic projection with a full
vertical size, finite near and far planes and optional fixed aspect. Two
equal rulers at different depths drew equally in native Game view, an
approved Claude capture and a relocated Linux player. Clipping, foreground
occlusion and parallel specular response were validated on Vulkan. Native
Undo restored the saved lens after a guarded AI edit. A committed paused
audio cue kept the expected pan and gain and emitted no PCM. At 5,000
objects, median extraction p95 was 1.241 ms for either projection.
Perspective and Orthographic kept two scene draws and no shadow passes in
this workload. The GPU scene p95 ranges overlapped.

Double Sided draws both faces of an opaque v23 material. Opaque GLB import
and explicit Reimport preserve the setting for new placements. Existing
assignments keep their authored value and share Undo. A Blender sheet
verified back-face picking, mirrored artwork, depth and lighting in SDR
and Surface HDR, plus shadows in both sun directions. Single-sided views
matched prior pixels. A 5,000-sheet release run retained one color draw and
pipeline for each uniform mode. Mixed kept two. Median GPU scene p95 was
0.095 ms single-sided, 0.108 ms double-sided and 0.102 ms mixed. Shadow
p95 stayed at 0.012 ms in this fixture.

The v23 Inspector shows live Transform and script state during Play and Step.
An imported model's Animator lists same-model clips and their durations.
Native and approved MCP edits choose a clip and Loop setting with shared Undo.
A drawbridge held its final pose or looped as authored through Play and Linux
Build, after a source refresh retained the selected clip's ID.
The Inspector and approved MCP can preview one exact Animator before Play.
Scrubbing includes the final pose with Loop enabled and preserves saved
authoring and Undo. Blender drawbridge captures verified the pose and an
unchanged peer. Preview-to-viewport CPU submission was 6.4 ms p95 on the
16-object fixture and 7.1 ms with 5,000 objects. MCP capture with a verified
pose took 191 ms and 190 ms p95, including transport and image readback.
At the zero pose, parked viewport CPU prepare p95 was 98–107 µs on the
small fixture and 1.01–1.19 ms with 5,000 objects. Authored-rest phases
measured 101–118 µs and 0.99–1.06 ms respectively. Offscreen Vulkan
timestamps varied across repeated rest phases while the desktop was in
use, so the sample does not isolate the GPU cost of parking.
Assets watches Lua files. Add Component → Script and approved MCP
attach the same stable file reference. New Play runs pick up accepted
edits, and Linux Build embeds a verified copy of the source. v23 Input
Actions in Project Settings and approved MCP share Button and 1D Axis
bindings and Undo. File-backed Object Lua reads the map captured for that
run. Focus loss releases held keys. A cube moved and turned on named
actions through editor Play, guarded Steps and a relocated Linux build.
File-backed Object Lua offers Kinematic Sphere movement for one mover in a
flat scene with stationary blockers. Native Rot and Plain and approved Claude
verified wall sliding and corner stops. Editor and relocated Linux playback
matched committed poses and contact IDs. Another binding kept Ordinary
movement while using the same file. With 128 stationary blockers, warmed
release Step p95 was 29 µs ordinary, 299 µs in free travel and 273 µs in
sustained wall contact. Paused screenshots kept the committed tick and poses.
Named Box Sensor crossings compare the Kinematic Sphere's previous
committed center and accepted endpoint. Native authoring and approved
Claude verified entry at geometric contact, dwell, exit and re-entry.
A wall kept the sealed sensor outside, and a failed Step preserved the
previous pose and cue history. The relocated player sounded two entries.
Warmed release Step p95 added about 2 µs on fixtures with 1 and 32
stationary colliders. At 128, the measured ranges overlapped at 283–292 µs.
File-backed Object Lua can bind imported WAVs as named Sound Cues in the
Inspector and through approved MCP. Two Script components sharing a Lua
file kept separate WAVs and gain. Editor capture verified silence after a
rejected Step. A relocated Vulkan build played two horn presses through
one audio stream and opened the packaged WAV only during startup.
Positional WAV cues use the active Transform Perspective Game Camera and
the emitter's committed pose. A moving beacon used its new side on the
same tick. Private stereo captures in the editor and relocated Linux
builds showed a 3:1 channel ratio that reversed with the camera. Half
range gave half amplitude, and the range boundary was silent. Legacy 2D
matched the reference. Warmed release Step p95 for 0, 1 and 16 positional
requests was 91–92, 93–94 and 101–102 µs. Matched 2D measured 90–93,
92–93 and 97–99 µs.
Window → Game HUD lets creators add, edit, anchor, hide and remove
static text in v23. Native edits and approved MCP share Undo. The
grouped horn trolley retained keyboard input with its label visible.
A 5,000-object static scene kept its cached presentation. Relocated
Linux builds matched the bottom-right anchor at 16:9 and 4:3.
Object Lua API1/2 can drive a child under transform-only, positive-uniform
groups. A trolley moved along a rotated rail without moving the markers.
World-pose and camera checks run before the tick commits. A rejected Step
keeps the previous pose and camera. The compact MCP catalog shows button
and axis input examples. Grouped runtime draws share the committed world
matrices, lights and camera across unchanged frames. The depth-eight
release test cut paused CPU extraction p95 from 0.762 to 0.012 ms on
256 targets. Vulkan GPU scene p95 stayed at 0.128 ms in the same fixture.
A supported scripted Mesh Renderer leaf can also carry one Point or Spot
Light. The mesh and light have separate Enabled settings and share one
committed pose. Approved MCP reads the committed light transform in world
space. A Blender trolley moved its beam between two plates and aimed
in place in Play and a relocated Linux startup build. A rejected Step
preserved the preceding geometry and beam. For matched moving-lamp
scenes, Step plus viewport CPU extraction was 0.097 ms p95 for the
attached lamp and 1.847 ms for a Group driving a separate lamp.
The editor and viewer draw, fit and pick from cached world matrices.
Play and Linux Build share the same render data for supported static hierarchies,
including shear. Set Game Camera from View captures a fixed v23 perspective
shot for Play and Linux Build, including large static hierarchies. A Transform
Perspective Camera follows its object's position and authored rotation, including
roll and overhead views under scaled parents. Native and approved MCP edits share
Undo. In supported animated or scripted scenes, the editor, MCP and packaged
player use the pose committed with the same scene tick. A failed Lua Step keeps
the preceding camera and scene together. Stop restores editor navigation.
A disabled Mesh Renderer in supported scripted v23 scenes
hides its mesh and shadow while its Lua behavior and collider keep running.
Build retains assets referenced by disabled geometry. A relocated Vulkan
player loaded a 1.44 MB v23 scene with 5,000 objects and an imported mesh. Build failures
offer keyboard-accessible Error Details.
v23 point, spot, and directional lights can be created as objects or attached to meshes.
The Inspector and approved MCP clients edit the same component. Aim at
Object rotates a Spot Light owner toward a target pivot with one Undo.
The selected Scene view guide shows the ray and both half-angles while
the owner is being rotated. Approved MCP reads the authored world origin
and aim. A scaled, parented Blender scene kept the aimed beam through
save, reopen and a relocated Vulkan startup build. The editor warns at
the render budget and keeps extra lights editable. Static Linux builds preserve
attached lights and a parented sun's direction and shadow settings.
Approved MCP clients see v23 scene context, share hierarchy Undo and can save
with inspected guards. Three linked cottages survived reopen and a relocated build.
Find Uses in Scene reports exact v23 texture slots and renderers. It keeps
same-PNG import variants separate and shows unassigned, disabled, linked
and missing uses. Keyboard reveal, approved Claude reassignment, native
Undo and reopen were verified. At 5,000 objects, release query p95 was
0.154 ms cold, 0.036 ms for a warm page and 0.174 ms after reassignment.
Group Selection in v23 inserts a parent around the selected objects with
world transforms preserved. The Object menu, Hierarchy context menu,
remappable Ctrl+G and approved MCP clients share one Undo entry.
Duplicate copies ordinary v23 subtrees in the editor and guarded MCP. The
selected copies retain assets and materials and remap internal references.
Native edits, Undo and Redo, approved Claude and relocated Vulkan verified
two independent market stalls. In a 5,000-object release fixture, a
100-object copy took 5.41 ms p95, and its first UI frame took 1.24 ms p95
after the command. Matched GPU scene p95 was 0.225 ms after copying and
0.223 ms for the prebuilt scene.
v23 Move (W), Rotate (E), and Scale (R) preview geometry and lights during
drags. World and Local axes, pivot modes and snapping are in the viewport
toolbar. Release commits one shared Undo entry. Approved MCP clients read
the committed scene during a drag.
Edit Collider adds local face handles to v23 Box Colliders. One drag moves
a face while keeping the opposite face fixed and commits one shared Undo.
Approved MCP inspects the exact component and its world corners. A flat
sphere sweep verified the edited contact in Play and a relocated Linux
player. A sheared box remains editable through local numeric fields.
On Linux, v23 watches `Assets/` for imports and changes. The Assets panel
shows live folders, previews and diagnostics, accepts drops, and places
meshes and multi-primitive GLB hierarchies with Undo. File actions keep stable IDs
on rename and preserve sources in project trash. GLB imports support
representable node matrices, unnamed or repeated names, and relative PNG
textures. Base-color textures support indexed PNG. Import reports sampler
approximations, other material fallbacks and skipped triangles in the Assets
Inspector and MCP. Static GLBs import point, spot, and directional lights
as editable v23 components on their source nodes. Native Create and Add
Component support Spot Lights with degree half-angles and world-unit range.
The beam follows the object's authored rotation chain, and four active
Point and Spot Lights share the local light budget. Diagnostics report
photometric approximations, defaults and clamps. Explicit Reimport can
adopt a spot from an older accepted catalog using the same source bytes.
Blender beam center, penumbra and cutoff matched editor and relocated
Vulkan in SDR and HDR. Approved Claude edited a spot using its exact
scene revision, and native Undo restored the imported angles.
GLB perspective cameras retain parented rotation, vertical lens, finite or
infinite clipping, and authored framing through editor Play and Linux
Build. Native placement keeps the current Game Camera active. Asset
inspection and MCP explain when a lens is skipped. Packed glTF
metallic–roughness PNGs supply linear Roughness maps from green. Placement
keeps the authored multiplier, and Linux Build embeds the verified map.
Refresh can add the map to an older accepted model while preserving
materials already assigned in the scene. Diagnostics explain omitted maps
when the UV mapping or sampler is unsupported. v23 shared Emission Color
and factor-only GLB emission survived editor authoring, guarded Claude and
shared Undo, and relocated Vulkan at three display settings. The dark
housing and receiver stayed pixel-exact. In a 5,000-mesh offscreen release
test, emission added 60 KB of instance data per frame. Median GPU scene
p95 was 0.301 ms before and 0.309 ms with half the meshes emitting. CPU
preparation p95 varied from 0.358 to 0.719 ms with half the meshes emitting
and 0.331 to 0.543 ms before. Failed imports show
the source path, recovery, last working
version availability and separate Error Details in the Inspector. Approved MCP
clients can page the same issues, including sources with no asset ID. A
relocated build loaded 80 meshes.
The v23 Mesh Renderer can create and assign a textured material or edit
an existing shared definition. The editor shows its affected object count.
Make Unique copies the assigned material for one standalone Mesh Renderer,
including when an object has several renderers. The draft shows the exact
scope and starts with the same texture references. Apply and approved MCP
copy share one Undo step for creation and assignment. The independently
edited finish survives reopen and Linux Build. Native edits and approved MCP
commands preserve scene
material edits when the source GLB is reimported. Assets can import an
additional linear Roughness interpretation of a PNG, and the shared
material dialog can select, replace or clear it. Edit → Scene Display
sets Surface HDR and exposure independently of object selection. Existing
scenes keep their display settings.
The built-in Cube samples Base Color and roughness through its existing
face UVs. Triangular Prism provides a textured closed gable shape with a
local Z ridge, available in native creation and approved MCP. Two Cubes sharing both textures were verified in Play and a
relocated Vulkan build.
v23 prefabs link reusable hierarchies in the editor and MCP. Child transform
and mesh tint overrides support Apply, Revert and shared Undo. The cottage
trial verified a roof fix across three placements and in a relocated build.
Make Unique on a linked Mesh Renderer creates a local material override.
Revert restores the current inherited finish. Apply to Prefab updates the
matching renderer on inheriting copies and preserves independent finishes.
The Inspector and approved MCP report the exact renderer and its override
state. Source conflicts preserve the local work. Three imported displays
retained their finishes and shared textures in a relocated Linux build.
Prefab failures offer **Error Details**, and the Inspector explains how to
restore an unavailable source. In legacy scenes, the Material Inspector shows
unapplied edits, their scope, and **Apply Material** and **Cancel** by the
heading. Approved MCP clients can read the pending scope alongside committed
material values. S-08 is complete. Both editors use shared wording for
commands, controls and feedback. Themed Wording can be turned off in
Preferences → Appearance while keeping the Rot style.
[C-12 benchmark timings](PARITY-PLAN.md#5-performance-budgets)
cover 5,000 and 50,000 objects. Desktop frame rates remain unverified.

## What a creator can do today

In v23, create projects and scenes, assemble component hierarchies and
prefabs, and edit transforms, lighting, cameras, colliders and opaque
materials. Import OBJ, PNG, WAV and supported GLB assets. Attach Lua files,
set per-object properties, and test gameplay in Play. Build packages
supported scenes as a runnable Linux folder or portable archive. Approved
AI apps can work alongside the editor through MCP and share Undo. Legacy
scenes retain a 256-object limit and use the prepared runtime kit for
Build. The [historical inventory](PARITY-PLAN.md#2-where-we-are) records
the starting point for the parity plan.

## Installed on this machine

The **Rot Engine** launcher uses the 2026-11-19 build with GLB material
warnings (`6e32b7f`). Reinstall with `python3 scripts/install_local.py`.
The previous build is in `~/.local/share/rot-engine/previous` for `--rollback`.

## Known problems

- An editor serves one AI adapter at a time; a second waits or fails.
- In legacy scenes, **New Group** and **Create Empty** explain which input
  actions, HUD settings or scripts prevent empty group creation. Approved MCP
  clients see the same reasons in `scene_inspect`. Phase 2 tracks lifting the
  legacy restrictions on creating groups and reparenting.
- Play checks refuse scenes that use groups, so they cannot cover an animated
  model.
- Outside the repository, still using old names: two MCP entries in `~/.codex/config.toml`,
  and the projects in `~/Projects/Ruster-Creations`, which will not load in a
  build from this branch until their files are renamed.

The [backlog](BACKLOG.md) lists completed packages, including Rot styling,
Preferences and Project Settings.

## Not verified

- Gizmo display latency and physical pointer input remain unverified. Isolated
  app-scoped pointer tests cover live v23 Move, Rotate, and Scale.
