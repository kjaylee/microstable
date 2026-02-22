# Adversarial Agent Infrastructure — Test Report

## 1) Test Execution Summary
- Total TCs: **100**
- PASS: **100**
- FAIL: **0**
- SKIP: **0**
- Pytest summary: `============================= 100 passed in 10.00s =============================`

## 2) Immunity (100-round campaign, seed=42)
- Final immunity score: **1.0000**
- Attack success rate: **0.0000**
- MTTD: **1.02 epochs**
- MTTR: **2.29 epochs**
- Peg MAE: **0.0100**
- Uptime: **0.9990**

## 3) 주요 발견 (Blue가 못 막은 공격, low-defense 탐색)
- 조건: defense_strength=0.2, learned_bias=0.0, 300 샘플
  - sybil: 8 successes
  - mev: 7 successes
  - timing: 5 successes
  - eclipse: 4 successes
  - collusion: 4 successes

## 4) 산출물
- Spec: `docs/adversarial-agent-infrastructure.md`
- Simulation: `adversarial_agents.py`
- Tests: `test_adversarial_agents.py` (100 TCs)
- Raw log: `outputs/adversarial-test-run.log`
- JSON results: `outputs/adversarial-agent-results.json`
