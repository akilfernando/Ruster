# Packaged one-shot audio feedback

Date: 2026-11-16
Status: Adopted for the bounded Linux creator slice.

An authored interaction needs an audible response. Existing input and state
already express an edge. A typed Sound property uses the owning behavior
binding, familiar property editing and guarded command history for its name,
stable WAV asset ID and linear gain. Existing Lua APIs 4, 5 and 7 can call
`host.play_sound(property_name)` from an update with a Sound property.
Authoring may retain an unassigned reference for repair. A request for an
unknown or unassigned sound fails the transaction with its cue context.

A behavior can stage up to 16 requests in one fixed Step. Engine publishes
the bounded batch only after the entire Step commits, with tick and commit
revision. The editor worker and packaged host convey committed live batches
once within their playback lifetime. Reset trials, paused Steps, commands,
debugger-armed editor runs and isolated checks use silent paths. Failed
Steps keep preceding committed simulation data and retire live audio.
Pause uses a short Kira fade and retains voice position. Reset, Stop, stale
editor epochs and shutdown retire the output owner. A nonblocking epoch
guard silences each stale editor callback buffer.

We considered a state observer, a generic event bus, a standalone Lua
audio API generation, and binding-owned typed requests. The last preserves
a clear resource dependency and lets API7 actions use sound without another
behavior version. Hound validates and decodes PCM16. Kira owns bounded
static voices and resampling. CPAL supplies the Linux device stream. A
custom CPAL adapter sets an approximately 10 ms device buffer when the
reported buffer range supports it and avoids automatic sink switching
after a stream error. A production tracer using the default buffer
captured silence for the short scheduled intervals. The smaller buffer
passed the measured route. [Kira static sounds](https://docs.rs/kira/latest/kira/sound/static_sound/),
[Rodio alternatives](https://docs.rs/rodio/latest/rodio/index.html) and
[CPAL](https://github.com/RustAudio/cpal) were reviewed 2026-11-16.

The catalog admits PCM16 mono/stereo 8–48 kHz WAV, capped at 2 MiB encoded
and ten seconds per file, with 64 MiB of aggregate decoded catalog
frames. Playback can retain a second bounded Kira-format copy of used
clips per output lifetime. The cap on concurrent voices is 16. Device
exhaustion drops the new cue with status. An unavailable device leaves
simulation usable. Device confirmation is independent of a successful
simulation receipt.

Sound properties use Scene21 and catalogued WAV sources use Asset
Catalog6. Descriptor6 and `one_shot_audio` are required only when present
in authored bindings or retained history, including unassigned cues.
Verified raw WAV bytes cross the closure and saved-check pins. Earlier
descriptors and no-sound kits retain ordinary compatibility. The output
path qualified float PCM through PipeWire's PulseAudio support. An
integer-only default output may report unavailable.
