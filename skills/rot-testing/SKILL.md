---
name: rot-testing
description: Verification for Rot Engine changes. Points to docs/PLAYBOOK.md and docs/TESTING.md. Applies to Rot Engine code, tooling and documentation changes.
---
# Rot Engine testing

This skill is a pointer. Verification is described in the repository:

1. `docs/PLAYBOOK.md`, section "What to verify, and with what".
2. `docs/TESTING.md` for the detail of the gate and risk-specific checks.

Write the few tests that protect behavior a creator depends on, run
`python3 scripts/check.py` before merging, and look at the editor with
`scripts/ui_drive.py` for any UI change.
