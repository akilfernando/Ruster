# 0017 — Bounded hardware viewport antialiasing

Date:2026-11-04. Status: accepted initial4× default on supported devices, agent-selected after local quality/cost evidence. Other hardware remains unverified.

## Context

The first creator walkthrough found visibly aliased cubes. The shared renderer used single-sample offscreen color/depth; linear compositing did not provide geometric antialiasing. The user also requires lean optimized implementation.

## Decision

Use standard matching-sample color/depth plus automatic resolve into the existing single-sample composite target. Prefer requested4×, with capability-accurate fallback2×/1× and optional native format features only when supported. Keep shader refresh sample compatibility and recreate attachments on resize. Developer RUSTER_MSAA_SAMPLES=1|2|4 provides controlled comparison without new dependencies or shader ABI.

Bound retained attachment payload to256MiB through aspect-preserving dimensions; use logical camera/picking aspect. Estimate8/20/36bytes per pixel for1/2/4×, distinguishing this from driver allocation and resize peaks.4× is an initial quality default, not a user-selected setting or a guarantee on every platform.

## Alternatives and consequences

FXAA could reuse compositing but changes image sharpness and needs a separate tested filter; temporal AA requires motion/history and adds artifacts/state unjustified by this renderer. MSAA addresses geometric silhouettes without a render-graph/material rewrite. It costs extra attachment memory/bandwidth and does not fix shader/texture aliasing. High-resolution cap-triggering quality/resize and moving-camera stability need additional evidence.

## Verification

[M8-AA evidence](../evidence/M8-AA.md) owns raw logs, excluded captures, unaltered images and analysis.18matched release runs at1084×678,3/256objects and1/2/4× passed explicit Vulkan/synchronization scanning.4× added20,578,656estimated attachment bytes and about43microseconds median scene-pass GPU work locally; no full-frame/FPS or competitive claim. Actual editor1/4× images show smoother silhouettes. An existing GPU probe covers supported counts, accepted/rejected shaders, resize/pending work and shutdown with an explicit logger sink. Initial missing sink, concurrent incomplete dependency build, harness assumptions and visible-workspace sample were retained/excluded with reasons, not accepted as evidence. The43.60s integrated gate and later normal-close native workspace journey passed; other hardware/moving-camera human acceptance remain open.

Primary references accessed2026-11-04: [Khronos MSAA](https://docs.vulkan.org/tutorial/latest/10_Multisampling.html), [wgpu format flags](https://docs.rs/wgpu/30.0.1/wgpu/struct.TextureFormatFeatureFlags.html), [adapter capabilities](https://docs.rs/wgpu/30.0.1/wgpu/struct.Adapter.html), [Unity AA tradeoffs](https://docs.unity.com/en-us/engine/6000.0/manual/render-pipelines/universal-render-pipeline/anti-aliasing).
