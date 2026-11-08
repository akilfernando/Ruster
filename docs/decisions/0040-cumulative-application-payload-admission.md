# 0040 — Cumulative application payload admission

Date 2026-11-08. Status accepted and verified for the bounded AUD-005 repair. Product source `4e87f83` passes the frozen correctness/resource/regression gates and independent review.

## Context

The user authorizes the asset-admission experiment after the first engineering phase. Actual descriptor loaders return all decoded maps before Content aggregate admission. Baseline public maps retain111848100texture bytes in the five-chain case. Ordinary loading reads all five files and then rejects the64MiBaggregate in Content. This is avoidable suffix work after the fourth asset already crosses the cap. Existing AssetStore accounting excludes a replaced ID and has different ownership semantics.

## Decision

Use two small crate-private checked mesh/texture byte policies inside ruster-app. CreationManifest loaders decode in stable BTreeMap order, charge actual payload_bytes and check the corresponding limit before inserting each candidate. Keep48MiBCPU mesh and64MiBrole-aware texture limits independent. No new retained payload owner or buffer is introduced.

Content remains an independent final defense. Preserve its existing full-family validation before aggregate rejection through separate validation/accounting passes or equivalent ordering. Its count, cross-kind ID, scene-reference, UV and texture-role checks retain their owners. Manifest version/count/ID/path/role/normal-policy metadata is validated before resource reads. ResourceRoot retains canonical confinement and bounded regular-file decoding. AssetStore's replacement-aware policy remains unchanged.

A descriptor's earlier payload crossing can preempt subsequent resource I/O/decode errors and final Content checks that execution has yet to reach. Content's own function retains its prior diagnostic order when directly called. This changes the descriptor loading phase's failure boundary, without adding new descriptor collision or scene-reference rules. Diagnostics should identify the crossing stable asset and applicable budget.

## Alternatives

Collecting every decoded resource preserves avoidable work after a known crossing. A public registry or worker/scheduler redesign adds ownership and API scope beyond the reproduced defect. Moving store replacement semantics into this additive descriptor path would confuse distinct contracts. Applying early admission inside Content's validation loop would also change direct-call validation precedence. The local shared policy and separate Content passes preserve those responsibilities.

## Consequences and verification

Valid near/exact-cap content and legacy interpretations must remain byte-identical. Each mapped ID counts even when paths or Arcs are shared. Earlier rejection deliberately supersedes unreachable later work. Prefix plus one crossing candidate can exceed the aggregate cap while its actual decode completes, so the cap remains a retained admission policy with a one-candidate overlap.

Astra/root freeze the comparison after the unmodified baseline and before production edits. Owning red→green tests, complete read traces, public content fingerprints, a separate untimed private boundary-payload diagnostic and fixed paired measurements determine retention. Timing uses uninstrumented binaries. Source limits and CPU24/32byte render-layout mismatch AUD-006, plus staging-disposal AUD-007, retain separate scope. [Experiment](../plans/ASSET-ADMISSION-EXPERIMENT.md), [evidence](../evidence/ASSET-ADMISSION.md).

## Supersession

No prior decision is superseded. A future streaming or renderability policy requires its own accepted decision and measurements.


## Verified outcome

The expanded existing owners pass red→green and the full 283 Rust/28 Python gate. Public valid content remains identical, while complete read traces and separate private borrowed observations establish earlier suffix avoidance and the expected completed-payload overlap. All six fixed paired comparison gates pass. The five-texture rejection workload observes about 22% lower loading wall/whole-child CPU and 16.1 MiB lower process RSS high-water on this shared warm-cache host. Exact-cap timing retains outliers/order sensitivity and conditional pointwise intervals. Final retention includes those qualifications. [Measured evidence](../evidence/ASSET-ADMISSION.md#independent-review-and-retention--2026-11-08).
