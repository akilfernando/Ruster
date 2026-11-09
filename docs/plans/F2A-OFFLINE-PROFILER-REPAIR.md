# F2a offline profiler repair

Selected2026-11-09 after the user approves the proposed isolated repair. F2a remains functionally qualified, with overall cost acceptance HOLD. This slice qualifies offline tooling against existing recordings.

## Outcome and selected approach

Recover the verified sample1179 continuation through Step, Engine::step and its harness using correct ELF relocation. Preserve input recordings and executable bytes. Validate ordinary ELF layouts and explicit failure cases before accepting the tool.

Root/Astra select a private perf7.3.1 source patch. It retains the recording parser, exec/MMAP chronology, register decoder and libdw callbacks. A standalone adapter would take ownership of those sensitive boundaries, and interposition lacks the originating map context. A private matched unpatched build must reproduce the original failure before the patched build is credited. Build options and library identities remain explicit because the private build can differ from the packaged executable.

The agent-selected regular-ELF resolver computes file position F from mapping pgoff plus the queried IP's offset in the mapping. For an executable PT_LOAD containing F, derive ELF virtual position V from p_vaddr plus F minus p_offset. Relocation bias B is IP minus V. Check arithmetic, mapping/segment bounds, ELF identity and a unique consistent result. Unsupported or conflicting input stays unresolved.

For ET_DYN, report B through dwfl_report_elf with add_p_vaddr enabled. ET_EXEC requires B zero in this bounded implementation. Check reuse and successful registration through dwfl_module_getelf's returned bias and DSO identity. Use the same validated opened ELF for registration, with explicit descriptor cleanup. The existing JIT path retains its separate scope. No general symbol-address conversion or registry is introduced.

## Finite qualification

1. Freeze source, build options, tools, libraries, original recording and ELF identities before comparative replay.
2. Run the existing selected-sample failure through a matched unpatched private tool. Retain its actual failure.
3. Run the patched tool against the same recording. Require the independently verified return slot and Step-to-harness continuation.
4. Recheck the twelve previously frozen representatives, preserving unsupported or truncated outcomes explicitly. Do not replace inconvenient samples.
5. Check actual ordinary libc/Python layouts and a finite helper table covering ET_DYN nonzero initial virtual addresses, ET_EXEC, middle mappings, no match, conflicting segments, malformed bounds and overflow.
6. Verify unchanged data/executables, known child completion and an independently reviewed source/result handoff.

An offline full-record replay may assess recovered ancestry after the fixed representatives qualify. It is a new tooling capability observation. The original v2 cohort stays failed, and the original minimum100 rooted-Step criterion is preserved.

## Ownership and stopping boundary

Root owns compilation, tool execution and documentation. Children prepare disjoint source or perform read-only review. All work stays in the external F2a audit workspace. The user is actively using the desktop, and this slice requires no desktop focus or idle timing conditions.

Stop at accepted or rejected offline capability with recorded limitations and a concrete handoff. New Engine recordings, changed stack/rate thresholds, product optimization, F3 activation and F2a cost acceptance need separate evaluation. No failed cohort is resumed or retroactively accepted.

## Sources and evidence

Accessed2026-11-09. Version-matched and current perf both use map-start minus pgoff in regular module reporting. The selected correction follows ELF program-header semantics and the installed libdwfl public contracts for reporting and relocation bias. [perf7.3.1](https://raw.githubusercontent.com/gregkh/linux/v7.3.1/tools/perf/util/unwind-libdw.c), [current perf](https://raw.githubusercontent.com/torvalds/linux/master/tools/perf/util/unwind-libdw.c), [ELF program headers](https://gabi.xinuos.com/elf/07-pheader.html).

The existing recorded discrepancy, raw-CFI proof and observational limits remain in the [F2a evidence](../evidence/F2A-APPLICATION-LIFECYCLE.md#offline-audit-and-selected-transition-disposition). Actual repair results receive a separate evidence record after execution.

## Executed result

2026-11-09. Matched RED/GREEN replay, all12retained representatives, complete1688sample replay and20arithmetic/11actual-file checks pass within the recorded limits. The corrected tool recovers410rooted-Step samples. [Evidence, retained attempts and next cost-method proposal](../evidence/F2A-OFFLINE-PROFILER-REPAIR.md). F2a cost acceptance remains HOLD.
