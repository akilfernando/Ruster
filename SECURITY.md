# Security policy

Ruster is experimental software. Security fixes are currently developed on the default `main` branch; no stable release or long-term maintenance branch is promised.

Report sensitive vulnerabilities through the repository's **Security → Report a vulnerability** flow at [akilfernando/Ruster](https://github.com/akilfernando/Ruster/security). Avoid public issues containing exploit details, credentials or private project files. Include the affected commit, platform, minimal reproduction and expected impact. There is no guaranteed response time or paid support commitment.

The optional MCP bridge trusts processes running as the same user and uses a private local Unix socket; session tokens are concurrency checks, not authentication secrets. It does not provide sandboxing against hostile same-user processes. Use trusted clients and keep test inspection loopback-only. See the [adapter guide](crates/ruster-mcp/README.md) for the exact boundary.
