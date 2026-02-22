# Red Team v3 PoC Execution Results

- Total attempts: **36**
- Successful exploits: **0**
- Blocked attempts: **36**
- Success rate: **0.00%**

| ID | Outcome | Severity | Target | Bypass | Evidence |
|---|---|---|---|---|---|
| A01 | BLOCKED | CRITICAL | OAE staking | PT-001 | `claim_ok=False, ghost_balance=0.0` |
| A02 | BLOCKED | CRITICAL | OAE staking | PT-001 | `nan_claim=False, post_nan_large_claim=False, epoch_used=None` |
| A03 | BLOCKED | CRITICAL | OAE staking | - | `before=100.0, slash_amt=0.0, after=100.0` |
| A04 | BLOCKED | MEDIUM | OAE staking | PT-002 | `request_ok=True, withdrawn=10.0` |
| A05 | BLOCKED | MEDIUM | Tournament commit/reveal | PT-003 | `commit=True, reveal1=True, reveal2=False` |
| A06 | BLOCKED | MEDIUM | Tournament submission | PT-004 | `direct_submit=False` |
| A07 | BLOCKED | MEDIUM | Tournament scoring | PT-005 | `score=10.0` |
| A08 | BLOCKED | HIGH | Tournament evaluate | PT-006 | `winner=None, mint_fee=0.002, weights=[0.25, 0.25, 0.25, 0.25]` |
| A09 | BLOCKED | MEDIUM | Tournament anti-sybil | PT-007 | `first=True, second=False` |
| A10 | BLOCKED | HIGH | Tournament economics | PT-007 | `sybil_rewards=2.5000, honest_rewards=32.5000` |
| A11 | BLOCKED | MEDIUM | Watchdog settlement | PT-008 | `balance_first=6.0, balance_second=6.0` |
| A12 | BLOCKED | HIGH | Watchdog settlement | PT-008 | `consensus=False, before=5.0, after=5.0` |
| A13 | BLOCKED | MEDIUM | Watchdog evidence validation | PT-009 | `report_ok=False` |
| A14 | BLOCKED | MEDIUM | Watchdog evidence validation | PT-009 | `no_exception` |
| A15 | BLOCKED | MEDIUM | Watchdog bounty ordering | PT-010 | `zzz=6.0, aaa=5.0` |
| A16 | BLOCKED | HIGH | ACP auth | PT-011 | `verify1=False, verify2=False` |
| A17 | BLOCKED | CRITICAL | ACP auth / registry | PT-012 | `impersonation_verify=False` |
| A18 | BLOCKED | MEDIUM | Rate limiting | PT-013 | `allow=[True,True,False]` |
| A19 | BLOCKED | MEDIUM | Redemption queue | PT-014 | `victim_units=1333329, attacker_units=0` |
| A20 | BLOCKED | MEDIUM | Redemption queue | PT-014 | `no_exception` |
| A21 | BLOCKED | MEDIUM | Redemption queue | PT-015 | `u3=1, treasury_residual=2` |
| A22 | BLOCKED | MEDIUM | Circuit-breaker policy | PT-016 | `pt16_markers_present=True` |
| A23 | BLOCKED | MEDIUM | Tournament commit | PT-017 | `commit1=True, commit2=False` |
| A24 | BLOCKED | HIGH | Keeper rebalance guard | PT-018 | `result={'status': 'REJECTED', 'reason': 'delta_violation_0'}, turnover_window=[0.039999999999999925]` |
| A25 | BLOCKED | MEDIUM | Mint risk gate | PT-019 | `minted=0` |
| A26 | BLOCKED | MEDIUM | Oracle degradation scope | PT-020 | `enabled_assets=[True, False, True, True]` |
| A27 | BLOCKED | MEDIUM | Adversarial executor | PT-021 | `r1=failed, r2=failed` |
| A28 | BLOCKED | MEDIUM | Adversarial executor | PT-022 | `sig_a=9b57980ca3349cc0, sig_b=27c96020eaa02134` |
| A29 | BLOCKED | MEDIUM | Anomaly detector | PT-023 | `clusters=[{'type': 'collusion', 'agents': ['a', 'b'], 'similarity': 1.0, 'epoch': 1}]` |
| A30 | BLOCKED | MEDIUM | Response engine | PT-024 | `first=rate_limit, second=noop` |
| A31 | BLOCKED | MEDIUM | Response safety gate | PT-025 | `result={'safe_mode': True, 'registration_frozen': False, 'rate_limit_enabled': False}` |
| A32 | BLOCKED | MEDIUM | Forensics ↔ executor blocklist | PT-026 | `status=blocked, success=False, blocked_entry=uppercase` |
| A33 | BLOCKED | MEDIUM | Forensic signature robustness | PT-027 | `sig_a=9e7f19829b1ed23bb183, sig_b=714f3ae6cb036a5a81c0` |
| A34 | BLOCKED | MEDIUM | Solana on-chain oracle path | - | `has_quorum_call=True, has_keeper_signers=False` |
| A35 | BLOCKED | HIGH | Solana migration | - | `trusted_initializer_guard=True` |
| A36 | BLOCKED | HIGH | Solana mint CPI path | - | `guard_checks_present=[True, True, True, True]` |
