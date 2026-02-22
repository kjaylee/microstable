# Red Team v3 PoC Execution Results

- Total attempts: **36**
- Successful exploits: **16**
- Blocked attempts: **20**
- Success rate: **44.44%**

| ID | Outcome | Severity | Target | Bypass | Evidence |
|---|---|---|---|---|---|
| A01 | SUCCESS | CRITICAL | OAE staking | PT-001 | `claim_ok=True, ghost_balance=900.0` |
| A02 | SUCCESS | CRITICAL | OAE staking | PT-001 | `nan_claim=True, post_nan_large_claim=True, epoch_used=nan` |
| A03 | SUCCESS | CRITICAL | OAE staking | - | `before=100.0, slash_amt=-100.0, after=200.0` |
| A04 | BLOCKED | MEDIUM | OAE staking | PT-002 | `request_ok=True, withdrawn=10.0` |
| A05 | BLOCKED | MEDIUM | Tournament commit/reveal | PT-003 | `commit=True, reveal1=True, reveal2=False` |
| A06 | BLOCKED | MEDIUM | Tournament submission | PT-004 | `direct_submit=False` |
| A07 | BLOCKED | MEDIUM | Tournament scoring | PT-005 | `score=10.0` |
| A08 | SUCCESS | HIGH | Tournament evaluate | PT-006 | `winner=evil, mint_fee=nan, weights=[nan, 0.3, 0.3, 0.4]` |
| A09 | BLOCKED | MEDIUM | Tournament anti-sybil | PT-007 | `first=True, second=False` |
| A10 | SUCCESS | HIGH | Tournament economics | PT-007 | `sybil_rewards=14.7619, honest_rewards=30.2381` |
| A11 | BLOCKED | MEDIUM | Watchdog settlement | PT-008 | `balance_first=6.0, balance_second=6.0` |
| A12 | SUCCESS | HIGH | Watchdog settlement | PT-008 | `consensus=False, before=5.0, after=6.0` |
| A13 | BLOCKED | MEDIUM | Watchdog evidence validation | PT-009 | `report_ok=False` |
| A14 | SUCCESS | MEDIUM | Watchdog evidence validation | PT-009 | `ValueError:cannot convert float NaN to integer` |
| A15 | BLOCKED | MEDIUM | Watchdog bounty ordering | PT-010 | `zzz=6.0, aaa=5.0` |
| A16 | SUCCESS | HIGH | ACP auth | PT-011 | `verify1=True, verify2=True` |
| A17 | SUCCESS | CRITICAL | ACP auth / registry | PT-012 | `impersonation_verify=True` |
| A18 | BLOCKED | MEDIUM | Rate limiting | PT-013 | `allow=[True,True,False]` |
| A19 | BLOCKED | MEDIUM | Redemption queue | PT-014 | `victim_units=1333329, attacker_units=0` |
| A20 | SUCCESS | MEDIUM | Redemption queue | PT-014 | `ValueError:cannot convert float NaN to integer` |
| A21 | BLOCKED | MEDIUM | Redemption queue | PT-015 | `u3=1, treasury_residual=2` |
| A22 | BLOCKED | MEDIUM | Circuit-breaker policy | PT-016 | `pt16_markers_present=True` |
| A23 | BLOCKED | MEDIUM | Tournament commit | PT-017 | `commit1=True, commit2=False` |
| A24 | SUCCESS | HIGH | Keeper rebalance guard | PT-018 | `result={'status': 'APPLIED', 'weights': [0.46, 0.24, 0.2, 0.1], 'mint_fee': 0.002}, turnover_window=[0.039999999999999925, 0.040000000000000036, 0.040000000000000036]` |
| A25 | SUCCESS | MEDIUM | Mint risk gate | PT-019 | `minted=767691` |
| A26 | BLOCKED | MEDIUM | Oracle degradation scope | PT-020 | `enabled_assets=[True, False, True, True]` |
| A27 | BLOCKED | MEDIUM | Adversarial executor | PT-021 | `r1=failed, r2=failed` |
| A28 | BLOCKED | MEDIUM | Adversarial executor | PT-022 | `sig_a=b493e196a13d76fa, sig_b=53aada4958229af5` |
| A29 | BLOCKED | MEDIUM | Anomaly detector | PT-023 | `clusters=[{'type': 'collusion', 'agents': ['a', 'b'], 'similarity': 1.0, 'epoch': 1}]` |
| A30 | BLOCKED | MEDIUM | Response engine | PT-024 | `first=rate_limit, second=noop` |
| A31 | SUCCESS | MEDIUM | Response safety gate | PT-025 | `result={'safe_mode': False, 'registration_frozen': False, 'rate_limit_enabled': False}` |
| A32 | SUCCESS | MEDIUM | Forensics ↔ executor blocklist | PT-026 | `status=partial_success, success=True, blocked_entry=uppercase` |
| A33 | SUCCESS | MEDIUM | Forensic signature robustness | PT-027 | `sig_a=82d26edc9519934771eb, sig_b=82d26edc9519934771eb` |
| A34 | SUCCESS | MEDIUM | Solana on-chain oracle path | - | `has_quorum_call=False, has_keeper_signers=False` |
| A35 | BLOCKED | HIGH | Solana migration | - | `trusted_initializer_guard=True` |
| A36 | BLOCKED | HIGH | Solana mint CPI path | - | `guard_checks_present=[True, True, True, True]` |
