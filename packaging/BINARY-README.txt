Ruster Yard — Linux x86_64 development package

Extract the entire archive into a new directory. Run ./ruster-yard there, or
launch the executable by absolute path from any working directory. The owned
truck.obj resource is loaded from resources beside the executable, read-only.
No Ruster checkout, editor, MCP server, Cargo, network or account is required.

Try:
  ./ruster-yard --headless
  ./ruster-yard --headless --capacity 3
  ./ruster-yard --help

The native application starts paused. Run/Pause, Step and Reset are in the
toolbar; changing settings takes effect on Reset. The default capacity misses
the target at the deadline; capacity3 wins. Headless completed challenges exit0,
including a missed target. Invalid arguments/resources exit2 with a hint.

Requirements: compatible Linux x86_64 dynamic loader/glibc and shared libraries
listed in manifest.json. Native mode additionally needs a working Wayland or
X11 desktop, Vulkan loader and Vulkan-capable GPU/driver. System desktop/GPU
libraries and drivers are not bundled. Headless mode still loads ordinary ELF
shared dependencies before main. The manifest records exact ELF dependencies
and GLIBC symbols from this executable; it is not a tested distribution matrix.

This is a local development artifact, not a supported public binary release.
Other Linux distributions, GPUs, Windows and macOS remain unverified.

Original Ruster code and owned example resources: MIT OR Apache-2.0, at your
option. Dependencies retain their own terms. LICENSE-MIT, LICENSE-APACHE,
THIRD_PARTY_NOTICES.txt and SUPPLEMENTAL_NOTICES.txt preserve readable texts;
LICENSE-SOURCES.json records supplemental upstream provenance. The full locked
workspace notice file intentionally includes packages outside this executable.
manifest.json lists this package's active Linux dependency closure and hashes
all payload files except itself. It does not embed local source/cache paths.
