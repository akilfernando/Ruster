# Packaged one-shot audio feedback

Active 2026-11-16 from cumulative `f731b6b`. Astra independently selected
audio after checking the roadmap and current asset, behavior and runtime
owners. Existing input and typed state already express the cue trigger.
Imported animation and reusable composition remain candidates for
subsequent goals. Root owns integration and assessment.

## Creator outcome

Import a short WAV in the Content browser. Assign it to a named Sound
property on a behavior and trigger it from a saved keyboard action through
Lua. Hear one cue in ordinary preview and in a relocated Linux app.
Author and repair an independent case through guarded native MCP. Save
and reopen both and verify the same resource identity.

The isolated branch is `feat/packaged-audio-20261116`. The first gate is a
real fixed-step Lua request delivered only after commit to an isolated
Linux output, with a known waveform and silence on an injected late
binding failure. Reassess after 30 minutes without this tracer or after
two failed end-to-end attempts. Probe outside the main project and use a
disposable audio sink without changing the default output.

## Implementation scope

- Admit a small PCM16 WAV through the project asset catalog. Validate
  encoded bytes, sample rate, duration and decoded size before opening
  output. Retain exact bytes and resource identity through export. Bound
  per-cue assets, aggregate decoded memory and active voices.
- Use a typed Sound reference and linear gain as a named property of the
  requesting binding. Lua4/5/7 bindings with sound properties can call
  `host.play_sound(property_name)`. The binding holds the stable asset
  ID. Audio reads only the successful fixed-step receipt. Reset trials
  discard staged requests. Commands, paused preview Steps and isolated
  checks produce no device output. Keep existing kinematic behaviors
  usable with an added sound property.
- Reset, Stop, invalidation and close clear the output lifetime. Pause
  freezes voices and resume continues them. Device failure surfaces
  bounded status while simulation remains usable. Late failed/cancelled
  Step must never emit staged output. Queue and voice exhaustion have
  explicit bounded behavior. No device work runs in Lua.
- Ordinary authoring includes import, assign, repair, Undo and Save/open.
  Guarded MCP reuses existing catalog/property operations as far as they
  cover the authoring. Runtime cue receipts are distinct from device
  confirmation. The matched Linux kit verifies capability and preserves
  exact scoped resources. A no-feature package keeps its existing
  runtime and compatibility behavior.
- Record production Lua, loopback, lifecycle, native editor/MCP and
  relocated evidence. Measure clear no-audio Step overhead, cue Step,
  bounded decoded memory and teardown. Use app-owned windows and sink
  only for automated checks while away.

Non-goals are spatial audio, streaming, looping, microphones, music
timelines, audio buses, general events and cross-platform device claims.
The starting product scope and limits are agent-selected.

## Design references

Accessed 2026-11-16. [Godot AudioStreamPlayer](https://docs.godotengine.org/en/stable/classes/class_audiostreamplayer.html)
separates non-positional playback from spatial nodes. [Unity PlayOneShot](https://docs.unity3d.com/ScriptReference/AudioSource.PlayOneShot.html)
supports a discrete clip triggered by script. Evaluate
[Kira static sounds](https://docs.rs/kira/latest/kira/sound/static_sound/),
[Rodio](https://docs.rs/rodio/latest/rodio/index.html) and
[CPAL](https://github.com/RustAudio/cpal) based on the executable
wiring and bounded lifecycle. These facilities inform the first scope.
