# 0003 Scenario and Vulkan boundaries

Date: 2026-11-04. Status: implementation decisions under autonomous M1/M2 authorization.

## Authored projects

Use versioned JSON via serde/serde_json, one stable scenario identifier, integer logistics configuration and monotonically increasing authored revisions. Keep runtime state out of saved authored data. Bound undo/redo history and file size; unsupported versions and unknown fields are rejected. The first version has no historical migration requirement.

Use a sibling temporary file with flushed/synced contents and atomic replacement for saves. This preserves the prior destination on pre-replacement failure; it is not a universal power-loss durability guarantee. CLI writers take a sibling advisory lock around load/edit/save, then verify the expected revision. Other applications must cooperate with that protocol; arbitrary external file changes are not an atomic database transaction.

## Rendering

Use ash 0.38, ash-window 0.13 and winit 0.30 for direct Vulkan and window lifecycle. Keep safe simulation separate from the explicitly unsafe Vulkan boundary. The initial scene is a schematic geometric visualization, not a production 3D renderer, mesh/material pipeline or editor.

Prefer simple synchronous lifetime ownership for this first integration: one submitted frame at a time and correctly scoped presentation synchronization. Device-idle waits on resize/shutdown trade throughput for clear ownership; optimize only after measuring. Surface/window must outlive dependent resources. Partial initialization and teardown require the same discipline as steady-state rendering.

## Evidence and limitations

Consulted 2026-11-04: [ash API](https://docs.rs/ash/latest/ash/), [winit lifecycle](https://docs.rs/winit/latest/winit/), [tempfile persistence](https://docs.rs/tempfile/latest/tempfile/struct.NamedTempFile.html), [presentation semaphore lifetime](https://docs.vulkan.org/guide/latest/swapchain_semaphore_reuse.html).

GPU validation and a bounded Windows compile assessment must accompany completion. Actual Windows runtime support is not implied by cross-compilation. Rendering budgets and local measurements are recorded in milestone evidence, not generalized benchmark claims.
