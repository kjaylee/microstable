# Red Team v4 PoC Execution Results

- Total attempts: **24**
- Successful exploits: **13**
- Blocked attempts: **11**
- Success rate: **54.17%**

| ID | Outcome | Severity | Defense Focus | Target | Evidence |
|---|---|---|---|---|---|
| A01 | SUCCESS | MEDIUM | Pyth staleness boundary | Solana oracle update | `max_age=60, guard='age<=max' (age==60 accepted)` |
| A02 | BLOCKED | MEDIUM | Pyth staleness boundary | Solana oracle update | `age_guard_present=True` |
| A03 | BLOCKED | HIGH | Feed ID validation | Solana oracle parser | `feed_id_binding_present=True` |
| A04 | BLOCKED | HIGH | Feed account validation | Solana set/update oracle | `account_binding=True, allowlist=True` |
| A05 | SUCCESS | HIGH | NaN/inf guard bypass via huge finite | Python secure_mint_amount | `mint_normal=831666, mint_huge=831666666666666666` |
| A06 | SUCCESS | HIGH | Rebalance commit secrets | Solana rebalance | `step=20000, turnover_max=40000, commit_threshold=50000` |
| A07 | BLOCKED | MEDIUM | Rebalance commit secrets | Python Keeper | `r1={'status': 'APPLIED', 'weights': [0.41, 0.29, 0.2, 0.1], 'mint_fee': 0.002}, r2={'status': 'REJECTED', 'reason': 'missing_commit_reveal_proof'}` |
| A08 | SUCCESS | MEDIUM | Rebalance commit secrets boundary | Python Keeper | `last={'status': 'APPLIED', 'weights': [0.425, 0.275, 0.2, 0.1], 'mint_fee': 0.002}, cumulative_turnover=0.04999999999999993` |
| A09 | SUCCESS | CRITICAL | Type confusion / non-finite handling | OAE staking deposit | `deposit_ok=True, balance=nan` |
| A10 | SUCCESS | CRITICAL | Type confusion / non-finite handling | OAE staking withdrawal | `request_ok=True, withdrawn=1000000.0, final_balance=0.0` |
| A11 | SUCCESS | HIGH | Type confusion / non-finite handling | OAE staking deposit | `deposit_ok=True, balance=inf` |
| A12 | SUCCESS | HIGH | Legacy unsigned claim carveout | OAE reward claims | `approved=1000, epoch_used=1000.0, balance=1000.0` |
| A13 | BLOCKED | LOW | NaN/inf guard edge value | OAE slash | `before=100.0, slashed=0.0, after=100.0` |
| A14 | SUCCESS | MEDIUM | Type confusion / queue logic | Redemption queue | `first_batch=['attacker-0', 'attacker-1', 'attacker-2', 'attacker-3'], second_batch=['victim']` |
| A15 | BLOCKED | MEDIUM | Anti-sybil dampening | Tournament rewards | `sybil_total=2.5` |
| A16 | SUCCESS | HIGH | Anti-sybil dampening | Tournament rewards | `sybil_total=14.761904761904761, unique_buckets=20` |
| A17 | BLOCKED | HIGH | Public key binding | Agent registry | `set_public_key_ok=False` |
| A18 | SUCCESS | CRITICAL | Public key binding race | Registry + ACP | `deregister=True, finalize=True, re_register=True, forged_verify=True` |
| A19 | BLOCKED | MEDIUM | allow_legacy=False | ACP verifier | `legacy_verify_default=False` |
| A20 | SUCCESS | MEDIUM | allow_legacy/expiry enforcement path | ACP verifier | `late_no_now=True, late_with_now=False` |
| A21 | BLOCKED | MEDIUM | Insurance cooldown | Insurance fund | `r1={'approved': True, 'reason': 'ok', 'treasury': 999000.0}, r2={'approved': False, 'reason': 'global_cooldown'}` |
| A22 | SUCCESS | HIGH | Insurance epoch manipulation | Insurance fund | `before=240000.0, invalid={'approved': False, 'reason': 'below_min_claim'}, after=440000.0` |
| A23 | BLOCKED | MEDIUM | Insurance cooldown boundary | Insurance fund | `tick19={'approved': True, 'reason': 'ok', 'treasury': 999000.0}, tick20={'approved': False, 'reason': 'global_cooldown'}` |
| A24 | BLOCKED | HIGH | Migration one-shot | Solana migration | `trusted_initializer=True, signer=True, one_shot=True` |
