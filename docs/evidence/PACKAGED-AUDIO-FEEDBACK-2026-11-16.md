# Packaged one-shot audio qualification

2026-11-16. Fixtures, capture scripts, raw monitor samples, pin manifests
and logs are in
`/home/akilf/Projects/Ruster-Audits/packaged-audio-20261116.b1pvOJ`.

## Transaction and output

A real Lua4 fixed-Step fixture requests a half-gain cue from a named
keyboard action. The first binding stages the request and a later binding
can reject the transaction. The failure leaves tick 0 and no new cue.
One successful Step commits the exact cue once. Separate behavior cases
cover Reset trials, cancellation, invalid or unassigned references, the
request limit and retaining the previous committed receipt on failure.

The device probe used a new PipeWire null sink and its monitor. The first
full Lua/device run used CPAL's default buffer and captured silence for
the cue and pause durations. The bounded device-buffer request resolved
that result. The passing core trace captured the expected 4500 peak
against a 9000 peak WAV at gain 0.5, with initial silence, playback,
pause silence, resumed playback, Stop silence, a fresh cue and Reset
silence. Duplicate delivery of the same commit generated no added
amplitude. The default sink stayed unchanged throughout. Logs include
`audio-core-tracer.log`, `pcm-verification.log` and the raw samples.

## Ordinary editor

A disposable editor imported a WAV using Enter in the ID and path fields.
Content showed a ready Sound. The Behavior panel created the typed
property, retained a rejected 1.1 gain draft, reverted it with Escape,
then accepted the WAV assignment and 0.5 gain with Enter. Save and reopen
retained the property and accepted catalog. The reopened project
produced monitor peak 3000 after the normal **Control runtime** and D
key path, and peak 0 during Reset and paused Step. The Runtime menu
reported paused output and the last committed cue. Stop, exact scene-byte
preservation, default sink verification and workspace restoration passed.
Input was scoped native automation. Shared editor Undo coverage exercised
the guarded typed property. See `EDITOR-QUALIFICATION.md`.

## Guarded native MCP

The real disposable STDIO adapter imported the WAV, refused assignment
of a known OBJ ID to a Sound property, and accepted the repaired
reference. Reset exposed no batch and a paused supplied Step exposed
one exact cue while physical output remained idle. Save and fresh
Session readback passed. The earlier kit refused the saved scene before
publication. The matched export published Descriptor6, then a relocated
headless replay preserved the saved scene and WAV bytes and produced
the exact cue receipt. See `STDIO-RESULT.json`,
`STDIO-EXPORT-RESULT.json` and `stdio-journey-3.log`. The two earlier
audit runs stopped on overly strict message literals after otherwise
correct refusals.

## Relocated Linux package

Built Motion Lab with remapped paths in optimized mode, prepared the
capability-bearing runtime kit, and staged exact Scene21, WAV and case
bytes. Package hashes are in `runtime-kit-receipt.json` and
`delivery-package-receipt.json`. `--play-check` passed at tick 3 with one
cue state transition despite an explicitly nonexistent audio target.
`--headless` at tick 1 emitted the expected cue receipt. The relocated
native package accepted ordinary keyboard focus and D. Its private
monitor captured one 1.51-second cue at peak 4500, surrounded by
silence. The compositor closed the exact owned app normally and the
default route and workspace were restored. See
`native-relocated-03/result.json` and `native-relocated-03/monitor.raw`.
An earlier accessibility attempt found no exposed standalone owner and
the keyboard route verified the same binary.

The exact prior kit packaged the earlier no-audio case. The new kit
packaged that same case as Descriptor5, passed its saved check at tick 3
and produced byte-identical headless output. Both helpers refused a
null Sound property with the old kit and left the output absent.
The qualified device route uses float PCM on PipeWire's PulseAudio
server. Default devices exposing integer samples only remain outside
the verified device support.

## Cost and checks

Matched release Engine cohorts on an Intel Core i9-13900H used CPU0,
200 warmups and 6,000 measured Steps per case in alternating order.
Each sample started from a fresh Reset outside the timer. The fixture
used the same named action, motion and state assertions. The cue case
added one typed cue with the exact same state transition.

| Case | Median (µs) | p95 (µs) |
| --- | ---: | ---: |
| Previous no-audio runtime | 19.264 | 28.750 |
| Current no-audio runtime | 19.258 | 28.681 |
| Current cue commit | 21.002 | 32.341 |

The cue adds about 1.74 µs in this fixture. This times the committed
behavior transaction. The host can additionally retain one Kira frame
copy per played clip, so decoded samples may approach 128 MiB across a
full 64 MiB catalog and one active playback lifetime. Encoded files and
ordinary process allocation are separate. No decoding runs in the callback.
One optimized live first submission took 27.97 ms including device open,
frame conversion and scheduling. Process VmRSS rose from 6440 to 8980 kB
in that single probe. See `timed-pcm-verification.log`. This cold-start
cost runs synchronously in the live owner and can exceed one tick. The
capture measured a short fixture on this host and does not measure
worst-case startup or a full catalog. The captured native logs passed the
Vulkan scanner.

`RUST_TEST_THREADS=1 python3 scripts/check.py --base f731b6b` passed
Markdown, formatting, strict workspace Clippy, 35 Python tests and 440
Rust tests with 20 explicit manual probes ignored. Total 119.89 seconds
including compilation. The final editor wording/type-assertion follow-up
passed its focused test and strict Clippy. An independent review examined
commit and generation isolation, silence paths and package dependency
closure. See the [architecture decision](../decisions/2026-11-16-packaged-audio-feedback.md).
