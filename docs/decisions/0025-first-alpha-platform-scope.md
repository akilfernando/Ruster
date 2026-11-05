# 0025 — First alpha platform scope

Status: accepted agent-selected release-scope decision,2026-11-05. This is not a user-selected operating-system preference or alpha acceptance.

## Context

The user authorizes autonomous through-M9 development and short-term adjustments serving creator adoption, while requiring lean measured code and honest verification. Windows is an intended target in DIRECTION; ROADMAP explicitly permits revising the first release scope when a target cannot be verified. Current editor/MCP attachment and application packaging have Linux-specific delivery boundaries. No native Windows execution environment or participant is established. Wine or a cross-build would not prove the required native creator workflow.

Exact-source full checks and an Ubuntu-linked release now pass in a private Ubuntu24.04.5 userspace. Bounded Intel/Mesa25.2.8 startup/resize/shutdown passes there. CachyOS NV/Intel rendering and validation evidence is separate. This shares one physical workstation; it does not establish all Linux distributions, another machine or complete Ubuntu creator-workflow acceptance.

## Decision

Target the first source alpha candidate at **Linux x86_64**, with separately reported named configuration/evidence rows. Retain Windows as intended future product scope. Do not claim a supported release or completed M9 until the remaining candidate gates pass. Keep package version0.1.0 separate from an eventual local evaluation-candidate identifier; candidate preparation, acceptance and publication are distinct actions.

This resolves the platform-scope decision only. Full creation/recovery/distribution/MCP adoption, experienced Unity/Unreal familiarity, measured resource/feedback budgets, another-machine evidence and exact candidate guidance remain open. The unavailable Unreal participant is not a waiver. No public push/tag/release or stable API/support promise is authorized by this record.

## Alternatives and consequences

Keeping native Windows as a blocker for this first alpha would require an unavailable test environment plus actual transport/packaging work before the Linux adoption path can be evaluated. Claiming Windows from Wine/cross-compilation or claiming broad Linux portability from one userspace would overstate evidence. A scoped source candidate permits useful adoption testing while clearly exposing the remaining work.

Revisit Windows scope when a native test environment is available: verify compiler/dependencies, editor/keyboard/recovery/runtime, MCP transport and independent application packaging on recorded configurations. Revisit Linux matrix rows as real machines/workflows are tested. A later decision may supersede this scope without removing historical proof boundaries.

## Evidence

[Candidate gates](../ALPHA-CANDIDATE.md), [Ubuntu evaluation](../evidence/M9-ubuntu-userspace.md), [named device checks](../evidence/M9-responsive-editor.md) and [resource evidence](../evidence/M9-editor-resources.md). Independent Astra high read-only review recommended this explicitly scoped candidate decision; root retains integration responsibility. No code or dependencies changed.
