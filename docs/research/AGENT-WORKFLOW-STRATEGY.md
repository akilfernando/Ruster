# Agent workflow research assimilation

2026-11-16. Requested research assimilation after static GLB delivery; inspected
cumulative source `b012e50`. This is Astra's strategic recommendation, not user
approval of every statement in the supplied report or an implementation result.
The canonical schedule is [ROADMAP](../ROADMAP.md); the next bounded proposal is
[repeatable play checks](../plans/REPEATABLE-PLAY-CHECKS.md).

## Provenance and reliability

The complete user-supplied [report](sources/2026-11-16-agent-engine-report.txt)
is retained verbatim, including its unsupported claims. Original attachment:
`/home/akilf/Projects/Rusterize/.codex-attachments/f038c911-8e5d-44b6-b7c3-3a19da16865a/16d25415-3b92-4f5d-9402-84338c79296c/deep-research-report.md`.
SHA-256: `27955dd7cd8202cd7c8ff2a7e36b4a115b012c29641911482991ac438471d368`.
Its November 2026 label and generic Sources paragraph do not identify retrievable
citations. Treat the report as a hypothesis-generating input. Its illustrative
benchmark table is invented example data, not evidence. No dates, savings,
market demand or competitive superiority are adopted from it.

Primary sources below were opened on 2026-11-16. They establish documented
facilities, not comparative reliability or measured performance. Unversioned
vendor pages and `latest` documentation may change.

| Report claim | Checked evidence and disposition |
| --- | --- |
| Unity has official MCP/CLI integration but lacks a unified command model | Unity documents Pipeline remote commands and a shared command ID mapped to MCP tool names. Its old in-editor assistant MCP is deprecated in favor of CLI MCP. The broad architectural absence claim is unsupported. [Pipeline](https://docs.unity.com/en-us/unity-cli/unity-pipeline/unity-pipeline-package), [command conventions](https://docs.unity.com/en-us/unity-cli/command-name-conventions), [migration](https://docs.unity.com/en-us/unity-cli/replace-mcp-server-unity-cli). |
| No competitor emphasizes context efficiency | Unity's migration documentation explicitly promotes direct commands for lower token use. This is a vendor claim, not a measured Rusterize comparison. Reject exclusivity. |
| Unreal offers broad experimental MCP | Epic's current page, labeled UE5.9 when accessed, documents editor tools, profiling and automation tests and calls the feature experimental. This supports capability, not the report's exact first-version assertion or production maturity. [Epic MCP](https://dev.epicgames.com/documentation/unreal-engine/unreal-mcp-in-unreal-editor). |
| Bevy lacks built-in scenes/assets and is unsuitable for headless work | Current official crate docs describe scene composition and asset loading/hot reload; upstream has a headless example. Reject these absence claims. Do not infer a need to migrate Rusterize. [Scenes](https://docs.rs/bevy/latest/bevy/scene/index.html), [assets](https://docs.rs/bevy/latest/bevy/asset/index.html), [headless source](https://raw.githubusercontent.com/bevyengine/bevy/main/examples/app/headless.rs). |
| Godot plugins have particular tool counts, installs, prices and superior maturity; Fyrox/Bevy have no agent integrations | Not re-established here and not used for scheduling. Negative ecosystem claims require a scoped, dated search; counts/prices are volatile. The existing [mature-engine study](MATURE-ENGINE-PARITY.md) remains the capability comparison. |
| Nobody provides transactions, verification or selective context | Documentation omissions cannot establish universal absence. Evaluate a pinned competitor workflow before making any comparative claim. |

Initial `bevy.org` scene/headless/assets URLs were inaccessible through the web
tool; official crate docs and upstream source supplied the narrower evidence
above. No competitor was installed or benchmarked.

## What transfers to the existing product

The report's strongest idea is measuring the complete edit → run → inspect →
repair → ship workflow. Supporting agents well is useful when a creator can
understand the same result and continue manually. This fits the existing
[DIRECTION](../DIRECTION.md), which remains the authority for product identity.

Source inspection found substantial foundations already present:

- `crates/ruster-scene/src/authoring.rs` and scene Session/history own authored
  operations; the editor and MCP already share guarded operations. This does
  not establish that every future interface has complete parity.
- `crates/ruster-mcp/src/runtime_tools.rs` already offers asynchronous Reset,
  pinned input Step, bounded committed-state reads, operation receipts and
  cancellation/Stop. `scene_replay` is a static scene facility, not a gameplay
  verifier. Preserve that distinction.
- `crates/ruster-behavior/src/lib.rs` exposes runtime state, query observations,
  Reset and `step_with_input`; `crates/ruster-app/src/lib.rs` supplies application
  playback. Existing delivery checks already run pinned input sequences.
- Static GLB [resource generations](../decisions/2026-11-16-static-glb-resource-generations.md)
  preserve accepted imports separately from editable placements and caches.
  Authored history, asset publication and runtime Reset are different ownership
  domains; a universal rollback promise would be false.
- Enrolled native connection and capture-bound authored geometry grounding have
  actual-client acceptance. Runtime image grounding remains unsupported. Do not
  silently describe a state assertion as visible-play correctness.

Root's independent read-only probe on 2026-11-16 ran the existing relocated
Scene21 follow fixture twice with two named-action frames. Both broad receipts
were byte-identical (8,091 bytes), tick 2, subject `(2,0,0)`, camera anchor
`(2,1,0)`. Evidence: `/home/akilf/Projects/Ruster-Audits/research-playcheck-baseline.MBrytk`.
This supports reuse feasibility for one fixture; it establishes neither general
determinism nor a new package, performance or release qualification.

## Adoption, adaptation and rejection

**Adopt as planning criteria:** shared validated operations; repeatable runs with
explicit observations; reversible authoring; concise actionable failures; bounded
context; and outcome measurements including repairs and human interventions.
These extend existing foundations instead of restarting an MVP.

**Adapt:** make repeatable checks creator-authored and usable without AI. Assert
specific rules and label unobserved properties; never claim that passing a small
scenario fully tests a game. Use existing targeted state pages before inventing
a query language or subscriptions. Introduce batching only for a measured costly
workflow with clear transaction boundaries. Treat deterministic physics as a
separate solver contract, not a prerequisite for the next check slice.

**Do not adopt:** prioritizing agent tooling over required everyday editor UX;
mandatory approval for every edit after enrolled project trust; a greenfield
ECS/renderer/MCP rebuild; Bevy/Fyrox migration without a measured blocking need;
an agent-only product; schedule estimates; multi-agent merging; or publication
and marketing activities. The report does not override human-only authoring,
Rust/Vulkan, openness, core capability development, later LOD/ray tracing, or
separate release authority. Established libraries remain candidates for missing
subsystems when compatibility, licensing, maintenance and measured behavior fit.

## Priority judgment

Immediately implementing audio, animation or physics adds missing creator
capability. A broad agent platform would delay those jobs and is not selected.
One small repeatable-check slice is worthwhile because the current source already
runs the necessary playback; creators lack a convenient saved, inspectable
expectation-and-result loop. It can make subsequent collision/lifecycle changes
safer without manufacturing another simulation engine. Its first probe must
show that reuse is cheap; otherwise return to the highest concrete capability
blocker and retain existing developer checks.

Proposed positioning is therefore **a creator-first open Rust/Vulkan engine with
reliable human and agent iteration**. This is a working description consistent
with existing direction, not a new market claim or a user-approved rebrand.
AI-assisted small teams are a research cohort, not an exclusive new audience.

For eventual comparison, first finish one honest Rusterize workflow. Then compare
one equivalent job on pinned engine/plugin/model versions with equal starting
assets, acceptance criteria and intervention accounting. Report attempts,
failures, end-to-end time, tool calls, output bytes and measured model usage when
available; bytes are not tokens and neither implies billing savings. Include
setup, repairs and human work. No universal cross-engine benchmark suite or
competitive speedup target is necessary for the next feature.
