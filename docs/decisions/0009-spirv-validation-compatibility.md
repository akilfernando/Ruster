# 0009 SPIR-V compatibility and visible validation

Date: 2026-11-04. Status: implemented bounded dependency workaround under M6 development authorization. Supersedes the original M5 release-validation acceptance claim; preserves its other scoped evidence. Owner: Ruster maintainers. Revisit before any wgpu/Naga upgrade or advanced GPU feature requiring newer SPIR-V.

## Evidence and correction

Raw M6 release Vulkan logs reported VUID-StandaloneSpirv-None-10684. The same defect was present in retained M5 release logs; an uppercase-only ERROR check missed mixed-case Validation Error lines. That earlier acceptance statement was incorrect. Upstream wgpu-hal30.0.1 also explicitly suppresses this message in its debug callback, so clean debug logs could not disprove it.

The actual dependency issue: Naga30.0.1 emits member Offset layout decorations on types used in Function storage, while HAL chooses SPIR-V1.6 on Vulkan1.3 hardware. The relevant Vulkan rule permits these decorations only through SPIR-V1.4. Upstream issue7696 documents the same version/validation SDK reproduction and was still open when checked. This is not dismissed as driver noise or hidden by changing logging flags.

## Decision and alternatives

Vendor the published wgpu-hal30.0.1 source with its MIT/Apache licenses, exact origin commit/crate checksum and a narrow patch: generate actual SPIR-V1.3 (1.0 for older Vulkan), and remove only the10684 callback filter. No SPIR-V header relabeling or validation disable. The current fixed scene/composite/egui pipeline uses basic features compatible with this output; higher-SPIR-V-dependent facilities are outside this verified scope. This is a local compatibility workaround, not a general upstream Naga layout fix or performance improvement.

A full Naga layout-free-type/OpCopyLogical repair is larger compiler work and inappropriate to guess inside M6. Rewriting only our WGSL constructors leaves egui and arbitrary supported project shader paths affected. Public wgpu configuration exposes no suitable output-language cap. Suppressing the message or merely enabling debug would conceal a real defect and was rejected. The vendored patch is an explicit maintenance cost, not a permanent architecture preference.

The source is at [third_party/wgpu-hal](../../third_party/wgpu-hal/RUSTER-PATCH.md), excluded from workspace-owned formatting/lint scope but built normally as a dependency. Its original source/licenses are retained; three generated rustdoc README links are normalized to versioned docs.rs links. Dependency inventory now includes non-workspace path dependencies, so vendoring cannot silently drop original notices. No external global toolchain/library installation or replacement was made.

## Verification

Controlled release GPU probe on the same adapter/layer: original output produced3 raw validation-error blocks; capped output produced0. Independent Naga emission plus spirv-val on scene, composite, project and egui sources: all four1.6 outputs fail with10684; all four1.3 outputs pass. The integrated final release imported-scene run rendered92frames and exited normally with the explicit project shader, no defects found by the new case-insensitive log scanner. Native candidate recovery and full gate results are recorded in [M6 evidence](../evidence/M6.md).

The scanner regression specifically covers mixed-case errors and VUIDs. It is not proof of capture coverage or of errors a callback still hides; raw stdout/stderr and profile/layer context remain required. Other upstream filters are unchanged, making raw release validation an important complementary check. Keep the known failing baseline and successful corrected output in the evidence rather than rewriting historical results.

## Primary sources

Checked2026-11-04: [wgpu issue7696](https://github.com/gfx-rs/wgpu/issues/7696), [Vulkan shader interfaces](https://docs.vulkan.org/spec/latest/chapters/interfaces.html), installed/published Naga30.0.1 writer and wgpu-hal30.0.1 adapter/instance source (origin/checksum in vendor record). The baseline/fixed validation results establish this workload's compatibility; no broader GPU matrix or shader-language support guarantee follows.
