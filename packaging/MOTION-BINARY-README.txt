Ruster Motion Lab — Linux x86_64 development package

Extract the complete archive into a fresh directory. The application consumes
Scene5 behavior documents through the same kit as Yard. Its comparison demo has
motion, properties and references, with no player, score or win-state concepts.

Try:
  ./ruster-motion-lab --generic-demo --headless --ticks 10
  ./ruster-motion-lab --generic-demo
  ./ruster-motion-lab --scene resources/generic.scene.json --headless --ticks 10
  ./ruster-motion-lab --scene resources/generic.scene.json --headless --speed 2
  ./ruster-motion-lab --help

Author sources/properties in Ruster Editor, save the scene, then run the app on
that file. Native generic mode starts paused; Reset validates the snapshot, then
Run/Pause or Step advances it. Runtime transforms and speed overrides are never
saved into the source document. Unsupported/unfinished code stays editable in
the authoring tools but cannot run until repaired. Source documents are read
only. No Ruster checkout, editor, MCP server, Cargo or account is needed to run.

The original no-argument and --script/--project modes remain the legacy Rhai
compatibility experiment. They use a separate project1 format and do not convert
Rhai into Lua. The generic mode adds no competing source/history editor.

Requirements: compatible Linux x86_64 loader/glibc and shared libraries listed
in manifest.json. Native mode needs Wayland or X11, a Vulkan loader and a capable
GPU/driver. Desktop/GPU libraries and drivers are not bundled. The interpreter
is included in the application; a system Lua install is not required. This is a
local development artifact, not a supported public release or platform matrix.

Original Ruster code/resources: MIT OR Apache-2.0, at your option. Dependencies
retain their terms. LICENSE-MIT, LICENSE-APACHE, THIRD_PARTY_NOTICES.txt and
SUPPLEMENTAL_NOTICES.txt retain readable texts; LICENSE-SOURCES.json records
supplemental provenance. manifest.json identifies the active dependency closure,
ELF requirements and hashes of payload files except itself. It contains no local
source/cache paths. Other machines, GPUs and platforms remain unverified.
