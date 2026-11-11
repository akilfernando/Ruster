# Model routing

Checked 2026-11-11 for the current root session. This is a replaceable snapshot, not a permanent model pin. The policy lives in [the skill](../../skills/ruster-development/SKILL.md).

## Current routing

| Role | Available model | Effort guidance |
| --- | --- | --- |
| Main developer | `gpt-6.1-sol` | Project default low; medium/high for justified complexity |
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

## Usage-informed routing revision — 2026-11-11

The user approves updating the skills after reviewing a seven-day snapshot showing 257.31% of weekly allowance, model shares Sol57%/Astra41%/Terra2%/Luna0%, and effort shares high62%/medium38%/light0%. These rounded aggregate shares do not identify task counts, main-agent versus worker usage, quality per task or savings. The delegation log contains useful specialist findings and repeated high-effort reviews, with per-task usage mostly unavailable.

The canonical development skill now starts routine work at Sol low, escalates by task risk, distinguishes deeper coding analysis on Sol high from architectural judgment on Astra low, and consolidates specialist reviews. Luna receives bounded mechanical trials; Terra remains capability-driven. Whole-roadmap review retains the user's Astra preference. The next two resumed bounded slices will assess acceptance, repairs, useful findings and available usage without model-share quotas or a benchmark campaign. Actual savings and Sol-high/Astra-low relative cost remain unmeasured.

Official [Models](https://learn.chatgpt.com/docs/models?surface=app), [Subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents), [Model selection](https://learn.chatgpt.com/docs/model-selection) and [Pricing](https://learn.chatgpt.com/docs/pricing) pages were fetched on2026-11-11. Official guidance starts Sol at the client default with task-specific adjustment; Sol low as the project default is an accepted working choice. Published Standard credits price Astra uncached input/output at5times Sol and cached input at10times; total task usage also depends on context, reasoning and retries. Credit rates do not directly determine included subscription allowance consumption.

The active collaboration schema still exposes the five model IDs listed above, four total slots, and explicit overrides only with minimal-context forks. Sol6.1/Terra6.1 support low through ultra with low defaults in this tool; Astra supports low through ultra with medium default; Luna supports low through max with medium default. Explicit assignments take precedence. No main model, global configuration or engine implementation is changed.

## Activation and portability

The canonical skill is versioned at `skills/ruster-development/`. On this workstation it is installed through a symlink at `/home/akilf/.codex/skills/ruster-development`. Repository AGENTS.md also instructs sessions to read the canonical skill, so project continuity does not depend solely on skill-catalog refresh. On another workstation, install/link that directory into its configured Codex skills directory if catalog discovery is desired.

Refresh is an agent instruction executed when a root session follows AGENTS.md/the skill, not a background hook or guaranteed pre-session launcher. No global model configuration is changed. Select the recommended balanced model in the client for the main session; the skill selects child models dynamically. If a future host lacks selectable subagents, continue with available capabilities and report that limitation.

## Main-model clarification, 2026-11-04

The user reports that the main model is currently Astra and asks whether Sol better realizes the routing policy. Official Models and Subagents pages were rechecked; the active tool still exposes the same role candidates. Recommend GPT-6.1 Sol as the main developer with its client-default effort, escalating effort/model selectively. Astra as parent can still delegate to Sol/Luna/Terra, but parent reasoning/integration remains Astra work; delegation does not replace the parent. Explicit worker model/effort avoids accidental expensive parent inheritance. No main model or global configuration was changed. Actual project cost/latency savings remain unmeasured.

## M4 session refresh — 2026-11-04

Reopened all three official sources above and checked the active collaboration schema. Role assignments and supported effort/inheritance/slot limits remain unchanged. Used Astra at high effort for bounded M4 ownership review because session identity and draft/runtime conflicts are consequential; main implementation remains in the root. No cache inspection, API billing job or global model change. Actual subscription cost and active root model identity remain unmeasured.

## M8 persistence session refresh — 2026-11-04

Reopened the three official sources above at 07:43 UTC and inspected the active collaboration schema. Available role models, efforts, four-slot limit and explicit-override inheritance rules remain unchanged. Sol6.1 remains the recommended main developer; Astra high is assigned the bounded persistence/ownership review. Child workers reuse this snapshot. No global settings changed; actual subscription usage/cost remains unknown.
