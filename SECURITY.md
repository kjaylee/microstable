# Security Policy

## Scope

microstable is currently in **Phase 1 (simulation only)**. No real funds are at risk.

## Reporting

If you discover a security vulnerability, please:

1. **Do NOT** open a public issue
2. Email the maintainers or use GitHub's private vulnerability reporting
3. Include: description, reproduction steps, potential impact

## What We Consider Security Issues

- Logic errors in gradient computation that could be exploited
- Circuit breaker bypass scenarios
- Oracle manipulation vectors not covered by existing mitigations
- Agent consensus mechanism vulnerabilities
- Wallet/key exposure in committed code

## What We Do NOT Consider Security Issues (Phase 1)

- Simulation performance issues
- Cosmetic bugs in output formatting
- Feature requests

## Disclosure Policy

- We aim to acknowledge reports within 48 hours
- Fixes will be committed with appropriate credit (unless anonymity is requested)
- Critical issues will be disclosed after a fix is available

## Automated Security

This repository is monitored by AI agents that perform:
- Daily code scanning for exposed secrets/keys
- Invariant verification on all commits
- Dependency vulnerability checks
