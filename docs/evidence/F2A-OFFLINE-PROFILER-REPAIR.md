# F2a offline profiler repair evidence

Updated 2026-11-09. The user selected the [bounded repair](../plans/F2A-OFFLINE-PROFILER-REPAIR.md). Product source remains `6e4426972c9ef147239ef3a02b2b4810e02a4148`. Overall F2a cost acceptance remains **HOLD**.

## Provenance and retained attempts

All external artifacts live under `/home/akilf/Projects/Ruster-Audits/f2a-application-lifecycle-20261109/profiler-repair-1/`. The original recording, old classification and failed v2 cohort remain unchanged.

The private baseline uses official stable Git tag `v7.3.1`, commit `79a907609d53d5cd645b42c11550d295d43abf99`, tag object `dfc1145bbaa1c5dd626fd25d9ae8fc277864e7bd`. Signature verification was not performed. The initial standalone archive URL returned404. A slow full archive download was interrupted and directly waited130, with its partial file retained and unused. Sparse checkout initially omitted required build files. The first build failure and subsequent sparse-path correction are retained. Root corrected the perf wrapper's parallelism setting to `JOBS=4`.

`source-provenance.json`, `build-config.json` and the two build-owner records retain source identity, commands and completion. Private offline build options differ from the packaged tool. Both matched builds use identical options with libdw unwinding enabled and strict `WERROR=1`. Baseline build completes0 in43.160seconds. Patched build completes0 in47.551seconds. No Engine binary was rebuilt or executed.

| Binary | SHA256 |
| --- | --- |
| Matched unpatched perf | `302b3939b113ad78ab05f8be601c2065cd28180eca711a880ec596c4fa31ef3f` |
| Matched patched perf | `89f7996b582effce8f44a6f69c6194bbb4ce2b13fd3f82486fc9b1c988dfa791` |

`final-source-pins.json` pins the seven private integration/helper/fixture files. `private-tracked.patch` retains the three tracked perf changes. The two added helper sources are separately pinned and retained in the private tree. The system profiler installation remains unchanged. A post-replay closure audit rehashes84original file pins and36symlink targets with zero mismatches. `recorded-buildid-crosscheck.json` independently compares the actual archived executable/libc/libpython ELF build IDs with the retained recording build-ID table. All three match.

## Matched failure and repaired sample

Root executes `python3 replay.py baseline selected`, then `python3 replay.py patched selected`. Each uses a private empty home/cache/build-ID directory, empty configuration and `DEBUGINFOD_URLS=''`. Commands, directly waited exit0, input hashes and output hashes are recorded in the respective `parent.json`. The matched unpatched replay reproduces the original three addresses and wrong stack read. The original trace's debuginfod limitation remains in its historical record.

For frozen sample1179, the unpatched profiler reads stack address `0x7fff67cee148`, offset240, yielding `0x3f80000000000000`. The corrected profiler reads `0x7fff67cee458`, offset1024, yielding `0x560367f9bccb`, exactly matching the independently verified captured bytes. Its displayed return address is `0x560367f9bcca`. The chain continues through `Engine::step_with_input`, `Engine::step`, harness `execute`, and harness `main`, then ordinary startup frames. Frame count rises from3 to12.

The old verbose relative-address display still uses the original symbol-offset conversion. Qualification depends on the actual callback memory request, recorded bytes, correct DWFL relocation and recovered ancestry. The repair does not change that display conversion.

## Complete offline replay

Root executes `python3 replay.py patched full`, directly waited0 in0.966seconds. The unchanged classifier reads all1688samples with identical ordered PID/TID/CPU/time/period/event tuples against the original classification. It reports zero unparsed lines, zero loss/throttle records and410rooted-Step samples against the original minimum100 criterion. This is a new offline capability result. The original failed v2 cohort remains failed.

`replay-patched-full/classification.json`, `summary.json` and `representatives.json` retain complete results. All twelve originally selected indices are preserved. Samples1179/1183/1184/1195 recover rooted Step ancestry. Sample1196 becomes oracle,21/22/23 become known harness, and210 retains Reset ancestry. Sample24 and Python-image controls1/2 remain unresolved for the target harness. They are retained without substitution. Across the full recording29samples lack a known target harness root,2244symbol frames remain unknown, and exact stack truncation count remains unestablished. Full stderr retains five explicit regular-ELF rejections for unsupported machine or missing DSO build ID. Existing trailing DWFL error context does not supersede those explicit reasons. Astra independently recounts the samples and verifies that all bounded explicit-CFI prefixes from the prior twelve-sample audit agree with the repaired output.

The recording contains baseline A/noop only and combines warmup, retained-state and other process activity. It supplies no paired candidate comparison and does not diagnose the high-changed workload's sustained increase. Recovered environment-construction/VM/GC categories describe sampled user-cycle attribution within this recording. They cannot establish the cause or magnitude of an A/B slowdown.

## Helper qualification

Root compiles `preparation/elf_bias.c` with `bias_test.c` using C11, optimization, debug symbols and `-Wall -Wextra -Werror`, linking libdw/libelf. The driver completes all20arithmetic/boundary cases successfully. `helper-check-1/math.json` retains line-oriented results despite its historical extension. Root also compiles three benign ELF fixtures and retains their program headers. These fixture programs are never executed.

Root reviews the finite driver, prepares `file-qualification-1/plan.json` with SHA256 `b34a7a9f3c5ba19a232361efc0966f260de894e63177f5e9b582278019310d60`, and executes its exact plan once. All11cases finish normally0. Seven accepted cases cover ordinary/nonzero-initial-VA ET_DYN, fixed ET_EXEC, archived executable, recorded benchmark libc, recorded perf-host libpython and a middle mapping. Four negatives require exact errors for wrong build ID, truncated header, nonexecutable mapping and wrong expected ELF type. Accepted ET_DYN cases also verify cached conflicting-bias rejection, session-discard indication, incoming descriptor closure and preservation of the old module until teardown. Cached reuse and final descriptor closure pass. The Python library is explicitly a perf-host mapping control. It is not presented as a benchmark dependency. No fixture executable runs. The driver/source/ELF/input pins are checked before and after every helper process, with a15second direct-child timeout and stop on first failure. No timeout or failure occurs. `result.json` and per-case parent/stdout/stderr files retain the executed evidence. Supported qualification is bounded to regular x86_64 little-endian ELF64 files with independently pinned recorded build IDs. JIT, other architectures, arbitrary malformed-file security and universal unwind completeness remain unqualified.

## Current handoff

The repair now recovers the diagnosed return and useful ancestry from existing data. Actual-file qualification passes. Astra independently accepts source, replay and all actual-file results, including rehashed15plan inputs, independently recomputed seven positive biases and six dynamic cache-conflict checks. No material private-tool qualification blocker remains. Subsequent cost investigation needs the separately bounded method below. Product-cost HOLD, failed cohort history and unchanged functional/native verification remain authoritative. All work is background file analysis/private builds, with no desktop focus changes or idle timing claims.


## Next cost method proposed by Astra/root

A separately frozen high-changed-only attribution block is the next proposal. Maximum four fresh Engine recordings use exact archived binaries in A/B/B/A order, existing fixtures/counts/CPU8/event/rate/stack settings and the qualified complete ordered-environment restoration. Freeze source, binary, profiler, input, schedule and analyzer identities before dispatch. First A must pass complete correctness/environment/cleanup checks and the unchanged100rooted-Step minimum before releasing the other three. Every later record must pass the same checks. Any failure stops the block with all results retained and no replacement. Keep existing180second recorder/240second owner deadlines and no automatic extension.

Analyze absolute sampled user-cycle periods and matching call paths per process, with Reset/Step/oracle/other harness attribution separated and unresolved coverage retained. Step includes111calls comprising warmup, measured and retained-state work. It cannot isolate100timed calls. Retain those ordinary timing outputs separately. The single question is whether candidate B adds a consistent Step path in both orders that the product source difference supports. A positive result selects a bounded counterfactual repair candidate requiring subsequent verification. An inconclusive result ends as unlocalized with cost HOLD. Whole-process percentages and nested category sums cannot establish added Step work.

The earlier A/noop recording cannot be paired with a fresh B/high-changed record. A phase-instrumented rebuild would change the questioned regime again. User-active background sampling can reveal paths but competing load prevents clean interpretation of the roughly4percent difference. Prefer a quiet opportunity for this finite block. No new Engine recording has been dispatched in the profiler-repair slice, and the failed v2 cohort is never continued. No product optimization, threshold waiver, F3 activation or new Goal-tool goal follows from tool qualification.

The final Markdown-only gate against `e7b11c7` passes in0.36seconds. Unchanged Rust/native product verification is reused. The external evidence inventory contains93files.
