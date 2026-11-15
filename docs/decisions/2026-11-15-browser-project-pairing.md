# Browser/project pairing for prompt-driven connection

Date: 2026-11-15.
Status: user-approved interaction and authority model; implementation pending.

Akil explicitly selected **Use one-time browser/project approval, then automatic connection** in the Telegram clarification. The question specified approval of the pending browser for the selected project, automatic authentication while that grant is valid, no capability copying or secret-bearing URLs, and no first-visitor auto-trust.

## Required outcome

A prompt selects an exact registered project; ChatGPT opens its existing internal browser (already user-confirmed). The workspace presents a pending pairing request, Rusterize exposes a meaningful native approval of that exact browser/project, and approval allows automatic authenticated load/reload. Subsequent direct MCP authoring and browser controls must share the same native project/session/history. Browser credentials must not pass through model-visible text, URLs or manual clipboard steps. Reuse the existing native consent, HTTP workspace and persistent bridge; no replacement renderer, history or question UI.

Project registration/routing is not consent. A browser cannot approve itself; a model-callable MCP operation is not a substitute for the approved human action. The first visitor must remain unauthenticated until approval. Wrong-project, wrong-origin, stale-incarnation, denied, expired and revoked requests must fail closed without replay. Retain explicit Host/Origin/CSRF protections and exact native-owner binding.

The user approved authentication **while the grant is valid**, not permanent browser trust, transparent permission expansion or silent migration across changed native identities. Declare the grant lifetime and disconnect/revocation behavior in the implementation; use conservative session/identity-bound validity unless a broader persistence policy is separately accepted. Normal page reload within a valid grant must not require another secret or Connect click. Any initial native/MCP project-access approval remains meaningful and separate unless safely combined in the same explicit native consent view.

## First acceptance slice

Use one explicitly registered disposable saved project, existing real native owner/adapter and the existing browser workspace. Demonstrate pending refusal → exact native approval (fixture approval labeled in automated evidence) → automatic browser connection → actual MCP edit visible in that same session → reload without credential entry → revocation/disconnect refusal, plus wrong-origin/project/expired negative checks at their owning layers. Actual human native approval and actual one-prompt ChatGPT acceptance remain distinct from owned automated fixtures.

The prior exact-project adapter result at `1a1595a` is supporting evidence, not complete onboarding. Startup, registry refresh and new native incarnation handling remain explicit unfinished work. No installation, real ChatGPT configuration change, canonical merge or release is approved by this decision.
