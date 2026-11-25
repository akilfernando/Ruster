# From a scene to an application

Updated2026-11-05. The engineering implementation targets Linux x86_64, built-in appearance, Scene5, Lua and triangle OBJ meshes. Native MCP and direct-editor export journeys pass within the recorded engineering scope; this guide does not claim completed human acceptance. [Current evidence](evidence/CREATOR-export.md) owns verification limits.

V1 foundation update2026-11-05 (historical accepted source252fe48): Save writes Scene6; prepare a current descriptor2 runtime kit for these files, including untextured scenes. Old frozen kits remain valid only for their recorded Scene1–5 scope. Shared CLI/controller packaging now includes active material texture dependencies as exact accepted PNG sources. The complete direct-editor/native-MCP material workflow, real draft recovery, named costs and exact relocated default delivery now have [V1 evidence and evaluation](evidence/V1-readiness.md). Historical creator-eval-1 evidence above remains Scene5-specific.

## Prepare a Linux export template

Do this once for an engine version, with the pinned toolchain and native development prerequisites. Build a release Motion runtime with source/home path remapping as described in [distribution](DISTRIBUTION.md), then:

```sh
python3 scripts/package_creation.py prepare-runtime \
  --binary target/release/rot-motion-lab --output /tmp/rot-runtime.tar.gz
mkdir /tmp/rot-template
tar -xzf /tmp/rot-runtime.tar.gz -C /tmp/rot-template
ROT_RUNTIME_KIT=/tmp/rot-template/rot-runtime target/debug/rot-editor
```

Choose a fresh archive path. Preparation verifies the Linux executable/profile and includes dependency/license notices, requirements, fixed helpers and hashed inventory. The template contains trusted executable code; use a template you prepared or trust. It is specific to its recorded Linux requirements, not a universal Linux binary. Each later scene export uses this template without Cargo or recompiling. Changing the runtime/helpers requires a new template. Editor export requires Python3 and the current Linux process-cleanup utility `/usr/bin/kill`; standalone application execution does not require Python or Rust.

## Start a project in the component editor

In the Project Hub, choose **New Project**, select **Empty**, name the
project, and choose **Create Project**. The scene starts with an editable sun
and an `Assets/` folder. Open the project again from the hub or open its
`scene.json` directly. **Basic 3D** still opens the legacy editor with its
ground, cube, sun, and game camera.

For AI-assisted editing, call `scene_inspect` first. Its first page identifies
the open project and scene, the active object, and up to 16 selected object
IDs. It reports the full selection count even when a selected object is on
a later page. Read subsequent pages using `next_offset`.

To assemble an object from parts, right-click the parent in the Hierarchy
and choose **Create Child**. The new part starts at the parent's origin.
Edit the part's position, rotation, and scale in the Inspector. An approved
MCP client can supply `parent_id` and `transform` to `scene_create` or
`scene_group_create` to create and place a part in one Undo step.

Rot uses a right-handed coordinate system with +Y up. The v23 Inspector
shows degrees. MCP uses Euler radians about fixed X, then Y, then Z axes.
Transforms are relative to the parent. To pitch a roof and turn the house,
tilt the roof child about X and rotate the house parent about Y.

For a pitched roof with closed ends, create a **Triangular Prism** child.
It starts one unit wide, high, and deep, with its ridge along local Z.
Scale X for width including overhang, Y for the rise, and Z for length.
Place the pivot half the rise above the wall top. The pivot is at the
center of the shape's bounds. Press Ctrl+K and search **Triangular Prism**
to create one from the command palette. You can assign a textured
material and add a separate Collider when needed.

For a post, roller, or bollard, choose **Cylinder** in **Object → 3D Object**,
**Create Child**, the command palette, or the Mesh Renderer geometry picker.
The built-in shape is two units high along local Y and one unit in diameter,
with its pivot at the center. At unit scale, a child with local Y = 1 rests
on the parent's local floor. Scale X and Z together to keep the cross-section
round. Assign a textured material as usual and add a separate Collider if
needed.

The side wraps once, with the seam on local −Z at U = 0 and U = 1.
U increases around the side toward −X, +Z, then +X. V runs from bottom to
top. Each cap uses its own planar UVs. Cap U follows +X, with V following
−Z on top and +Z on the bottom.

Double-click the parent's name in the v23 Hierarchy to select it and fit
its visible geometry and child parts, even when its branch is folded.
You can also select the parent and press **F** or choose **Frame Selected**
from its Hierarchy menu. For an
orthographic check, choose **Front**, **Right**, or **Top** from **View
Presets**. An approved MCP client can call `viewport_frame` with the parent
ID and `front`, `side`, or `top`, then call `viewport_screenshot` for each
view. Inspect one assembly from multiple angles before copying it.

## Create a scene in the current project

Choose **File → New Scene**, use the default **Ctrl+N** shortcut, or search
**New Scene** in the command palette. Enter a name and check the
`.scene.json` filename shown in the dialog. Choose **Create** to save and
open an empty scene in the same project.

For unsaved work, choose **Save and Create** or **Discard and Create**.
**Cancel** or Escape returns to the current scene. An existing filename
stays intact and the dialog lets you choose another name.

The new scene uses the existing project Assets and keeps the project's
startup setting. To edit it with an approved MCP client, connect to the
newly opened scene and inspect its session and revision before editing.

### Save a scene variant in the current project

Choose **File → Save As…**, use **Ctrl+Shift+S**, or search **Save Scene As**
in the command palette. Enter a name, check the **Scene File** preview and
press Enter or choose **Save Scene As**. The sibling includes your committed
edits and any valid Inspector draft. It opens with its own scene ID and Undo
history, using the same project Assets. The original stays available through
**Open Scene**, and the existing project startup and Start Transition settings
still point to their designated scenes.

If the filename is occupied, choose another name. If an Inspector change
needs correction, cancel or press Escape to return to the original draft.
An approved client can reconnect to the newly opened scene and inspect its
session and revision before making edits.

### Recover a scene after an unexpected exit

Rot periodically checkpoints committed changes in the open v23 scene,
including changes made by an approved AI client. After a crash, choose
**Recover as New Scene** to open the latest completed checkpoint as a
separate saved scene in its original project. The original scene and
project startup selection stay in place. If a recovered filename is already
taken, Rot tries a numbered sibling.

Checkpoints contain scene changes and asset references and support up to
64 MiB of scene data. Unapplied edits,
Undo history, and Play state are outside the checkpoint. Choose **Keep for
Later** or reopen the list through **File → Recovery Checkpoints…**.
**Discard Recovery Copy** removes the checkpoint.

## Choose a project startup scene

Save a v23 scene directly in the project folder. Open **Edit → Project
Settings… → Project** and choose **Use Current Saved Scene**. The Hub opens
that scene when you open the project folder or its recent project row.
**File → Open** still opens the file you choose, and **Play** runs the scene
currently open.

**Build Application…** identifies the startup file. When one is configured,
Build uses its saved content and assets. You can keep working in another
scene with unsaved changes. Build leaves the open scene and its edits in
place. Choose **Reset to Default** to restore the Hub's folder fallback
to `scene.json` and have Build use the active saved scene again.

If the startup file disappears or a different scene replaces it, open a
saved scene by filename and update or reset the setting. If another editor
changes the setting while a dialog is open, close and reopen the dialog
before retrying.

## Set the Linux player window

In a v23 project, open **Edit → Project Settings… → Player**. Set
**Window Title**, **Initial Width**, **Initial Height**, and **Window Mode**,
then choose **Apply**. Press Enter in a field to apply or Escape to cancel the draft.
The title accepts 1–128 UTF-8 bytes without controls or surrounding spaces.
Set both dimensions to request an initial size of 800–3840 wide by
540–2160 high in logical pixels. The desktop can adjust that request, and
the player window remains resizable.

**Windowed** is the default. **Borderless Fullscreen** starts on the
player's current monitor at the desktop's resolution. Press **F11** in the
Linux player to switch modes. The player asks the desktop to restore the
last windowed size, or the initial size when it launched fullscreen. The
desktop can constrain the result. F11 is reserved for the player window.

Leave the title or both dimensions blank to use the default
**Rot Engine | startup scene ID** or **1100 × 760**. Without a configured
Startup Scene, the default title uses the scene being built. Player settings
are independent of Startup Scene and Start Transition. Build shows the
effective settings and packages them for launch. A Start Transition keeps the
same title and window.

## Open a project scene

Choose **File → Open Scene…**, press **Ctrl+O**, or run **Open Scene** from the
command palette. **Project Scenes** lists saved scenes in the current project
folder and marks the open scene, startup scene and Start destination. Search
by name, use the arrow keys and Enter, or double-click a row. **Browse…**
opens a scene by its explicit path, including one outside the project.
Refresh the list after changing files outside the editor. Unsaved work still
offers **Save and Open**, **Discard and Open**, and **Cancel**.

## Enter another scene from the startup screen

Save the startup scene and its destination in the project folder. Create a
named **Button** Input Action in the startup scene and bind a key such as
Enter. In **Edit → Project Settings… → Project**, find **Start Transition**.
Choose the **Start Button**, use **Choose Scene…** to select the saved
destination, and choose **Set Transition**. **Clear Transition** keeps the startup scene and
removes the route.

In Play, open the startup scene and claim the Game view's controls. Press
the chosen button once to enter the destination with its own camera, input
map and fresh script state. **Stop** returns to the authored startup scene
with its edits and Undo history. Opening the destination directly also
works through the usual Play controls.

Linux Build packages the saved startup scene and the destination with their
respective required assets. The relocated game follows the same button
route. Approved MCP clients can inspect the configured route and use a
guarded input Step with its action ID. Read runtime inspection again after
the transition to obtain the destination's run, tick and input guards.

## Create an empty group in a legacy scene

Choose **New Group** in the Hierarchy or **Create Empty** in the Create menu
or command palette. One **Undo** removes the new group. When scene content
blocks creation, hover over **New Group** or run **Create Empty** to see the
reasons. They can include input actions, a HUD, ordinary script sources, and
scripts attached to objects or the application. Groups with only group-local
rotation scripts can still gain another empty group.

An approved MCP client can check `group_creation_blockers` in `scene_inspect`
before using `scene_group_create`. The list is empty when scene content allows
empty group creation. Drafts, the scene limit, and other edit checks still apply.

## Import assets into a v23 project

Put OBJ meshes, PNG textures, WAV sounds or supported GLB models in the
project's `Assets/` folder. Opening the scene imports supported files in
that folder and its subfolders. On Linux, drop files from your file manager
onto the Assets panel or choose **Assets → Import…**. The editor copies them
into the current Assets folder and adds a number if the name is taken. It also
picks up additions, edits, and moves made on disk while the scene is open.
Move the generated `.rotmeta` file with its source to keep existing references. Put both files in version control so the
asset ID survives a cache rebuild. Existing registered sources receive
sidecars with all their accepted import settings. Move older source files
and their sidecars into `Assets/` so the editor can discover them when
rebuilding the catalog.

Use the **Assets** panel to browse project folders or search by file name,
path, asset ID, or type. Select a file to see its status and import settings
in the Inspector. Double-click a mesh or choose **Place in Scene** to add a
Mesh Renderer at the origin. Drop a mesh on a Hierarchy row to make it a child
of that object. Both placement commands support Undo.

Select a ready WAV to see its duration and sample rate. On Linux, choose
**Preview Sound** to hear it once, or **Stop Preview** to stop early. Selecting
another asset, changing the accepted sound, switching or closing the project,
or starting Play stops audition. Previewing a clip is separate from using it as a Sound Cue.

Choose **Assets → New Folder…** to create a folder. Right-click a file or
open **Asset Actions** in the Inspector to duplicate it, reimport it, or
open its containing folder. Duplicates receive new IDs with the original
import settings. Files under `Assets/` can be renamed with their IDs intact.
For a ready standalone OBJ, Lua or WAV, choose **Move to Folder…** in
**Asset Actions**. Pick a different existing folder and check the exact
destination before choosing **Move**. Rot keeps the filename, source bytes,
import settings and asset ID, then selects the asset at its new location.
Existing scene references still use the same ID. An occupied destination,
including an orphan import sidecar, leaves the source in place.
**Delete… → Move to Trash** moves the source and sidecar together to
`.rot-trash/` in the project. Restore both to their original location to
recover the asset and its existing references.

Choose an exact **Type** beside **Search Assets** to filter the browser.
With an empty search, the filter applies to the current folder. Enter
a query to search the project within that type. The chosen type stays
active when you change folders. Choose **All Types** to clear it.

To assign a mesh to an existing object, select the object and choose its file
in **Mesh Renderer → Geometry**. Click **Apply**, then save the scene.

Double-click an imported GLB or choose **Place in Scene** to create an
editable hierarchy. Placement copies the node names, local transforms, and
materials into the scene. Rot accepts node matrices that can be represented
as translation, rotation, and positive scale. Unnamed and repeated-name nodes
use their source node indices for identity. Keep those indices stable when
reimporting an existing model. A mesh with multiple triangle primitives gets
one child per surface. Each surface keeps its material and inherits
animation from the original node. Placement binds the first supported
transform clip. Place, Undo, and Redo operate on the hierarchy and its
materials together. An approved MCP client can use `scene_model_place` with
the model ID and current asset revision.

For a static GLB with **KHR_lights_punctual**, point, spot, and directional
lights become editable v23 Light components on their source nodes.
Use **Place in Scene** to add a meshless Blender lighting rig to an
existing set. Select the imported spot and use **Aim at Object…** to
retarget it.
They keep their linear colors and follow the imported hierarchy. Rot
divides point and spot intensity in candela by 100 and directional intensity
in lux by 1,000, then clamps the relative result to 0–16. Point and spot
range keeps the source distance within 0.1–10,000 units. An unspecified range
uses 5 units. Spot cone angles convert from radians to degrees. **Import
Warnings** reports the brightness approximation, range substitutions and
clamps. If an older catalog skipped a spot, choose **Asset Actions → Reimport** and
place the model again to use that light. Objects already placed keep their components.
GLBs with both transform clips and lights carry a static playback warning.

A placed GLB in a v23 scene adds a **Transform Perspective** or
**Transform Orthographic** Camera to each source node with a supported lens.
A static camera-only GLB can be imported and placed directly, including a
Blender export with cameras enabled. Placement keeps the rig hierarchy and
leaves the current Game Camera active until you choose an imported lens.
Shared lenses become independently editable Camera components. The
orthographic lens keeps its full vertical frame size, fixed aspect and
clip planes. Choose **Make Active** on a Camera to use it in Game view.
For a model imported by an older Rot version, choose **Asset Actions →
Reimport** and place it again to import a supported orthographic lens.
Existing placed lens settings stay as they are. **Import Warnings** explains
when a lens needs positive magnification, a positive near clip, or values
within Rot's supported ranges. Its node and geometry still place. Import
requires a supported model with geometry.

Keep any PNG files a GLB references beside the model or in their original
subfolder. Rot imports them with the model and refreshes the model when a
referenced PNG in `Assets/` changes. Base-color textures support 8-bit
noninterlaced indexed PNG alongside RGB and RGBA. Rot uses mipmaps and
chooses the filter that matches the most explicitly requested sampling
stages. Select a GLB and look under **Import Warnings** in the Inspector for
sampler approximations, other material fallbacks, or skipped faces.
You can still place the model. `asset_inspect` reports
the light count when present, the warning count, and up to four warnings per model.

Supported glTF metallic-roughness PNGs supply a linear **Roughness Map**
from the green channel. The material keeps its scalar roughness multiplier.
Rot renders the surface as dielectric and omits metallic detail. A source PNG
can also supply a separate sRGB **Base Color** texture. Roughness needs
`TEXCOORD_0`, identity UV mapping, and a sampler compatible with Base Color
when both maps are present. Check **Import Warnings** if the map was omitted.
**Refresh** can add roughness maps to an older accepted import. Place the
refreshed model to use its updated material defaults, or select **Embedded
Roughness** in the material editor for an existing placement.

When a file fails to import, select its marked tile under **Assets**. The
Inspector shows its project-relative path and how to recover. Open **Error
Details** for the importer message. Replace or restore the source at the
same path. The editor watches for changes, and **Asset Actions → Reimport**
lets you retry. If the Inspector says the last working version is available,
you can still use that version while you repair the source. Keep the
`.rotmeta` sidecar in place to preserve the asset's ID and scene references.

An approved MCP client can inspect the current catalog with `asset_inspect`,
then call `asset_import` or `asset_refresh` with its current asset revision.
Use project-relative paths. Sources in `Assets/` already picked up by the
watcher can be assigned with their existing IDs. Import a different
interpretation only when needed, such as flat normals for an OBJ. Assign a
mesh with `scene_mesh` using the current catalog revision. Assignment shares
the editor's Undo history and is saved with the scene.

In v23, `asset_inspect` also lists import issues by source path, including
files without an asset ID. Each issue includes recovery guidance, a separate
diagnostic, and whether the last working content is available for the reported
ID. If multiple import settings share a source, the issue shows an unavailable
interpretation first. Follow `issue_next_offset` using `issue_offset`. These
pages are independent of assets, models and prefabs. Recheck the first issue
page after making a repair because issues can change without a catalog
revision.

## Edit a shared material in v23

Select an object with a **Mesh Renderer**. **New Material** opens a draft
for that renderer. **Apply** creates and assigns the material in one Undo
step. To change an existing definition, choose **Edit Material** and check
the affected object count before applying. The renderer's own **Tint**
still multiplies the shared material color.

Choose **Make Unique** beside the assigned material to make a variation
for that Mesh Renderer. The draft starts with the shared material’s
settings and texture references. **Apply** creates the copy and assigns it
to that renderer in one Undo step. Other renderers, including ones on the
same object, retain their assignments. The default name adds **Copy** so
you can find the new definition in the material picker. On a linked prefab, the
copy becomes a local material override.

Set the material tint, **Base color (sRGB)**, and nonmetallic roughness in
the dialog. To use a loose PNG as a roughness map, put it in the project's
Assets folder, right-click its tile, and choose **Import as Roughness**.
When the same file has a failed Base Color import beside a ready Roughness
map, select the **Base Color** tile and choose **Clear Failed Base Color
Import** in the Inspector. This clears the failed reservation and keeps
the PNG, the roughness map and its asset ID.
Choose it under **Roughness map (linear)** in the material dialog. Choose
**None** to clear the map. The scalar roughness stays available, and
existing Base Color assignments keep their sRGB interpretation. Textures
need meshes with UVs. The built-in Cube samples each face.
**Emission Color** sets how much linear RGB light the surface emits on screen.
Black keeps the material's reflected appearance. Emission contributes only
to the color of that surface. GLB imports use a
constant emissive factor. Materials with an emissive texture stay black in
the emission slot and report an import warning. Reimport an older GLB to
pick up its factor for new placements. Existing placed materials keep their edits.
Press Enter to apply or Escape to cancel. Apply or cancel before saving.
Edits to materials copied from GLB models are saved with the scene.

In **Edit Material**, **Tiling** multiplies the X and Y texture coordinates
and **Offset** shifts them by fractions of the image. These values apply to
Base Color and Roughness together. For example, Tiling 2 on X repeats twice
with **Wrap → Repeat**. Choose **Clamp** to extend the image edge instead.
A negative axis mirrors that axis, and Offset
moves the sampled region. The defaults are Tiling (1, 1) and Offset (0, 0).
Apply commits one shared edit that affects every renderer using the material.

New GLB placements keep compatible TEXCOORD_0 scale and offset. Assets warns
about rotated mappings, unsupported UV sets, and separate transforms Rot
cannot share. When both maps need different tiling or offset, Base Color
takes priority and Roughness uses its scalar. Reimport can update the source
for new placements without changing existing scene materials.

Enable **Double Sided** under **Edit Material** to draw both faces of an
opaque surface such as a flat sign or leaf. Applying a shared material
changes every renderer using it. Use **Make Unique** first to change just
one renderer. Rot imports the setting from opaque GLB materials.
**Reimport** can pick up the setting for new placements. Existing scene
materials keep their values.

Select a texture in **Assets**, then choose **Find Uses in Scene** in the
Inspector. The list shows each material slot and Mesh Renderer that references
that texture ID, including disabled renderers and linked prefab parts.
Unassigned materials appear without a renderer. If one PNG has both Base Color
and Roughness interpretations, select the interpretation you want to check.
Use the arrow keys to select a result and press Enter, or choose
**Reveal in Scene**, to select its object and open the exact renderer.
Escape closes the list. Revealing an object does not change the scene.

You can also select a registered Lua source in **Assets** and choose
**Find Uses in Scene** to see which Scripts use it. The object and component
IDs distinguish attachments on same-named objects. Choose **Reveal in Scene**
to open the exact Script and expand any folded parent branches. The list
still shows attachments when the registered source is missing.

## Set scene display in v23

Open **Edit → Scene Display** with or without an object selected.
**Legacy SDR** is the default. Choose **Surface HDR** to see nonmetallic
roughness respond to lighting, then set **Exposure (EV)** from −8 to +8
stops. HDR starts at zero stops when enabled from Legacy SDR. Apply saves
the change to shared Undo history. Cancel discards the draft. Save the
scene to retain the display settings for the next open and for builds.

Under **Background**, choose **Solid Color** and use the **Color** picker
or enter an sRGB **Hex (sRGB)** value such as `#3366CC`. The color fills
the scene behind its geometry. Surface HDR applies the scene exposure and
tone map to it, and any letterbox bars stay black. **Engine Default**
restores the usual background.

## Set the Game Camera from the Scene view

Frame the shot in the v23 Scene view. In **View Presets**, choose **Set
Game Camera from View**, or find it with Ctrl+K. Select **Set Camera** to
capture the shot in one Undo step.

In **Perspective**, choose an existing Camera or **New Camera**. The active
fixed Camera is the default destination when available. The capture keeps
the rendered target, yaw, pitch, distance, and 45-degree vertical lens.

In **Orthographic**, **Set Camera** creates and activates a new root Camera.
It keeps the view direction, orientation, framing, aspect ratio, and clip
planes. An approved MCP client can capture the same orthographic view with
`scene_player_view_set`. Pass `from_scene_view_revision` from
`viewport_inspect` along with the current scene token and revision.

The Camera Inspector shows its saved world-space shot and active status.
Use **Make Active** to designate another Camera. **Clear Game Camera**
leaves its shot available for reuse. Disabling the Camera suspends its
shot. Static scenes also suspend the shot when its owner or an ancestor
is disabled. **Play** and Linux Build use the active shot. **Stop** returns
to your independent Scene view. Existing Follow cameras keep their target
behavior.

## Use an object's transform as the Game Camera

Select an object, choose **Add Component → Camera**, then choose **Make
Active**. Its **Transform Perspective** lens starts at 45°. Edit **Vertical
FOV** between 10° and 120° in the Camera Inspector. Use the object's Transform
or viewport tools to move, tilt, or roll the shot.

The camera looks down its local −Z axis with local +Y up. It uses its world
position, including the effects of parent scale, and its composed authored
rotation. Parent scale leaves lens orientation and FOV unchanged. The
native lens follows the viewport with clip planes at 0.1 and 1,000 world
units. For an imported camera, **Aspect Ratio**, **Near Clip**, and
**Far Clip** show its authored settings. **Viewport Adaptive** means the
lens follows the output size. An authored aspect fits the shot in the
center with black bars where needed. **Infinity** means the lens has
no far clip plane. Edit **Vertical FOV** in the Camera Inspector.

Play and Linux builds use the active camera. A supported transform clip can
move either the camera or its parent. **Pause** holds the last committed
shot, **Step** advances it with the simulation, and **Stop** returns to
the creator’s Scene view. Disabling the Camera, its object, or an ancestor
suspends the designation until it is enabled again. **Clear Game Camera**
keeps the component and its lens settings.

To frame an isometric or 2D game, change **Projection** to **Orthographic**.
Set **Vertical Size** to the full frame height in world units. Objects of
equal size stay the same size on screen at any distance within **Near Clip**
and **Far Clip**. Enable **Fixed Aspect** to set a width-to-height ratio and
show bars when the Game view or player window has a different ratio. The
transform and active designation stay in place when you change projection.
Choose **Apply** to commit the lens settings together.

## Edit a material in a legacy scene

Select an object and find **Material** in the Inspector. Editing a material
field shows **Unapplied Changes** and how many objects the edit affects.
Choose **Apply Material** to update the shared material in one Undo step.
Choose **Make Unique** to copy the material for the selected object, or
**New Material** to create one for it. Apply creates and assigns the material
in one step. **Cancel** discards the pending edit.

With a material field focused, press Enter to apply or Escape to cancel. If
a dropdown is open, those keys act on the dropdown first. Saving applies a
valid pending edit before writing the scene. Correct or cancel an invalid
edit to save.

An approved MCP client can use `material_inspect` to read committed settings.
The optional `pending_material` summary reports the current edit's intent,
target IDs, and affected object count. Apply or cancel the pending edit
before retrying a material command that writes to the scene.

## Attach a Lua file in v23

Save a UTF-8 `.lua` file under the project's **Assets** folder. For
example, this file rotates its owner at 1.2 radians per second.

```lua
return {
    update = function(object, properties, dt)
        object.rotation[2] = object.rotation[2] + 1.2 * dt
        return object
    end
}
```

Select a mesh object and choose **Add Component → Script**. Find the file
under **Choose Script** and select it. Enter selects the matching item and
Escape closes the picker. The Inspector shows the source filename. One Undo
removes the attachment. The source file remains in Assets.

Use **Play** or **Step** to run the file with Rot's Lua transform API.
An open run uses the source it started with. Stop and start Play after
saving edits to the file. If Play reports a missing or invalid source,
repair that file in Assets and try again. A Lua file can be up to 16 KiB.

Use **Rename…** on the file in Assets to keep its ID and attachments.
A linked prefab can reuse the same asset. Saving the scene records the
asset reference. Linux Build verifies the accepted source and embeds it
in the packaged scene so the standalone can run independently.

## Bind keyboard actions to Lua in v23

Open **Edit → Project Settings… → Input Actions**, or find **Input Actions…**
in the command palette. Choose **Add Button** for an on/off action or **Add Axis**
for a 1D axis with negative and positive keys. Give each action a stable ID
such as `fire` or `move`, capture the keys, then choose **Apply**. Tab moves
between controls and Escape cancels capture or the edit. Each Apply shares
one Undo. You can bind up to four logical keys per action and author up to
32 actions. Escape and Tab are reserved.

In a file attached through the Script component, read the stable IDs using
the fourth `update` argument. Button state includes `held`, `pressed`, and
`released`. An axis returns −1, 0, or +1. Opposite sides held together cancel
out. Return the updated object transform as before.

```lua
return {
    update = function(object, properties, dt, host)
        object.translation[1] = object.translation[1] + 6 * host.axis("move") * dt
        if host.button("fire").pressed then
            object.rotation[2] = object.rotation[2] + 0.5235987756
        end
        return object
    end
}
```

Choose **Control runtime** in the Game view to give it keyboard input.
Stop before editing the bindings. Each Play run uses its starting action
map and accepted Lua source. Save the scene before building. The Linux
player uses the saved bindings and compiled runtime package. This workflow
works in supported flat scenes and under transform-only groups with positive
uniform scale. A script attached to a mesh under a group reads and returns that
object’s local transform. The parent determines how a local movement appears
in the world. If a step would move a child outside the world limits, Play
keeps the previous committed frame and reports the error.

### Create and edit a project script on Linux

Select a folder in **Assets** and choose **Assets → Create → Lua Script…**.
Enter a name and press Enter. Rot creates the `.lua` file in that folder
and selects it when it is ready. The starter script returns the object's
transform unchanged. Attach it with **Add Component → Script**.

Select the script and choose **Asset Actions → Open in Code Editor** to
open the exact project file. In **Preferences → External Editor**, set your
editor executable and include `{file}` or `{file}:{line}` as one argument.
The optional `{workspace}` points to the project directory. Save the file in
your code editor. An active Play keeps its starting code. Stop and Play
again to use the saved changes, or build to package them.

After a project Lua compile or runtime error, expand **Error Details** in
the Game toolbar's **…** menu. **Open Error Line** opens the verified project
file at the location reported by Play. **Open File** opens the source when
the error has no line or your editor arguments omit `{line}`. A renamed
asset can still open if its saved contents match the run. If the source
changed, run Play again for a current error location.

For a missing Lua script or WAV, select its row in **Assets** and choose
**Asset Actions → Restore Missing Source** when available. Rot verifies
the last accepted cached bytes and the original import sidecar before
restoring the same file at its original path. The asset returns to **Ready**
with the same ID, so existing Script and Sound Cue bindings keep working.
If the original cache or sidecar is unavailable, use a backup. If a file
has appeared at the path, inspect it in Assets before continuing.

For a v23 **Button**, select **Primary Mouse** to bind the primary button
alongside any keyboard keys. In Play, click the Game view to give it control,
then press inside the game image. The claim click never fires the action.
Holding the mouse button or any bound key holds the same action. Releasing
the mouse outside the image clears that press. Escape or losing focus releases
Game control. Letterbox bars and interactive HUD controls keep their usual roles.

## Slide a scripted sphere along blockers

On a file-backed **Script**, set **Movement** to **Kinematic Sphere** and
choose **Apply**. Use a flat scene with one enabled mover. Give the mover an
enabled uniformly scaled Blocker Sphere collider and keep other blockers
stationary. In **Input Actions**, add two axes such as `move_x` and `move_z`.

```lua
return {
    update = function(object, properties, dt, host)
        if host.move_and_slide then
            if dt > 0 then
                local dx = host.axis("move_x") * 4 * dt
                local dz = host.axis("move_z") * 4 * dt
                local result = host.move_and_slide({dx, 0, dz})
                object.translation = result.suggested_translation
            end
            return {transform = object}
        end
        return object
    end
}
```

Use the solver translation as returned and call it once per Step. **Ordinary**
keeps the normal transform-return API, and each Script chooses its own mode
even when several components use the same file.

### Detect a named checkpoint

On a Kinematic Sphere **Script**, open **Sensor Crossings** and choose **Add
Sensor**. Enter a name such as `checkpoint`, choose an enabled stationary
object with one Box Collider set to **Sensor**, and choose **Apply**. To change or remove the binding, open **Edit Sensor**.
Bindings belong to the Script component. Two scripts using the same Lua file
can name different sensors.

After `host.move_and_slide`, query the accepted endpoint. A sphere touching
the box counts as inside. The starting pose is used as the first comparison
after Reset. Passing all the way through a box between two outside endpoints
returns `outside`.

```lua
local hit = host.sensor_crossing("checkpoint")
if hit.state == "entered" then
    host.play_sound("chime") -- Bind this Sound Cue on the same Script.
end
```

The other states are `inside`, `exited`, and `outside`. A rejected Step
leaves the previous result in place and does not play its cue.

## Play one-shot sounds from a Script

Import a WAV into **Assets**. On the file-backed **Script** component, choose
**Add Sound Cue**. Name the cue, choose the WAV, set **Gain** from 0 to 1,
and choose **Apply**. Use the `host.play_sound("cue-name")` call shown in the
Inspector from the fourth `update` argument. For example, call the cue when
`host.button("fire").pressed` is true to play it once per press.

In the Sound Cue dialog, **Playback** defaults to **2D**. Choose
**Positional** and set a positive **Range** in scene units to hear it
from the script owner's position relative to the active **Transform** Game
Camera. The cue fades linearly to silence at Range and pans with the
camera's orientation. Each one-shot keeps the position
and camera sampled when its Step commits. The Lua call is the same.

Each Script component owns its cues. Two objects can use the same Lua file
with different WAVs or gains. **Edit Cue** changes or removes the selected cue.
Changes share Undo. Save before Linux Build, which verifies and packages the
referenced WAVs. Play emits sounds when a simulation step succeeds.
Paused Step exposes the committed cue without playing it.

## Add Game HUD text and action buttons

Choose **Window → Game HUD…**. Select **Add Label**, enter the text, and set
the panel's **Anchor**. **Top Center** and **Bottom Center** position the
panel at the corresponding edge of the Game view. **Show in Game** controls
visibility and keeps the authored text and layout. **Apply** commits the
change as one Undo step.
Use **Remove Label** to delete a row, or Escape to cancel the draft.

Labels appear in the Game view and in a saved Linux build. They leave keyboard
and pointer input available to the game. Existing buttons and state-bound
rows remain in their panels.

To trigger a named input from the HUD, choose **Add Action Button**, enter a
**Caption**, and choose a **Button** action in the **Action** picker. Create
or edit the action in **Project Settings → Input Actions**. Apply the HUD
draft and restart Play to use the new button. Activating it sends one tap
on the next simulation tick, including when you click while paused and
then choose **Step**. A keyboard key already holding the action keeps its
hold, and clicking the HUD does not send Primary Mouse to the scene.
**Remove Button** deletes the row.

## Place selected objects on the ground

In the Hierarchy, select the objects to move and choose **Place Selection
on Ground** from the selection's context menu or the command palette. The
lowest indexed mesh point in the selection and its children lands on
world Y=0. The whole selection moves by the same amount, including groups.
A child selected with its parent moves once. Disabled Mesh Renderers count
toward the fit. One Undo restores the original placement.

## Edit a collider in Scene view

Select an object with an enabled Box Collider and choose **Edit Collider**
on its Inspector component. Drag a filled handle along its local axis.
If a handle is hollow, orbit to see the face from an angle. The opposite face stays fixed. Release to
commit the center and size together in one Undo. Escape cancels. For
keyboard entry, use **Center** and **Size** in the same Inspector section.

Use **Fit to Mesh** on a Box Collider to fill **Center** and **Size** from
the indexed geometry of a Mesh Renderer on the same object. When the object
has more than one renderer, choose its exact row. Fitting uses object-local
bounds and keeps the collider's role and enabled state. Thin axes use the
minimum supported size. Undo reverses the fit as one edit.

A box can follow rotated and scaled objects. If a parent transform shears
the box, the viewport cannot draw handles that match the runtime box.
Adjust the parent rotation or scale, or edit the local numeric fields.
For collider participation in Play, the scene must also meet the runtime's
existing behavior requirements.

For a Sphere Collider, choose **Edit Collider** and drag the round handle
at the right of the sphere. The radius changes around its fixed center
in either projection. Release to commit one Undo or press Escape to cancel.
You can also enter **Radius** directly in the Inspector. The sphere guide
requires a positive, uniform world scale and a world pose without shear.

## Reuse a hierarchy as a prefab

1. In a v23 project, create an empty root and use **Create Child** to build
   the assembly in its local coordinates. For a cottage, build one complete
   example and use **Frame Selected** with the front, side and top view
   presets to check the roof and gables before making copies.
2. Right-click the root in **Hierarchy** and choose **Save as Prefab**.
   Rot creates an editable `.prefab` file and sidecar in `Assets/` and links
   your existing assembly as the first instance.
3. Select the prefab in **Assets** and choose **Place in Scene**. Each
   instance keeps its own position, heading and scale at the root. Edit a
   child's local position, rotation or scale, or a Mesh Renderer's tint,
   to override that property for the instance.
4. Use the Inspector's **Apply** action for that override to change the
   source property and update linked instances. Other overrides and each
   root's placement stay intact. Use **Revert** for the property to restore
   its source value on the selected instance. Ordinary scene edits and
   prefab Apply/Revert share Undo and Redo with MCP.

For a linked Mesh Renderer, **Make Unique** creates a **Local Material**
override on that renderer. It keeps the material’s texture references and
settings as the starting point. In the **Prefab** section, check the
renderer number and choose **Apply to Prefab** to promote that finish to
the source renderer. Peers using the inherited finish update. Peers with
their own local finish keep it. **Revert Material** restores the current
inherited finish on the selected renderer. Both actions share Undo with
the editor and MCP.

To keep an instance as ordinary scene objects, select its root and choose
**Unpack Prefab Instance…** in the Inspector. Review the root and object
count, then choose **Unpack**. The assembly keeps its objects, components,
placement, and current materials, including local overrides. It stops
receiving prefab updates. The source and other linked instances stay in
place. Unpack works when the source is missing, and Undo restores the link.

Keep the `.prefab` and its `.rotmeta` together when moving them. If a source
is unavailable, the Inspector reports it and instances keep their last
usable contents for editing, saving and building. Restore the file and
sidecar to reconnect. Select **Error Details** in the status bar if a prefab
edit or source update fails. When Rot incorporates an external source change into
the scene, it clears Undo and Redo to keep the new source and scene together.

Approved MCP clients can use `scene_prefab` to `save` the selected
`object_id`, or `place` a prefab using its `asset_id` from `asset_inspect`
and a new root `object_id`. Placement accepts an optional `parent_id` and
local `transform`. For `apply` and `revert`, supply the target object and
one property (`position`, `rotation`, `scale`, `tint` or `material`).
Tint also needs the Mesh Renderer `component_id`. Material needs the
Mesh Renderer `component_id` and `expected_source_hash` from
`scene_components_inspect`. Use the current scene session token and
revision for each operation.

## Open a v23 scene

Use `rot-scene migrate OLD.json NEW.json` to create a v23 copy, then
open it through **Open Project** in the hub, **File → Open Scene**, or `rot-editor --scene NEW.json`. Select an object in Hierarchy,
edit its name, enabled state, or Transform in Inspector, then choose
**Apply**. Use **Add Component** to attach a **Mesh Renderer** or
**Collider**. Expand its section to edit properties or toggle the component.
Right-click the section header to remove it. Use **Object** or
**Hierarchy → Create** to add an empty object, cube, sphere, capsule, cylinder, or triangular prism.
Drag an object onto another row to parent it, or onto **Scene Root** to
unparent it. You can also change **Parent** in Inspector. Both preserve the
object’s world transform. Drag to the top or bottom edge of a row to place
the object before or after it. **Object → Move Up** and **Move Down** reorder
the selection among its siblings. Ctrl-click toggles objects in the selection
and Shift-click selects a range. Select objects and choose **Object → Group
Selection**, right-click a selected Hierarchy row and choose **Group Selection**,
or press **Ctrl+G**. Remap the shortcut in **Edit → Keyboard Shortcuts…**.
The editor selects the new group and keeps its members in scene-file order
with their world transforms preserved. It places the group under their shared
parent or at **Scene Root** if their parents differ. If both an ancestor and
its child are selected, the child stays attached to the ancestor. Finish pending
Inspector edits and unlock the affected objects before grouping. You can undo or
redo the entire group in one step.

Use the chevron beside a parent in the v23 Hierarchy to fold or open its
branch. With a row focused, Right opens the branch or moves to its first child,
and Left folds the branch or moves to its parent. Up and Down move through
visible rows, and Shift extends the selection across those rows. Folding keeps
your current selection and has no effect on the saved scene. Search Hierarchy
and Find Uses in Scene open the ancestors needed to reveal their result.

In a v23 scene, type in **Search Hierarchy** or press **Ctrl+F** to focus
it. Search matches part of an object name or stable ID, regardless of case.
Results include hidden and disabled objects. Parent breadcrumbs and stable
IDs distinguish repeated names. Use the arrow keys and Enter, or click
anywhere on a result, to select and reveal it in the tree. Escape restores
the tree and keeps the existing selection. **Clear Search** clears the
query and keeps focus in the field. Inspector validation still applies
when you select a result.

Press **Ctrl+D** to duplicate the selection, or choose **Duplicate** in
**Object**, the Hierarchy row menu, or the command palette. Each selected
root is copied beside its original, and the copies become selected. A selected
child is copied with its selected ancestor. Local transforms, materials, and
assets are preserved. Internal object references point to the copies, and
external references retain their original targets. Place another instance
from **Assets** to duplicate a linked prefab. The entire duplicate is one
Undo step.

Use **Move Tool** (W), **Rotate Tool** (E), and **Scale Tool** (R) in the
viewport toolbar. **World** uses scene axes and **Local** uses the active
root's orientation. **Center** uses the selection center, **Active** uses
the active root, and **Individual** transforms each selected root about
its own origin. When you select a parent and child together, the child
follows its parent. **Snap** sets the distance, angle, and scale increments.
Hold **Ctrl** to enable snapping for a drag.

Geometry and lights respond while you drag. Release to commit one Undo
entry or press **Escape** to cancel. Approved AI clients see the committed
scene during a drag and share Undo after release. If a rotation or
nonuniform scale cannot be stored under a scaled parent, use **Local**
and **Individual**. For scale you can also drag the uniform ring.

Drag a selected row to move the selection together, or set its **Parent**
in Inspector. Edit **Enabled**, individual
Transform axes, or properties of shared components, then choose **Apply**.
A dash or **Mixed** marks values that differ. An expression such as `+=1`
adds 1 to each object's own numeric value. **Object → Delete** removes selected objects
and their children together. **Edit → Undo** and **Redo** work until you close
the scene.

Use the eye and padlock beside a Hierarchy row to hide it in the viewport or
block viewport picking. Children inherit both settings. The **Object** menu
applies either setting to the selection. You can still select objects in the
Hierarchy and edit them in the Inspector. These settings are saved on this
machine for the scene. Play and exported games use the scene's authored state.

Choose **Object → Light** and select **Point Light**, **Spot Light**, or
**Directional Light** to create a light with its own Transform. Use **Add
Component** to attach a light to an existing object. Edit **Color** and
**Intensity**, or **Range** for point and spot lights, then choose **Apply**.

Rotate a spot or directional light or its parent to aim it along local −Z.
The cone applies its full strength inside **Inner Half-Angle** and fades to
zero at **Outer Half-Angle**. The fields are in degrees. Set the inner value from
0° up to the outer value and keep the outer value at or below 90°.
Parent scale affects the light's position. Spot angles stay in degrees and range stays in world units. Spot Lights render in the Scene view and in static Play and
Linux builds.

Select an active Spot Light to see its forward ray, dashed inner ring and solid
outer ring in the Scene view. In its Inspector, choose **Aim at Object…**,
search by object name or ID, and choose the target. Use the arrow keys to
choose a result, Enter to aim, or Escape to close the picker. The edit points
the light at the target's current pivot. It rotates the light's owner along
with its attached mesh and children. Undo restores the previous owner
rotation. Approved MCP clients can read the authored world origin and
forward through exact `scene_components_inspect` and request one guarded
`aim_spot_at_object` edit.

To make a moving headlamp, attach one **Point Light** or **Spot Light** to
the object that carries the Mesh Renderer and an Object Lua API 1 or 2
script. Use a leaf object and its supported transform-only parent groups.
The beam follows the same committed world pose as the mesh in Play and
Linux Build. The two component **Enabled** controls work independently.
A scripted scene can contain up to four point or spot lights, including
disabled lights. Approved MCP clients can read the committed world light
transforms by owner ID on the first page of `behavior_runtime_inspect`.

Use **Shadows** and **Quality** to configure a directional light's shadow.
**Advanced Shadows** exposes depth bias and normal offset in shadow texels.
Turning shadows off keeps those settings. Approved AI clients can inspect
and edit a light with `scene_components_inspect` and the guarded
`scene_components_edit`.

The renderer uses the first four enabled point or spot lights and the first
enabled directional light in scene order. The editor keeps extra lights and
shows a warning. Approved clients see the warning in the first page of
`scene_inspect`. Disable an earlier light to preview the next one. The
editor's visibility controls also affect the preview. A static Linux build
uses the authored enabled states.

**File → Save** writes the edits to the v23 file. An approved MCP client
can call `scene_persistence_inspect`, then `scene_save` with the inspected
session, revision and persistence guards. This saves the active scene in
place. Finish any pending Inspector edits before saving through MCP. If
the Save reply is lost, inspect `last_committed_save` before retrying.
When opening another scene, **Save and Open** saves pending changes. **Discard and Open** leaves
the original file as saved, and **Cancel** keeps the current edits. Use
**Play**, **Pause**, **Step**, and **Stop** for supported scenes. During Play
or while paused, Inspector shows the latest committed Transform and script
state under **Play Values**. Select **Scene Values** to edit the authored
scene. **Stop** returns Inspector to the authored values.

On Unix, an approved AI app can add cubes, spheres, capsules, cylinders,
triangular prisms and empty groups to the open v23 scene. It can inspect, rename, reparent, move or
delete objects. Batched moves or deletes take one Undo each. In v23,
`scene_reparent` accepts an `object_id` or an ordered `object_ids` list.
Choose `parent_id` (null for Scene Root), `before_id`, or `after_id`.
Sibling placement uses the requested order. Children stay attached to any
moved ancestor, and world placement is kept when representable.
The AI can inspect components by ID and add, replace, enable, disable or
remove them in one batch. Undo restores the entire batch.
Use **Window → AI Connection…** to pause edits or disconnect. When using a
private project registry, an unenrolled client needs **Review Request…**
and **Allow** in that panel. The project registration is tied to the saved
scene file and scene ID.
Editor and AI Undo and Redo share the same history. Save from the editor.
An approved client can also control Play and inspect its last committed
frame or script state in supported scenes. It uses the editor's Play session.
Ask the AI for a viewport screenshot in Edit mode or after pausing Play.
Close open menus and dialogs first so the scene view is clear.
An approved client can browse the project catalog and assign an imported
mesh to an object with one Mesh Renderer. If the object has several Mesh
Renderers, use `scene_components_edit` with the component ID.

On Linux, save the v23 scene, then use **File → Build…**. Choose a new
folder and select **Build** or **Build and Run**. The folder contains an
`application` executable, the saved v23 scene, and its project assets.
Choose **Open Build Folder** in the build status to open that output in
your file manager. The action is also available when the folder publishes
successfully and the player fails to launch.

To share one file, select **Linux archive (.tar.gz)** under **Output
Format**. Enter a new **Archive File** path and choose **Build**. After
compression, **Show Archive** opens its containing folder. The archive
extracts to one game folder containing the same executable and resources
as a folder build. Extract it before running `application`. You can cancel
during **Compressing Archive…**. A cancellation leaves no new archive.

The Linux player accepts v23 scene files larger than 1 MiB.
Run `application --headless --ticks 16` to check it without opening a window.
In a supported static scene, Play and Build use the same world transforms and
lights. Mesh objects can have children and carry lights. A directional light
can follow its parent's rotation. Rotated objects under nonuniform scale keep
their placement, including shear. The headless report includes `world_matrices`
keyed by object ID, and `behavior_runtime_inspect` includes world matrices on
the paged render objects in a paused static Game. Local Scene visibility only
affects editing. In Play and Build, disabling a parent hides its descendants.
Disabling a Mesh Renderer keeps enabled components on the same object
active. In a supported scripted scene, clear **Enable Mesh Renderer**, select
**Apply**, and start Play. The object keeps its identity, Lua behavior and
enabled collider while the mesh and its shadow disappear. Undo restores the
renderer for the next Play session. Build preserves these authored states and
includes imported meshes and textures referenced by disabled geometry.
Scripted or animated hierarchies and scenes with input actions, HUD controls
or an active Follow camera use the behavior runtime. Its hierarchy still uses
transform-only groups as parents.
The editor finds the `rot-motion-lab` player alongside `rot-editor`. Local
installs include both. When building from source, build both Cargo packages.

If the build fails, the dialog explains what happened and how to recover.
Use **Error Details** for the technical message. Tab reaches it from
**Build Folder**, and Enter opens it. Change the folder in the same dialog
if the destination already exists or its parent is unavailable, then retry
with **Build** or **Build and Run**. A cancelled build leaves the destination
untouched. If the build finishes and the player fails to start, the dialog
reports both results. You can still run `application` from the published
folder.

An approved MCP client can read the current or last v23 Linux build with
`creation_export_inspect` and empty arguments. Clients that use the tool
catalog can call it through `rot_tools`. The `build` object reports
`state`, `published`, and a `failure` with a code, message and recovery
when applicable. **Error Details** are available in the editor. Start,
retry or cancel a v23 build from the editor.

## Build a legacy scene in the editor

1. Start with the empty welcome scene or **File → New Scene**. Create an object, name it, save into your own project directory and import a triangle OBJ through Assets. Assign it using **Edit → Assign mesh…** while the object is selected (or the Assets item’s **Assign to Active Object** context menu). Drag-and-drop is also available; physical-focus validation remains separately recorded.
2. Add a Lua script, edit its code and properties, and press **Play** (Ctrl+P) or **Step** to check it. Correct any reported script errors. Stop preview before editing authored transforms; running transforms are separate from saved values.
3. Save the scene. **File → Build…** or **Ctrl+Shift+E** opens the build dialog. Enter a title, the prepared template directory and a fresh `.tar.gz` destination. An authored Scene21 **Player View** owns standalone framing and needs a template advertising `authored_player_view`. For a scene without a player view, choose **Use Current View** to freeze the Scene View camera as the standalone's initial framing, or leave it unselected to start with Fit. Legacy initial framing requires a template advertising camera support. Relative archive paths resolve beside the saved scene. Shortcuts are remappable in editor settings.
4. Enter submits text fields; Tab traverses controls, Enter on Cancel cancels, and Escape closes the dialog. The build runs in the background. The status window offers **Cancel Build** and shows **Error Details** if the build fails. Save the scene and check for missing or broken meshes before building. Resolve drafts/dialogs first; changed OBJ sources require explicit Refresh. Custom shaders require an explicit choice of built-in appearance for this session; their source file is preserved.
5. Extract the archive elsewhere and run `application/application`. It discovers its adjacent `app.json`, starts the saved creation and needs neither editor nor MCP. For a deterministic check, run `application/application --headless --ticks 10` from another working directory. At the default100ms step, ten ticks simulate one second.

Export cleanup update2026-11-09. A committed archive is ready while the worker removes its private staging files. Status shows cleanup progress and any cleanup warning. Cancellation applies before publication. A later cleanup problem preserves the committed receipt and archive. The next export and normal editor close wait for the existing worker to finish. Native operation inspection adds `cleanup_pending` and `cleanup_warning`; `busy` includes that cleanup interval. This candidate lifecycle has headless verification, with native acceptance pending in [current status](archive/STATUS-2026-11-17.md#reliability-milestones--in-progress).

The artifact contains exact saved scene bytes/history plus only the current runtime mesh and assigned-material texture closure. It is a runnable application, not a relocated editable project: assets referenced only by historical undo entries or unused materials are not copied. Export does not include unrelated project files.

## Iterate on a static Blender model

Save the scene in your project, then put a supported self-contained GLB
source under that directory. Use **Content → Import / source tools…** to
register it with a stable asset ID. **Place** on the model row inserts an
editable copy of its hierarchy and materials under a new creator-owned
Group. Move that Group or attach a supported behavior and Player View target
using the normal editor controls.

When the source geometry or embedded base-color image changes, overwrite
the editable GLB and use the model row's **Refresh**. Keep the source node
paths, transforms and material settings equivalent. An edit during **Play** restarts
it with the new data. A malformed or structurally incompatible source keeps the
last accepted version available. Repair the source and Refresh, or import a
structurally different model with a fresh identity. When copying an editable
project, include its asset catalog and `.rot-models` directory.

## Animate an imported model

Animate objects in Blender with ordinary location, rotation or scale
keys, name the action, and export a self-contained `.glb` with animation
enabled. Import it as above. The model row reads **Animated model** with
each clip's name and length. **Place** attaches the first clip to the new
placement Group and plays it in a loop.

**Step** from stopped shows the clip's first pose. **Play**, **Pause** and **Step**
advance it with every other runtime motion, and **Stop** returns to the
authored rest pose. Select the placement Group and open **Animator** in
the v23 Inspector. **Clip** lists the model’s clips and their durations.
Choose a clip and use **Apply** to commit it. Clear **Loop** to hold
the last pose. Tab reaches the controls, Enter selects a focused clip
or submits **Apply**, and Escape cancels the draft. Use **Remove
Component** to detach the Animator. Each Apply or removal is one Undo
step. In legacy scenes the **Animation** section offers the same clip
and loop controls. A target the clip animates cannot also carry a
transform-writing behavior, and deleting it requires removing the
animation first.

To open a door or lower a bridge on input, create a **Button** in
**Project Settings → Input Actions**. Select **On Button Press** under
**Animator → Start Playback**, choose the Button, and **Apply**. Play starts
at the clip's first pose and waits there. The press shows that pose for the
current Step. The next Step advances the clip. Holding the Button continues
playback, and pressing it again starts the clip over. **Loop** still controls
whether playback repeats or holds the final pose. Choose **On Play** to
start automatically. Repair a missing Button by selecting another Button
or switching to **On Play**.

Set **Playback Speed** on each v23 Animator to play the shared clip at
different rates. Use `0.5` for half speed, `2` for double, and `1` for
the original rate. Choose **Apply** to save the edit to Undo history.
The rate applies in Play, saved Play Checks, and Linux builds.
Preview Time still reads seconds from the source clip.

Choose **Preview** in the v23 Animator to scrub its transform clip in the
Scene view. Use **Preview Time** or enter an exact **Time (seconds)**. The
end of the slider shows the final pose even when **Loop** is on.
**Exit Preview** or Escape restores the authored pose. Selecting another
object, changing the scene, starting Play, or accepting refreshed assets
clears preview. The scene camera, saved scene, Undo history and other
placements are retained.

After changing keys in Blender, overwrite the source and use **Refresh**.
The placement keeps its binding. Renaming a clip, or animating a
different set of nodes or properties, needs a fresh import and Place.
Exporting an animated scene needs a runtime template that advertises
`imported_transform_animation`. Skinned characters, shape keys, clip
blending and script-controlled playback are not supported yet.

Save, then use the normal export dialog. The relocated app runs with
generated mesh and texture resources from the accepted GLB generation. The
original modelling file and Blender are unnecessary for app playback.
See the [asset guide](../crates/rot-assets/README.md#static-glb-source-generations)
for the bounded format and the [MCP guide](../crates/rot-mcp/README.md)
for the guarded equivalent.

## Save and rerun a play check

Open **Window → Play Checks…** in a saved scene. In v23, checks support
hierarchies that work in ordinary Play, including imported animation.
Scripted Play supports up to 256 geometry and light targets. Static checks
support larger scenes and advance one tick per frame. Checks run the fixed
input frames against the saved scene, including verified Lua files from
Assets. Your live Play session and scene Undo stay available.
In **Results**, choose **Run Saved Check** to run isolated playback. Failed
assertions show their expected and actual values first. Expand an assertion
to see its details. **Cancel Operation** appears while a check is running.

Switch to **Edit Check** and choose **New Check** or **Reopen Check**. Enter
named actions under **Input Frames**, then add **Assertions** for the final
tick, **Object Local Translation**, or typed state values. A translation
assertion on a modeled child reads its position relative to its parent.
**Capture Current Identity** pins the saved scene and its resources.
**Save Check** writes the sibling `<scene-stem>.play-check.json`. Switching
views or closing the dialog keeps the draft.

When the scene or resources change, save the scene and capture its identity
again. Review the existing assertions before saving the check.

For legacy scenes, export using a matched kit advertising `repeatable_play_checks`. After
relocation run `application/application --play-check`. The app prints a
structured report and exits 0 on pass or 2 on a failed check. You can still
launch the app normally. The [MCP guide](../crates/rot-mcp/README.md)
covers guarded start, inspect and cancel.

## Export a legacy scene through MCP

Launch the editor with explicit existing directory grants:

```sh
ROT_MCP_ENABLE=1 \
ROT_MCP_SAVE_DIR=/absolute/project \
ROT_MCP_EXPORT_DIR=/absolute/exports \
ROT_RUNTIME_KIT=/absolute/template/rot-runtime \
  target/debug/rot-editor --scene /absolute/project/scene.json
```

Attach the adapter using the private socket printed by the editor; see the [MCP guide](../crates/rot-mcp/README.md). Scene/content/behavior tools perform the same validated authoring operations as the editor. Save via `scene_persistence_inspect` then `scene_save`, using the returned tokens. Save may preserve unfinished Lua; export's actual runtime preflight must succeed.

Call `creation_export_inspect`, then `creation_export_start` with title, a fresh `.tar.gz` leaf filename and its exact returned session, scene revision, persistence, asset, shader and export-configuration preconditions. Revisions/operation IDs are decimal strings; a missing asset catalog is JSON null. No executable, shell arguments or arbitrary output path is accepted. `shader_refresh` with `builtin:true` explicitly selects the same built-in appearance as the editor.

Optionally pass `initial_camera` using the exact validated camera returned by `viewport_inspect`. This is explicit export intent, frozen independently of later navigation. Omit it or pass null for default framing. The old immutable template does not gain this capability automatically; prepare a new template from the current runtime/helpers. Runtime navigation remains available after initialization.

Start returns an operation ID, not a completed archive. Poll `creation_export_inspect` with that ID; a committed terminal receipt records archive/manifest/scene/template/runtime hashes and byte count. Cancel using `creation_export_cancel` with the current session token and operation ID. Inspect after a timeout/disconnect instead of restarting: the last eight terminal receipts survive scene replacement. A root-changing SaveAs also advances the session token; use the resulting reply/inspection tokens. Human edits/drafts, save association changes, catalog/shader changes, replacement or cancellation prevent a stale result from publishing. Existing destinations are never overwritten. Directory grants survive scene replacement without expanding authority; restart to change them.

Runtime execution is read-only. Editor export uses private staging and a bounded owned helper process; it is not a sandbox for arbitrary trusted Lua/OS code. External human usability, broader hardware and M9 alpha acceptance remain separate gates.


Historical V2a implementation note2026-11-05 (V2a was subsequently accepted; current source is superseded below): source writes Scene7 and stages content schema3. These files require a new exact runtime profile with `initial_camera`, `opaque_materials` and `authored_lighting`; old frozen kits reject them, including unlit/Legacy Scene7 files. Descriptor2 shape is retained. The new kit continues to support older content schemas. This is an in-progress migration, not V2a workflow acceptance; use the preserved V1 review copy/artifacts for the already accepted workflow. [Active plan](plans/V2A-AUTHORED-LIGHTING.md), [evidence](evidence/V2A-foundation.md).


V2b integration2026-11-06: source now saves Scene8/stages content schema4 and requires exact ordered `initial_camera`, `opaque_materials`, `authored_lighting`, `sun_shadows` runtime capabilities. Descriptor2 remains. Every Scene8 rejects old kits before staging/publication even with shadows disabled; older supported content remains compatible with the new kit. This source progression is not complete V2b creator acceptance. Preserve the accepted V1/V2a review artifacts and follow the [bounded V2b plan](plans/V2B-SUN-SHADOWS.md)/[current evidence](evidence/V2B-feasibility.md).

## Keyboard-controlled kinematic mover

In a flat scene, add an enabled blocker sphere to the moving object at a
uniform scale. Give the obstacles enabled sphere or box blocker colliders.
The obstacles may be rotated and must stay stationary during the run.
Create Axis actions with the keys you want for X and Z. Select the mover,
then choose **Scripts** → **Add Script…** → **Lua Kinematic Mover**.
Set `axis_x` and `axis_z` to the saved Axis IDs, and `speed` to scene units
per second. A blank axis is neutral. Press **Play** and click the game view to
give it the keyboard, or pause and Step. **Spatial feedback**, in the **…**
menu beside Stop, shows the committed outcome and contact targets, normals
and viewport markers.

Save the authored scene before creating a repeatable play check. Expect the
mover's terminal translation and any declared contact-count state you use in
the behavior. For Linux export use a current runtime kit advertising
`kinematic_move_and_slide`, then run the relocated application's
`--play-check`. Older kits can still package compatible scenes without API7.
See the [Lua contract](../crates/rot-behavior/README.md#opt-in-api7-kinematic-move-and-slide).

## One-shot sound feedback

Copy a short PCM16 WAV into the project. In **Content**, import the file
with a stable asset ID. On a Lua behavior, use **Add property** and choose
**Sound**. Give it a name, choose the WAV from the Sound selector and set
gain from 0 to 1. Enter commits the focused field and Escape cancels its
draft. Save the scene and press **Play**.

Call `host.play_sound("confirm")` in the enabled behavior's update when
the action or owned state crosses the desired edge. `confirm` is the
Sound property name. Use named input and earlier committed state to
distinguish a new press from a held key. **Runtime → Audio** shows the
output status and latest committed cue count. The audio output status
and the simulation receipt describe different stages of the request.

A Linux export with sound uses a kit advertising `one_shot_audio`.
A saved play check can assert the triggering state or count and runs
silently. For device playback, run the exported application and use its
**Control runtime** button to claim keyboard input.

### Tune a project Lua script per object

In the file-backed **Script** Inspector, open **Number Properties** and choose
**Add Number Property**. Enter a unique name such as `speed` and a finite
number, then **Apply**. The Script can read it as `properties["speed"]`.
Attach the same Lua file to another object and set its own value there.
**Edit Number Property** lets you change the name or value, or **Remove** it.
The change is one Undo step and the next Play uses the latest values. Add a
fallback in Lua if you plan to run the script without the property.

For a per-object switch, choose **Add Boolean Property** under **Boolean
Properties**. Name it `auto_start`, set the **Value** checkbox, and choose
**Apply**. Lua reads a checked box as `true`, an unchecked box as `false`,
and a removed property as `nil`. Use an explicit nil check if your default
should be true.

```lua
local auto_start = properties["auto_start"]
if auto_start == nil then auto_start = true end
```

### Copy a local Transform

Select one object and open **Local Transform** in its Inspector. Choose
**Copy Local Transform**. Select one or more destination objects and choose
**Paste Position**, **Paste Rotation**, **Paste Scale**, or **Paste All**.

The values are relative to each object's own parent. Two objects with the
same local values can have different world poses when their parents differ.
The copied values stay available in the open scene as you change selection
or edit the source. A paste that changes values is one Undo step.

Choose **Reset Local Transform** from the same menu to set Position and
Rotation to zero and Scale to one for the current selection. Each object
resets relative to its own parent. A reset that changes values is one Undo
step across the selection, and your copied values stay available for Paste.
