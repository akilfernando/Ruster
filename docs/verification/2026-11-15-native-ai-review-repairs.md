# Native AI workspace review repairs — 2026-11-15

Bounded repair of the three independent review findings against `2670a4f`.
Implementation and headless seam verification only; parent owns review readback,
integration and subsequent guarded native/client qualification. No installation,
GUI, GPU, live enrollment, client settings or frozen-candidate changes.

## Repaired behavior

- An enrolled bind refused after early owner-loop admission returns
  `trusted_connection_unavailable`, not legacy approval. A test-only two-channel
  barrier deterministically revokes after frame admission and before `authorized()`;
  it does not bypass the enrollment lock. The client also retires on an older
  native owner's `approval_required` bind response when enrolled proof is present.
  Untrusted legacy approval and enrolled happy-path behavior remain covered.
- Provisioning validates the production registry's required routing alternatives,
  identifiers, name ambiguity, nested fields/types, strict duplicate keys and UTF-8
  byte limits before creating output. Generated files also respect the production
  readers' size bounds. Malformed input leaves no output directory or successful
  command receipt. A Rust test runs the provisioner and reads both locator and
  startup registrations through `Registry::load`, and the allowed grant through
  `Grant::load`, without launching native. Routing fixtures are not live owners.
- Native-only `LocalPolicy` can revoke an existing registered project even when
  its work grant is revoked or the current document differs. The UI identifies
  the actual registered ID and saved path. Real egui button input first revokes
  one project, proves the all-project control remains, changes documents, and
  revokes the remaining project. Grant load/commit and connection still refuse;
  snapshots remain unchanged. No administrative MCP tool is introduced.

The existing retained-inode exclusive revoke lock, shared publication lock,
generation checks and commit/publication ordering are preserved. No retries,
mutation replay, additional Session, aliases/epoch changes, or Save/export/browser
permission changes are introduced. Adaptive workspace geometry is untouched.

## Executed evidence

Private audit root: `native-ai-workspace/review-repairs/` in the existing cumulative
audit. `verification.json` records final commands, exit codes and elapsed times;
`manifest.json` pins changed source, test binaries, adapter and logs.

- RED: `p1-bridge-red.log`, `p1-client-red.log`, `p2-red.log`,
  `p2-output-bound-red.log`, `p2-utf8-red.log`, `p3-red.log`,
  `p3-same-document-red.log`. The latter isolates the disappearing all-project
  control after a successful narrow revoke; the earlier UI failure covers a
  replacement document. All fail for the intended behavioral assertion.
- GREEN: bridge 16 passed/1 existing ignored; connection seams 18 passed/1 existing
  ignored; MCP 27 unit plus 1 startup-provenance integration test passed;
  provisioning 2 tests passed normally and with `PYTHONOPTIMIZE=1`.
- Existing real STDIO/windowless production-Host tracer passed cold attach,
  edit/Undo, Save/reopen, reconnect, project switch, stale refusal and cross-owner
  revoke. This is not graphical startup, human native interaction or actual
  ChatGPT/model image reception.
- Inspection feature: enrolled bridge 3 passed; native controls 5 passed.
  Affected default and inspection strict Clippy plus formatting passed.

The generic runner dry plan conservatively selects the whole workspace for shared
support/tooling paths. Per the bounded assignment, semantic consumers were traced
and affected checks run explicitly instead; no aggregate whole-suite pass is
claimed. Prior optional-browser long-path fixture failures and the unresolved
inspection asset-retirement failure remain historical holds, not repaired or
retroactively greened by these results.
