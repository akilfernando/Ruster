# Browser pairing final-source qualification — 2026-11-15

## Outcome and boundary

Final-source owned Chromium pairing **passes** at source checkpoint `8cb09df`.
This is explicit disposable-fixture approval, **not native human consent, ordinary
native startup attachment, installation or actual ChatGPT onboarding**. Existing
user-confirmed ChatGPT prompt navigation is not being repeated.

Audit root:
`/home/akilf/Projects/Ruster-Audits/prompt-connect-20261115/browser-pairing/`.
The manifest pins source, compiler messages, artifacts, journeys and cleanup.

The shared QA gate was acquired normally; no lease was stolen or extended. The
first exact-source compilation failed because native.rs used Rust2024 let-chains
in this Rust2021 crate. `build-resume.jsonl` preserves that RED. Equivalent Option
composition repairs only the edition error. `build-resume-fixed.jsonl` records
actual compilation from this checkout. The fix was pushed and the remote SHA
read back before browser qualification.

## Executed checks

- `cargo test --locked -p ruster-editor --features inspection --no-run --message-format=json`:
  fixed compile passes; immutable copy `host-resume` has SHA-256
  `7dda796dcfbb7677692d6e2d8044f6d2fdece5ce3fe5fca6b9b8fc3cb91051fd`.
- Existing native widget consent regression: one selected test passes. Actual egui
  buttons cancel then approve the exact socket owner, approve an exact pending
  browser/project, reject a different project and Disconnect revokes authority.
  Injected app-scoped egui input is not physical human approval.
- Existing exact-context one-shot denial/expiry regression: one selected test
  passes, including expired pending requests and expired grants; no clock/lease
  extension. Logs: `widget-resume.log`, `expiry-resume.log`.
- `affected-resume.log`: ordinary workspace rustfmt; default and inspection
  all-target strict Clippy for ruster-editor; inspection editor suite181passed,
  six explicit existing ignores; four existing Python stream tests pass.
  No broad workspace suite rerun solely due to ownership change. Prior unchanged
  package evidence stands; uncompiled changed native inputs received these gates.
- `cargo build --locked -p ruster-editor -p ruster-mcp --message-format=json`:
  ordinary native/adapter binaries build, with compiler JSON and artifact pins in
  `build-normal-resume.*` and the manifest. No native window was launched.
- Existing `qualify.py --pairing-only`, outputs `resume-journey/` and final
  `resume-final-journey/`: pending refuses
  reads, explicit exact fixture approval, automatic native pixels, real saved
  project_list/project_connect and direct MCP transform, observer sees revision1,
  browser Shared Undo reaches revision2, reload automatically authenticates with
  an empty capability field and HttpOnly cookie, Disconnect refuses refresh.
- Existing complete legacy-capability journey, `resume-legacy-journey/`: passes on
  the same pinned Host. This covers affected legacy-mode observer/poll cleanup,
  camera/picking/drafts/shared history and Host/Origin/capability refusals; it is
  not browser-pairing consent or a replacement for the paired journey.

Pinned rebuilt adapter `adapter-resume` SHA-256:
`e54551c562f852e021806bb4750c309afbe62c3e7b08d6a729ef2949b612646b`.
Initial paired run used the retained earlier build hash
`06c6dcdaeb80652bad7b21acb1b6cf5369616ee1af0c41d69bd8bd143e1d26fe`;
normal build produced a distinct debug artifact, so the final paired journey was
rerun with the immutable rebuilt adapter copy. Legacy journey uses the same
rebuilt adapter bytes. All journeys report full owned resource cleanup. Independent readback checks
exact native/server PID absence, listener refusal and capability/locator/socket
absence, then reacquires the shared gate. No endpoint remains live.

## Ordinary-native attachment: exact source/interface blocker

The approved human interaction is resolved; the blocker is **not** a missing
permission decision about one-time pairing. The following production transport
integration does not exist yet:

| Existing owner/surface | Actual source contract | Missing attachment |
| --- | --- | --- |
| Ordinary scene Host | `scene_editor.rs:5874–5986`: owns SceneEditor, InputGate and optional MCP Bridge; run_session opens a saved Session, renders through native eframe; RUSTER_MCP_ENABLE starts only an unbound MCP listener | No HTTP/workspace owner, local bridge transport, endpoint publication or HTTP lifetime tied to Host |
| Ordinary native consent | `scene_editor/connection.rs:48–81`, `connection/native.rs`: Connect/adapter review and browser approve/deny widgets operate exact Bridge authority | Pending browser injection and browser authorization are still cfg(test); normal transport cannot populate the pending widget |
| Production MCP socket | `bridge.rs:426–518`: UID/PID/binding handshake, exactly one persistent approved adapter owner; every command requires that owner's capability and exact five-field envelope | No independent restricted workspace attach/pair channel. An HTTP process cannot simply open another Client alongside MCP or claim the existing owner; arbitrary browser fields are not admitted |
| Existing workspace stream | `scene_editor.rs:29–30`, `preview_probe.rs`, `preview_probe/stream.rs` | Entire stream dispatch/render loop is test+inspection only; its token/PID/incarnation handshake is created by server.py launching the ignored fixture, not by a normal Host. Moving cfg alone would promote fixture authority and omit the production attachment/lifetime contract |
| Production capture | `scene_editor/capture_commands.rs:118–211` | Guarded two-phase capture start/inspect exists; the fixture's synchronous full lossless-frame/status/gesture response is not an interchangeable production capture API |
| Registry/connect | `ruster-mcp/src/project_tools.rs:10–93`, `main.rs:708–748`: private one-project immutable snapshot, pinned locator/binding, native scene readback | No Host-published endpoint or endpoint-to-attachment proof. Publishing an arbitrary registry URL would assert an authenticated workspace that does not exist |

Consequently no URL was invented and `project_connect` retains its conservative
`approval_channel_required` diagnostic. That diagnostic is now historically
imprecise: the native approval interaction exists, but ordinary HTTP delivery to
it does not. It must not be changed to ready merely because the fixture passes.

Smallest next coupled implementation must explicitly define a **restricted
workspace attachment** to the same live Host without becoming another MCP owner:
exact project/session/incarnation plus native/HTTP peer identity, no scene access
before native browser approval, no approval command, bounded single-flight IO,
Host-close/Disconnect revocation and no reconnect/replay. Reuse the current
server/stream/native consent and production capture rather than a new renderer,
history, daemon or broad project manager. Publish the nonsecret workspace URL
only from that established native-owned attachment, and validate the same binding
when project_connect returns it. One saved registered project remains sufficient.
This is a consequential transport/authority extension, not a startup flag or
registry-field edit. Per the bounded handoff's stop condition, implementation is
**BLOCKED at this interface; stopped at a compiled, pushed checkpoint** rather
than silently introducing a second owner protocol. Parent protocol/integration
continuation is QUEUED; no background implementation is running.

## Reproduction and remaining user-flow steps

Developer-only paired journey (acquires the existing QA gate itself):

```sh
python3 spikes/003-interactive-localhost/qualify.py \
  --host-binary /home/akilf/Projects/Ruster-Audits/prompt-connect-20261115/browser-pairing/host-resume \
  --adapter /home/akilf/Projects/Ruster-Audits/prompt-connect-20261115/browser-pairing/adapter-resume \
  --output /home/akilf/Projects/Ruster-Audits/prompt-connect-20261115/browser-pairing/new-owned-run \
  --pairing-only
```

Normal existing startup is `RUSTER_MCP_ENABLE=1 target/debug/ruster-editor --scene
/path/to/saved.scene.json`, followed by explicit native adapter review. It does
**not** start HTTP or produce a workspace URL. No ordinary authenticated browser
startup command is available yet. Never give the fixture launcher/its assumed
first-owner grant as terminal-free end-user onboarding.

Intended remaining actual-client exit: installed connection/access setup → prompt
naming exact registered project → attach/start its normal native owner → open its
nonsecret URL inside ChatGPT → compare code and human-approve that pending
browser/project once → automatic native workspace → direct MCP edit reflected in
that same workspace → reload without credential entry → Disconnect refusal.
Actual ChatGPT settings/install, physical native approval, normal HTTP attachment,
URL handoff/refresh and that complete client journey remain unperformed.
