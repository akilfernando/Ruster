# Ruster's bounded wgpu-hal workaround

Updated 2026-11-04. This is the crates.io `wgpu-hal` 30.0.1 package, vendored with its upstream MIT and Apache-2.0 license files. Original source is unchanged except for the two edits listed below. Registry bookkeeping (`.cargo-*`) and the package-local lockfile are omitted; the Ruster workspace lockfile owns resolution. `Cargo.toml` is the normalized published manifest, while `Cargo.toml.orig` preserves the original package manifest.

- Origin: [crates.io wgpu-hal30.0.1](https://crates.io/crates/wgpu-hal/30.0.1), [upstream revision40f4a34](https://github.com/gfx-rs/wgpu/tree/40f4a34ebaf56f9a046231f54125ad046239d3f3/wgpu-hal).
- Published `.crate` SHA-256: `b6b7fb58561a792bc237628ba0792e332de418fefe145f13b5ed8201e6d52f58`.
- Upstream known defect: [wgpu issue7696](https://github.com/gfx-rs/wgpu/issues/7696), open when checked 2026-11-04. The October18 report specifically covers Naga30.0.1 with validation SDK1.4.363.0.

## Why this exists

Naga30.0.1 decorates struct members with SPIR-V `Offset`, including structs used by `Function` storage variables. Its Vulkan backend normally selects SPIR-V1.5/1.6 on modern devices. The Vulkan [shader-interface rules](https://docs.vulkan.org/spec/latest/chapters/interfaces.html#interfaces-resources-layout) permit such layout decorations in `Function` and `Private` storage only through SPIR-V1.4. This produces actual invalid shader modules under `VUID-StandaloneSpirv-None-10684`, even when the driver renders them successfully. The upstream wgpu debug callback suppresses that diagnostic; clean debug logs therefore did not establish correctness.

## Exact changes and limits

1. In [adapter.rs](src/vulkan/adapter.rs), set the SPIR-V generator's `lang_version` to1.3 on Vulkan1.1+ devices, preserving1.0 for Vulkan1.0. Naga generates for that target from the outset; no emitted binary header is rewritten. The actual Vulkan device/API version remains unchanged.
2. In [instance.rs](src/vulkan/instance.rs), remove only the special suppression of `VUID-StandaloneSpirv-None-10684`. Existing unrelated upstream callback behavior remains unchanged.

This is scoped to Ruster's current basic opaque graphics, texture sampling and optional timestamp queries. It is not a promise that arbitrary wgpu features work with a SPIR-V1.3 target. Before adding features that require newer SPIR-V or different layout rules, revisit this workaround, test the feature requirements, and prefer a verified upstream correction. A full Naga fix would need layout-free type variants and correct value conversions; it is deliberately not approximated here. Remove this vendor patch when an upstream version generates valid modern SPIR-V for these shaders and equivalent validation evidence passes.

## Evidence and maintenance

The isolated release probe on2026-11-04 used the real renderer's explicit Vulkan shader-recovery test with the extracted Khronos1.4.363 layer, synchronization validation and no wgpu debug callback. The upstream package emitted three10684 errors in this headless probe even though the Rust test returned success; the standalone runtime also creates egui's pipeline and emitted four. With these two edits, the same test accepted the valid replacement, rejected malformed and incompatible-binding WGSL, submitted the retained pipeline, and returned success with zero raw validation errors (test0.55s; cold changed-dependency build19.28s).

An independent Naga30.0.1 translation experiment generated SPIR-V1.3 and1.6 for all four shader sources: built-in scene, composite, project shader and egui's shader. Running `spirv-val --relax-block-layout --target-env vulkan1.3` failed all four1.6 modules with10684 and accepted all four1.3 modules. This checks emitted-module validity separately from driver acceptance. It is not a performance comparison or broad hardware/feature certification.

For the repository-integrated gate, run the renderer's explicit device test in release with the available Khronos layer and inspect the complete raw log for validation errors, including mixed-case `Validation Error` and VUID lines. Also exercise native editor/runtime creation, resize and shutdown under validation. The workspace source gate cannot substitute for those GPU checks. Current integrated results belong in Ruster's status/evidence documents; the isolated probe alone does not certify a subsequent integration.

To review an update, compare the complete vendored package with the checked upstream crate and confirm only these two source hunks plus this provenance document differ. Preserve both licenses and regenerate Ruster's dependency notices after dependency changes.

Documentation-only adaptation: three upstream README reference links to generated rustdoc HTML are expanded to their versioned docs.rs URLs so the vendored source is browsable outside the upstream documentation build. No additional executable source change.
