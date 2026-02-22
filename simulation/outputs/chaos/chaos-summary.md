# Chaos Engineering Run Summary

- generated_at: 2026-02-22T04:56:24.180538+00:00
- total_scenarios: 8
- pass: 8
- fail: 0

| Scenario | Status | Recovery (ticks) | Max Peg Error | Min CR | Max CB |
|---|---|---:|---:|---:|---:|
| agent_kill | PASS | 5 | 0.0005 | 1.2010 | 0 |
| network_partition | PASS | 5 | 0.0007 | 1.2010 | 0 |
| oracle_freeze | PASS | 9 | 0.0012 | 1.2035 | 3 |
| memory_pressure | PASS | 5 | 0.0005 | 1.2010 | 0 |
| clock_skew | PASS | 5 | 0.0004 | 1.2010 | 0 |
| rapid_config_change | PASS | 5 | 0.0007 | 1.2010 | 0 |
| partial_collateral_failure | PASS | 33 | 0.0058 | 1.2025 | 2 |
| double_spend_race | PASS | 5 | 0.0008 | 1.2010 | 0 |

## Scenario Notes

### agent_kill
- status: PASS
- reason: single-agent failures degraded gracefully
- recovery_ticks: 5
- impact_scope: `{"funds_preserved": true, "max_cb_level": 0, "max_peg_error": 0.0005442388442358226, "min_cr": 1.2009999999999998, "mode_counts": {"NORMAL": 47, "SAFE_MODE": 33}, "tx_failure_rate": 0.0}`

### network_partition
- status: PASS
- reason: partition tolerated with bounded degradation
- recovery_ticks: 5
- impact_scope: `{"funds_preserved": true, "max_cb_level": 0, "max_peg_error": 0.0006610635863734116, "min_cr": 1.2009999999999998, "mode_counts": {"NORMAL": 85}, "tx_failure_rate": 0.15294117647058825}`

### oracle_freeze
- status: PASS
- reason: oracle freeze triggered CB-3 and mint halt
- recovery_ticks: 9
- impact_scope: `{"funds_preserved": true, "max_cb_level": 3, "max_peg_error": 0.001215335035744225, "min_cr": 1.203549484409434, "mode_counts": {"NORMAL": 90}, "tx_failure_rate": 0.0}`

### memory_pressure
- status: PASS
- reason: memory pressure respected graph-depth cap
- recovery_ticks: 5
- impact_scope: `{"funds_preserved": true, "max_cb_level": 0, "max_peg_error": 0.0005183158201997884, "min_cr": 1.2009999999999998, "mode_counts": {"NORMAL": 80}, "tx_failure_rate": 0.0}`

### clock_skew
- status: PASS
- reason: clock skew did not break consensus safety envelopes
- recovery_ticks: 5
- impact_scope: `{"funds_preserved": true, "max_cb_level": 0, "max_peg_error": 0.00036996019918666967, "min_cr": 1.2009999999999998, "mode_counts": {"NORMAL": 82}, "tx_failure_rate": 0.08536585365853659}`

### rapid_config_change
- status: PASS
- reason: malicious rapid proposals rejected ratio=1.00
- recovery_ticks: 5
- impact_scope: `{"funds_preserved": true, "max_cb_level": 0, "max_peg_error": 0.000733097913633185, "min_cr": 1.2009999999999998, "mode_counts": {"NORMAL": 75}, "tx_failure_rate": 0.0}`

### partial_collateral_failure
- status: PASS
- reason: partial collateral failure contained by breakers
- recovery_ticks: 33
- impact_scope: `{"funds_preserved": true, "max_cb_level": 2, "max_peg_error": 0.005816822708626157, "min_cr": 1.2025390397479958, "mode_counts": {"NORMAL": 88}, "tx_failure_rate": 0.0}`

### double_spend_race
- status: PASS
- reason: concurrent mint/redeem race preserved supply accounting
- recovery_ticks: 5
- impact_scope: `{"funds_preserved": true, "max_cb_level": 0, "max_peg_error": 0.000765373072646236, "min_cr": 1.2009999999999998, "mode_counts": {"NORMAL": 86}, "tx_failure_rate": 0.0}`
