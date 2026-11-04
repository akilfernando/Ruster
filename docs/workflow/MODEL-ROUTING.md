# Model routing

Checked 2026-11-04 for the current root session. This is a replaceable snapshot, not a permanent model pin. The policy lives in [the skill](../../skills/ruster-development/SKILL.md).

## Current routing

| Role | Available model | Effort guidance |
| --- | --- | --- |
| Main developer | `gpt-6.1-sol` | Client-supported default; adjust to task difficulty |
| Deep reasoning | `gpt-6-astra` | Official starting point low; increase for demanding synthesis/review |
| Economical worker | `gpt-6-luna` | Official starting point high; adjust to bounded task |
| Visual and computer use | `gpt-6.1-terra` | Medium starting candidate; confirm tool availability |

Evidence: active `collaboration.spawn_agent` schema exposes these four models plus `gpt-6-sol`. Supported efforts are low, medium, high, xhigh, max and ultra, except Luna has no ultra. The current host has four total agent slots, including the root. Explicit model/effort overrides require `fork_turns="none"`. These host facts must be rediscovered in subsequent sessions.

Official sources fetched 2026-11-04:

- [Model selection for this client](https://learn.chatgpt.com/docs/models).
- [Subagent configuration and effort guidance](https://learn.chatgpt.com/docs/agent-configuration/subagents).
- [Model capability guidance](https://developers.openai.com/api/docs/guides/latest-model).

The local CLI model cache was dated 2026-11-04 and desktop cache 2026-11-03; both were supporting evidence only. They expose additional entries not offered by this session's delegation tool, which were excluded. No account identifiers were persisted.

Relative role guidance is supported by official documentation. Actual subscription cost, task latency and quality improvements are unmeasured. No numerical pricing assumptions are used. The current root model was not independently verified or switched; this is the recommended selection, not proof of the active root model.

## Activation and portability

The canonical skill is versioned at `skills/ruster-development/`. On this workstation it is installed through a symlink at `/home/akilf/.codex/skills/ruster-development`. Repository AGENTS.md also instructs sessions to read the canonical skill, so project continuity does not depend solely on skill-catalog refresh. On another workstation, install/link that directory into its configured Codex skills directory if catalog discovery is desired.

Refresh is an agent instruction executed when a root session follows AGENTS.md/the skill, not a background hook or guaranteed pre-session launcher. No global model configuration is changed. Select the recommended balanced model in the client for the main session; the skill selects child models dynamically. If a future host lacks selectable subagents, continue with available capabilities and report that limitation.
