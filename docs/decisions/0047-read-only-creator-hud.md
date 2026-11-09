# 0047 — Read-only creator HUD

Date 2026-11-10. Status accepted implementation contract after the user approves [F3a](../plans/F3A-LIVE-HUD.md) with “Ok, let's do it”. Retention awaits qualification. Specific representation and limits are agent-selected defaults serving that approved outcome.

## Context and alternatives

F1/F2a provide committed typed state and paused commands. A screen-space HUD exposes creator-owned data inside the creation. Interactive menus require a separately selected running-command contract. Existing egui and immutable runtime frames provide a shared presentation foundation. A separate UI framework would add another rendering/input/resource owner. Putting HUD definitions in Scene would copy their text with existing per-Step scene clones.

## Decision

Store default-omitted HUD definitions under shared Authoring, captured by immutable runtime/frame ownership. One four-corner-anchored vertical group contains up to32stable-ID labels. Static text or explicit typed state references use literal affixes, fixed locale-independent Number/Vec3 precision0–6, lowercase Bool and literal Text. Preserve signed zero. Authored text is limited to1024UTF-8 bytes per element/32KiB total. Resolved output is bounded separately at2KiB per label/64KiB total through a capped writer. Overflow becomes a per-label diagnostic.

Style uses finite original-f64 width64–1024, margin0–256, spacing0–64, font8–48 and RGBA0–1. Defaults are width320/margin12/spacing6/font14, white text and a dark translucent background. Effective viewport constraints preserve authored values, wrap text and clip overflow with observation feedback. Bottom anchors align the ordered group's bottom. These logical bounds do not certify total font/Galley/GPU memory.

Structural validation is strict, including duplicate IDs and bounds. Missing/disabled bindings, missing sources/fields and changed types remain recoverable diagnostics with a bounded unavailable placeholder. Broken definitions stay saveable. New/rebound labels require valid enabled typed references. Formatting/style edits preserve an existing broken reference for repair. Deleted binding/label identities referenced by current or reversible HUD history remain reserved against silent retargeting. Field names are explicit identities within their binding. Renames never cause fuzzy matching or automatic migration.

Editor and runtime use one shared pure resolver/cache and an egui Painter-only overlay. Rendering has no hit regions or focus ownership. It reads only referenced committed values, without script evaluation or whole-state serialization. The app host adds a default no-op viewport hook after advance and viewport creation. Editor presentation follows the exact displayed Frame, including captured Authoring/state/scene. Authored preview uses defaults with a clear designation. Stale/failed runtime views preserve captured last-committed data. HUD edits use the existing runtime invalidation/Reset boundary.

Session edits supply atomic history. Raw numeric/text/reference drafts retain original baselines and revision for recovery13, including unfinished invalid input. Scene15 and content11 require the new exact cumulative runtime_hud profile, including empty-HUD new-schema exports. Descriptor5 remains. Older documents remain accepted; older-version relabelled HUD fields reject explicitly. Old runtime kits reject before publication.

## Native MCP and bounded transport

Keep existing64KiB bridge/request/reply and256KiB MCP output limits. Whole maximum HUD replacement could exceed those limits after JSON escaping. Use bounded settings mutation, individual label upsert/remove, individual label reorder and one-label paged inspection. Every mutation builds the same candidate HUD and uses Session validation/history. Inspection preserves model/last-prepared distinctions and identity. Definition, model and last-prepared text are paged together. Large new queue/frame limits are unnecessary.

## Consequences and verification

This enables useful text/data HUDs through editor/MCP and packaged runtime, with interactive controls remaining separately scoped. Existing code-first applications retain a no-op overlay by default. Shared formatter/painter contracts keep both creator paths coherent.

Qualification requires the complete [F3a gates](../plans/F3A-LIVE-HUD.md), including actual keyboard/raw recovery, independent authoring/export, older-kit/format boundaries, passive overlay input and finite measured cost/resource evidence. Baseline source420381f is archived in the private F3a audit workspace. Matched measurements must distinguish model/layout CPU from native presentation/GPU work and preserve the closed F2a investigation. No retention or performance result is inferred from source review alone.

Primary layout/focus/binding sources and access dates are retained in the plan. No superseding decision exists.
