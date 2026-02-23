# Microstable Security & Bug Bounty Program

Thank you for helping secure Microstable.

Microstable accepts responsible vulnerability disclosures for:

- On-chain program (`solana/programs/microstable/src/lib.rs`)
- Keeper daemon (`solana/keeper/src/*.rs`)

---

## 1) Scope

### In Scope

1. **On-chain protocol logic**
   - Access control, signer/quorum checks, PDA/account validation
   - Mint/redeem accounting, fee logic, slashing/governance controls
   - Oracle integration and commit-reveal integrity

2. **Keeper daemon**
   - Oracle/monitor/rebalance/watchdog execution paths
   - Cross-RPC safety handling
   - Key management, configuration validation, supply-chain/runtime hardening

### Out of Scope

- Frontend/UI/UX issues (`docs/index.html` visual bugs, style/layout defects)
- Devnet-only environmental instability without protocol-level vulnerability
- Known limitation: **Pyth devnet staleness/noise characteristics** without exploitable protocol bypass
- Social engineering, phishing, physical attacks, and third-party account compromise unrelated to code defects
- Denial-of-service caused solely by generic public RPC outages with no protocol-specific exploit

---

## 2) Severity Tiers & Reward Ranges

Final reward amounts are determined by impact, exploitability, report quality, and fix complexity.

| Severity | Example Impact | Reward |
|---|---|---:|
| **Critical** | Direct fund loss, protocol drain, privileged takeover | Up to **$50,000** |
| **High** | Economic manipulation, oracle bypass, severe liveness/control break | Up to **$10,000** |
| **Medium** | Griefing, meaningful DoS, partial integrity break | Up to **$2,000** |
| **Low** | Informational or low-impact hardening issue | Acknowledgment + Hall of Fame |

Notes:
- Duplicate reports are rewarded at maintainers’ discretion (typically first valid report wins).
- Reports must include reproducible steps or a clear technical proof.

---

## 3) Disclosure Rules

1. **Responsible Disclosure**
   - Do not publicly disclose vulnerabilities before a fix is available and coordinated.

2. **No Harm Policy**
   - Do not access, modify, or destroy user funds/data.
   - Use minimal-impact testing and stop once vulnerability is demonstrated.

3. **72-hour Response SLA**
   - Microstable will acknowledge valid submissions within **72 hours**.

4. **Safe Harbor**
   - Good-faith research conducted within this policy’s scope is authorized.
   - Microstable will not pursue legal action for compliant, non-destructive testing.

---

## 4) How to Submit

Please use one of the following:

1. **GitHub Security Advisory (preferred, private):**
   - `Security` tab → `Report a vulnerability`

2. **Email (private):**
   - `security@microstable.dev`

Include:
- Vulnerability title and severity assessment
- Affected component/file/commit
- Reproduction steps and prerequisites
- Impact analysis
- Suggested remediation (optional but appreciated)

---

## 5) Coordinated Disclosure Process

1. Triage and acknowledgment (within 72h)
2. Severity classification and impact validation
3. Patch development and internal verification
4. Coordinated disclosure and reward determination

---

## 6) Hall of Fame

_This section will be updated as valid reports are disclosed._

- (empty)
