# Rot Yard

Copy this folder to a writable location if you want to experiment. In the
Rot Engine Project Hub, choose **Open Project** and select the folder. You can
also open `title.scene.json` directly. Rot imports the files under `Assets`
when the project opens.

- **Title and Lift Dock.** In Play, click **Start** to leave the title.
  Click the Game view and hold **Space** to slide the gate.
  Open `lift.scene.json` directly and choose **Window → Play Checks…** to
  run **Sliding Gate Travel**, which checks the gate's local position after
  120 fixed ticks. While playing the lift scene, save an edit to
  `Assets/Sliding Gate Motion.lua` to try live Lua reload.
  Move **Gate Beacon** along local X before starting Play to change how far
  the gate travels. Press **Space** to hear the gate latch and create a
  warning marker from the disabled template. Release it to remove the marker.
  The saved check replays two press-and-release cycles.
- **Riverside.** Open `riverside.scene.json` with **File → Open Scene…**.
  The scene has 5,000 objects, including 714 linked instances of
  `Assets/Riverside.prefab`. The shared subdivided roof mesh and the cottage
  details make up about one million triangles. Play uses the overview camera.
- **Gallery (Phase 3B Reference Interior).** Open `gallery.scene.json` to
  inspect a multi-light interior using Surface HDR, 4-cascade CSM sun shadows
  through the clerestory, spot/point shadow atlas casters, 15 clustered local
  lights, exponential-squared height fog, SSAO, bloom, ACES Fitted tonemapping,
  and `4× MSAA`. Switch viewport shading modes (`Lit`, `Wireframe`, `Normals`,
  `Roughness`, `Metallic`, `Albedo`, `AO`, `Depth`, `Clusters`, `Cascades`)
  from the viewport toolbar or `Alt+2..5`.
- **Standalone.** From the title scene, choose **File → Build…** and select
  a Linux output folder. Run `application` from that folder and click
  **Start**. Click the Game view, then hold **Space** to slide the
  gate. The build includes the title and lift scenes. See the [creator workflow](../../docs/CREATOR-WORKFLOW.md#set-up-the-editor-and-linux-player)
  for setup details.

`Assets/Kenney` contains 15 CC0 models and their color texture from Kenney's
[Starter Kit 3D Platformer](https://github.com/KenneyNL/Starter-Kit-3D-Platformer/tree/3fa8a04b1c01ab23db43123d4ce814a34c3fc7f0).
The gate mesh was exported from Blender. The latch sound was synthesized for
this scene. See [Kenney's license notice](https://raw.githubusercontent.com/KenneyNL/Starter-Kit-3D-Platformer/3fa8a04b1c01ab23db43123d4ce814a34c3fc7f0/README.md).
