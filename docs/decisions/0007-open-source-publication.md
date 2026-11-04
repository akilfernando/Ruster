# 0007 Public source hosting and licensing

Date: 2026-11-04. Status: implemented working choice under the user's explicit request to host Ruster on their personal GitHub and make it fully open source. This supersedes the previously unresolved release-license choice; it does not declare an alpha/binary release or later milestones complete.

## Decision and scope

Host the complete repository/source history at `https://github.com/akilfernando/Ruster`, public, owned by the authenticated personal account. GitHub connector and CLI identity agree. The connector lacks repository creation/local Git push, so the authenticated CLI handles creation and push. No organization, proprietary engine component, account-gated code or mandatory hosted service is introduced.

License original project source, documentation and examples under `MIT OR Apache-2.0`, at the recipient's option. This dual permissive license is an agent-selected implementation of the user's open-source requirement, not an independently stated user preference for a particular license. Include unmodified Apache2 terms, MIT terms with Ruster contributors copyright, matching Cargo workspace/package metadata and contribution terms. Keep publish=false to prevent accidental crates.io uploads while APIs evolve; that flag does not restrict source-license rights.

Alternatives: MIT-only is simpler; Apache-only provides a single explicit patent-grant policy; copyleft would impose different downstream obligations not requested by the user. The dual choice fits Rust consumption and permits either license while preserving third-party obligations. Do not claim that every dependency is dual-licensed: fonts, Unicode data and other dependencies retain their declared licenses/notices. No contributor license agreement or ownership transfer is introduced.

## Publication and maintenance

Rewrite README around current M5 capabilities, honest limits, clean-checkout commands, real screenshot, optional MCP, legacy logistics example and navigable docs. Add contribution/security guidance and a decision index. The user authorized publication; no extra approval flow is inferred. Public source hosting is separate from supported binary packages or alpha release readiness.

Add a read-only Linux GitHub Actions gate using the existing change-aware runner: actual PR base SHA, previous push SHA, full scope for initial push/manual runs. Pin actions to verified commit SHAs, use no repository secrets, disable persisted checkout credentials and retain bounded job time/cache. GPU/native/platform evidence remains separate. No branch-protection enforcement or merge queue is claimed.

A read-only audit found no credential/private-key patterns in current text or existing four-commit history, no private/git dependency sources, and only Ruster UI in screenshots without embedded text/EXIF data. This is pattern/provenance evidence, not a guarantee that arbitrary future content is safe. Historical evidence retains non-secret workstation paths/hardware metadata; preserve history rather than rewriting it without a user request. Remove avoidable identifying fields in current evidence where practical while retaining measurement conditions.

## Sources and evidence

Accessed 2026-11-04: [OSI MIT](https://opensource.org/license/mit), [OSI Apache2](https://opensource.org/license/apache-2.0), [canonical Apache license](https://www.apache.org/licenses/LICENSE-2.0.txt), [GitHub repository creation](https://docs.github.com/en/repositories/creating-and-managing-repositories/creating-a-new-repository), and official [checkout](https://github.com/actions/checkout)/[cache](https://github.com/actions/cache) sources. Canonical license files and actual Cargo dependency inventory are the inspectable implementation; legal templates are not a universal compliance certification.

Final local clean-checkout, remote visibility/default-branch/content checks and actual hosted CI result belong in STATUS.md. Do not call configured CI a passing CI run before observing execution.
