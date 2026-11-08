# Engineering remediation proposals

Proposed2026-11-08 after the [comprehensive engineering audit](../evidence/ENGINEERING-AUDIT.md). These packages are recommendations for user selection. The audit execution approval authorizes findings and fix plans. Each selected package requires its own bounded implementation acceptance. Relative effort describes coupling and verification burden, without a calendar commitment.

The [staged improvement sequence](ENGINEERING-IMPROVEMENT-SEQUENCE.md) now proposes scheduling, with native performance attribution beginning alongside urgent recovery and export work. Use that sequence for execution planning and the packages below for detailed finding contracts. Both remain proposals.

## Recommended first milestone

**Restore trustworthy pending-edit recovery.** A creator can keep editing, crash and reopen the latest pending text through ordinary recovery without an editor-produced checkpoint becoming unusable. Cover AUD-003 and AUD-004. This protects unfinished work before optimization or feature expansion.

### A · Draft and recovery integrity

Relative effort is medium to large because already-created checkpoints need a safe restoration policy.

- Reuse current pending-draft authority and add the missing input-action exclusion to visual authoring, Save/New/Close variants and applicable creation callers. Keep Cancel/repair reachable. Avoid a second draft registry.
- Advance the existing draft-generation observation when recovered Fields text really changes. Preserve exact legacy raw text and units, invalid intermediate input, quiet-period scheduling and continuous-edit deadlines.
- Specify restoration of already-produced compound/stale checkpoints. Preserve original journal bytes, expose recoverable drafts with explicit baseline validation, and provide a safe ordinary repair/cancel route. A future-only guard is insufficient. Choose compatibility semantics before changing the recovery codec.
- Demonstrate red-to-green using the retained compound checkpoint and a uniquely targeted second-checkpoint real-widget/scheduler case. Extend existing owners. Verify latest invalid raw text after a real disposable crash/Restore, then repair, Undo, SaveAs and fresh reopen through the ordinary editor. Confirm native MCP still rejects mutations owned by pending human drafts.
- Preserve supported recovery versions, stale/conflict refusal and existing project bytes. Never silently discard one draft to make the other restore.

Stop when both failures are closed and current/legacy recovery plus direct/MCP authority checks pass. Independent review should challenge migration and data-loss paths before acceptance.

### B · Reliable delivery and behavior tooling

Covers AUD-001, AUD-009 and AUD-010. Relative effort is medium. Source-export repair can be selected independently as a small urgent item.

- Include actual shared compiler inputs and fixture closure in deliberate source exports. Preserve no-overwrite atomic publication and existing exclusions.
- Export Yard and Motion into fresh relocated directories, compile their real locked/offline sources and run affected existing tests with the original checkout unavailable. Extend an existing distribution test at the missing-input boundary.
- Use the existing supported Lua backend/API authority for working-copy extension, property hints and version-accurate host declarations. Include current input edges and spatial result shapes. Preserve unknown-version and unfinished-source repair semantics.
- Verify API1/2/3 generation at the owner, then a real Zed/LuaLS completion/navigation journey. Retain working-copy identity, baseline conflict and import safeguards.
- Update SDK smokes to require their tool subset and complete current diff semantics. Run against explicitly disposable fixtures, inspect final state, and retain clear handling of failed or unknown mutation outcomes. Avoid blind retries.

Acceptance is an independently buildable export and discoverable current behavior authoring, with documented current SDK recipes reaching their intended checks. Broader debugger/plugin work stays separately scoped.

### C · Bounded resource admission and off-thread disposal

Covers AUD-005, AUD-006 and AUD-007. Relative effort is medium to large. Select each ownership boundary separately if needed.

- Account cumulatively while descriptor meshes/textures decode, rejecting at the first aggregate crossing before retaining subsequent entries. Keep one bounded candidate decode overlap and stop remaining work. Reuse Content's authoritative budget policy.
- Choose one end-to-end renderability contract for CPU/GPU footprints. Compare explicit GPU closure preflight with avoiding unnecessary UV expansion. Preserve caps until representative integrated-memory evidence supports a change. Surface structured actionable errors before an accepted creation silently falls back. Picking/Fit must match intentionally displayed fallback geometry.
- Keep export staging disposal on a bounded worker owner through success, cancellation, stale preparation, buffered result and controller-drop paths. Retain owner authorization, destination identity, no-clobber publication and a truthful committed receipt. Successful publication plus cleanup failure must remain successful with a warning.
- Extend existing loader/controller cases for early stop and destructor affinity. Exercise real valid near-budget content and rejected over-budget content. Measure foreground completion and whole-process/driver high-water under representative refresh/resize/export churn on both GPUs. Logical bytes and sampled peaks need separate labels.

Acceptance preserves exact content/receipt semantics, bounded queued/retiring work and visible diagnostics. No unbounded cleanup threads or increased caps follow automatically.

### D · Consistent selection, camera and keyboard ownership

Covers AUD-008, AUD-011, AUD-012, AUD-016 and AUD-024. Relative effort is medium, with separately shippable fixes.

- Replace unit-dependent ray intersection admission with a numerically justified scale-aware criterion. Preserve backface, clip and nearest-world-hit ordering. Extend the owning case with equivalent world geometry across source units, nonuniform transforms and supported numeric boundaries.
- Bind workspace capture to its real current widget/frame/focus owner using the existing input-action capture pattern. Cancel on diversion, blur, hidden/disabled/clipped ownership. Verify actual activation, Tab/pointer diversion, blur-return, Escape, conflict and discarded passes without creating duplicate remaps.
- Include every material texture role in cross-root clipboard confinement. Extend the current copy/paste owner with the narrow roughness-only trigger and normal same-root success. Preserve unresolved-resource repair semantics.
- Define a shared spatial numeric domain for imported world extents, camera Fit, navigation and persisted camera. Either produce valid framing or an actionable unsupported-domain result. Zero-scroll hover must preserve state.
- Reuse the existing popup focus helper for Sun Quality after a real-widget red case. Verify keyboard select/dismiss, choosing the current value, subsequent Tab/Escape and outside-click ownership.

Acceptance includes an isolated native focused journey using real app controls. Injected events must be labelled and cannot stand in for physical-human approval.

### E · Ingress, restart and compound transaction contracts

Covers AUD-002, AUD-013, AUD-014, AUD-015 and AUD-023. Relative effort ranges from small ingress changes to larger rename restart design.

- Reuse strict duplicate-key parsing at original-byte ingress and original-f64 lighting admission at the shared model owner. Include current/history/recovery/CLI routes, escaped-equivalent keys and valid writer roundtrips. Preserve deliberately different documented legacy numeric semantics.
- Reject nonregular legacy source/destination/lock paths before blocking open, preserving current project state on failure. Reuse generic Scene policy where compatible. Path prechecks alone cannot promise resistance to hostile replacement races.
- Select a bounded rename intent/reconciliation or explicit relink design. Preserve asset identity and never overwrite unrelated occupants. Test process termination at named publication boundaries, startup recovery and conflict refusal. Specify directory durability separately if power-loss guarantees become a requirement.
- Reproduce combined behavior Apply at an admission boundary and explicitly define atomicity. If one compound action promises atomic application, use an existing Session transaction and clear drafts only after success. Preserve standalone apply and invalid text. Avoid a new transaction abstraction.

The rename guarantee and combined-Apply contract require explicit design acceptance. They can be separated from straightforward ingress fixes.

### F · Maintained surfaces and evidence trust

Covers AUD-017 through AUD-022, with AUD-020 and AUD-021 also important to packages B/E. Relative effort is medium, mostly bounded maintenance.

- Choose whether the renderer demo remains a supported textured-scene host. If retained, use shared immutable mesh/texture snapshots and complete viewport material wiring, then verify actual appearance and refresh identity. If retired, explicitly restrict its documented role.
- Implement required focused Enter/Escape behavior in retained Motion/logistics workflows using widget Responses and owned modal focus. Preserve invalid fields, no-overwrite save and dirty-close semantics. Verify the actual keyboard entry points.
- Establish deliberate full-lock optional/target dependency inventory. Cover all459external baseline identities or narrow the claim with explicit exclusions. Update the existing generator/check so an optional-only omission fails. Preserve original notice bytes and provenance.
- Reconcile maintained current-contract guides against owners once. Label frozen candidate/historical tables by source commit. Preserve immutable historical acceptance evidence and limitations. Prioritize first-run source/build/runtime recipes and exact current timing/version contracts.
- Report validation/profile conditions as established or unknown in measurement tooling, preserving raw overrides and binary identity.

Acceptance includes source-consistency review and affected documentation/script checks. Avoid source-text snapshot tests for ordinary prose.

### G · Hardware-accessible performance qualification

A measurement package following correctness/ownership fixes. Relative effort is medium to large. It must not become an open-ended optimization goal.

Select one concrete creator workload and a supported device/quality target first. Define editor interaction/cadence expectations with the user, then allocate budget from the measured native critical path and practical headroom. Preserve the audit's slow valid Intel runs. Measure normal-use and conditioned regimes separately with contemporaneous power/clock/thermal telemetry.

Start with actual selection/navigation/typing and event-driven refresh/export/recovery on integrated graphics. Separately profile dense color/shadow cost, resolution/MSAA, memory high-water and CPU preparation. Use controlled variants, fixed source/fixture pins, sufficient independent paired processes and visible quality acceptance. A validation-off shipping-like cohort needs a separate protocol from correctness-validation runs.

Only implement an optimization after identifying its attributable owner, comparing a smaller alternative, documenting invalidation/admission consequences and measuring before/after latency, memory and quality. Keep existing fences, resource caps and acknowledgements unless replacement proves their contracts. Broad ECS/render-graph/caching/BVH changes remain hypotheses until such evidence exists.

## Selection and stopping discipline

Recommended order is A, urgent source-export repair from B, then remaining delivery/admission/interaction packages according to measured creator impact. A small source-export fix may precede A if independent scheduling improves delivery, while recovery remains highest data-integrity priority. Documentation contract correction should accompany each selected owning fix. Remaining broad guide cleanup belongs to F.

Do not bundle every finding into one long-running refactor. Set one milestone with a concrete creator outcome, explicit finding IDs, supported compatibility, proportional checks and a stopping condition. Update STATUS and decision records for accepted consequential choices. Completion closes only that milestone's verified contracts. The remaining audit findings keep their individual unresolved status.
