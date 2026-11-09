# 0046 — F2a measured performance tradeoff

Date 2026-11-10. Status accepted by the user with “Let's accept it then and make note of it”. This settles F2a retention after the [final bounded investigation](../evidence/F2A-FINAL-PERFORMANCE-ATTEMPT.md). It supplements [0045](0045-initialization-and-paused-application-commands.md).

## Context

F2a supplies computed initialization and typed paused Application commands through ordinary editor controls, native MCP and standalone delivery. Product source `6e4426972c9ef147239ef3a02b2b4810e02a4148` has qualified functional, compatibility, keyboard, recovery and delivery evidence with the recorded exclusions.

Original108compatibility and48CPU8 affinity records show a repeated approximately4percent/+0.49millisecond ordinary Step increase for the dense changed-state legacy workload. That workload has255scripted objects with64state fields each. The cause and typical creator-project impact remain unestablished. This is CPU-side scripting/state processing evidence.

The final four-record A/B/B/A comparison passes setup, complete correctness, identical full/ordered environment, ownership and profiling gates. It shows smaller descriptive differences in its separate observer regime. Independent call-path/source review selects no demonstrated additional operation or targeted repair. Original adverse measurements remain authoritative within their recorded scope. The investigation ends inconclusively under the user-approved stopping rule.

## Alternatives and rationale

Retaining F2a preserves verified creator capabilities and its shared lifecycle/state foundation. Continuing acceptance HOLD with feature revision or deferment was the alternative presented to the user. Root/Astra recommend explicit retention because further speculative optimization has no source-supported target after the bounded investigation. The user accepts the documented tradeoff.

## Decision

Accept and retain bounded F2a with the measured dense-state cost and unresolved cause recorded as an accepted limitation. Overall F2a milestone acceptance is settled. Preserve original raw results, failed diagnostic attempts, final valid comparison, exclusions and uncertainty. Record no performance repair or causal explanation.

The accepted scope is this F2a workload-specific tradeoff. Existing performance policy and future change verification remain in force. General performance equivalence, typical-project impact and broader hardware behavior remain unqualified.

## Consequences and continuity

The unresolved issue is tracked by this decision and the current status. Representative creator-project evidence can inform future performance priorities. Any future attempt to revisit this cost needs a concrete hypothesis and its own bounded method. The final profiler investigation stays closed.

F3 runtime UI is the next candidate for a separately scoped proposal. This acceptance creates no new implementation goal, public release, M9 completion or approval of broader lifecycle/event systems. Product code, dependencies and existing verification artifacts are unchanged by this documentation decision.

## Evidence and supersession

[Functional qualification and chronological failures](../evidence/F2A-APPLICATION-LIFECYCLE.md), [final comparison and closure](../evidence/F2A-FINAL-PERFORMANCE-ATTEMPT.md), [current milestone state](../STATUS.md).

This decision supersedes the pending user-product-decision and overall F2a cost-HOLD disposition recorded before acceptance. It preserves those historical observations and all remaining qualification boundaries. No superseding decision exists.

## Verification

Documentation-only `python3 scripts/check.py --base 18cdec4` passes the Markdown whitespace/link lane in0.19seconds. Existing functional/native/performance evidence is reused against unchanged product source and dependencies. No new benchmark or product test is claimed.
