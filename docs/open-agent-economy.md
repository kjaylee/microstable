# Microstable Phase 2 — Open Agent Economy (Spec)

> 문서 버전: v0.2 (Phase 2)
> 문서 상태: Draft (Simulation-first, On-chain PDA design included)
> 범위: 시뮬레이션 + Solana PDA 설계 (실자금 없음)

---

## 1. Vision & Architecture

### 1.1 Protocol Evolution

```
Closed 3-Agent (v1)
   └─ Semi-Open (v1.5)
        └─ Full Open (v2)
             └─ Agent Economy (v2.5+)
```

- **Closed 3-Agent**: Keeper/Watchdog/Auditor 고정 구성.
- **Semi-Open**: 제한된 whitelisted agent 참여.
- **Full Open**: permissionless 등록 + stake 기반 참여.
- **Agent Economy**: 경쟁형 최적화 토너먼트 + 보상/슬래싱 기반 자율 생태계.

### 1.2 Architecture (ASCII)

```
                  ┌────────────────────────────────────┐
                  │            Solana L1               │
                  │  - GlobalState / OracleState       │
                  │  - AgentRegistry (PDA)             │
                  │  - CircuitState / Treasury         │
                  └──────────────┬─────────────────────┘
                                 │
               ┌─────────────────┴──────────────────┐
               │                                    │
      ┌────────▼─────────┐                 ┌────────▼──────────┐
      │ Optimization     │                 │ Federated Watchdog│
      │ Tournament       │                 │ (M-of-N consensus)│
      │ - Commit/Reveal  │                 │ - PEG/CR/Oracle   │
      │ - Winner Select  │                 │ - Evidence + Slash│
      └────────┬─────────┘                 └────────┬──────────┘
               │                                    │
       ┌───────▼──────────┐                 ┌───────▼──────────┐
       │ Optimizer Agents │                 │ Monitor Agents   │
       └───────┬──────────┘                 └───────┬──────────┘
               │                                    │
       ┌───────▼──────────┐                 ┌───────▼──────────┐
       │ Auditor Agents   │                 │ Liquidator Agents│
       └──────────────────┘                 └──────────────────┘
```

### 1.3 Design Principles

- **Permissionless**: 누구나 stake로 참여 가능.
- **Stake-weighted**: 경제적 리스크를 부담하는 참여자 우선.
- **Reputation-based**: 과거 성과가 장기 보상과 거버넌스 영향력에 반영.
- **Slashable**: 잘못된 행동에 대해 명확한 경제적 처벌.

---

## 2. Agent Registry (On-chain)

### 2.1 Solana PDA Design

- **PDA seeds**: `"agent_registry" + agent_id`
- **Account**: `AgentRecord` 저장
- **Authority**: agent_id 공개키 기반 서명

### 2.2 Agent Types

- `Optimizer`
- `Monitor`
- `Auditor`
- `Liquidator`

### 2.3 Registration Flow

1. stake deposit
2. register instruction
3. status → Active

### 2.4 Deregistration Flow

1. deregister request
2. cooldown period
3. unstake 가능

### 2.5 Data Structure

```text
AgentRecord {
  agent_id: Pubkey,
  agent_type: AgentType,
  stake: u64,
  reputation: i64,
  registered_at: i64,
  last_active: i64,
  total_rewards: u64,
  total_slashed: u64,
  proposals_submitted: u64,
  proposals_accepted: u64,
  status: AgentStatus (Active/Cooldown/Slashed/Deregistered)
}
```

---

## 3. Agent Communication Protocol (ACP v1)

### 3.1 JSON-RPC 2.0 Envelope

```json
{
  "jsonrpc": "2.0",
  "method": "acp.submit_proposal",
  "params": {
    "agent_id": "...",
    "epoch": 42,
    "proposal": { "weights": [0.4,0.3,0.2,0.1], "mint_fee": 0.002 },
    "evidence": { "loss_estimate": 0.0028, "backtest_ticks": 1000 },
    "signature": "..."
  },
  "id": "uuid"
}
```

### 3.2 Actions

- `acp.register`
- `acp.submit_proposal`
- `acp.vote`
- `acp.report_anomaly`
- `acp.claim_reward`
- `acp.query_state`
- `acp.heartbeat`

### 3.3 Authentication

- Ed25519 signature per message
- Agent registry public key for verification

### 3.4 Rate Limiting

- per-agent, per-type
- 기본값: 100 msg/epoch (simulation)

---

## 4. Optimization Tournament

### 4.1 Epoch Structure

- Default epoch: 1 hour = 3600 slots
- Submission window: 0~80%
- Evaluation window: 80~100%

### 4.2 Selection Criteria

- Predicted loss (forward-looking)
- Risk-adjusted return (Sharpe-like)
- Novelty bonus
- Reputation weight

### 4.3 Winner Selection

- Weighted score 최고점
- 기존 파라미터 대비 악화 시 유지

### 4.4 Reward Distribution

- Winner: 30%
- Runner-up: 10%
- All valid participants: 5% (pro-rata)
- Treasury: 55%

### 4.5 Anti-gaming

- Commit–reveal (commit hash → reveal proposal)
- Minimum stake for submission
- Copycat penalty (cosine similarity > 0.95)
- Stake-weighted reputation (sybil resistance)

---

## 5. Federated Watchdog Network

### 5.1 Consensus

- **M-of-N** activation
- Dynamic M: `min(3, ceil(N/2))`

### 5.2 Alert Types

- `PEG_DEVIATION`
- `CR_VIOLATION`
- `ORACLE_STALE`
- `ANOMALY`

### 5.3 Incentives & Penalties

- False positive → slash
- True positive → bounty reward

### 5.4 Evidence Requirement

- State snapshot + oracle data + timestamp
- Stale evidence (>10 epochs) reject

### 5.5 Diversity Incentive

- Different methodologies yield bonus
- Monitors must not share identical detection logic

---

## 6. Staking & Slashing Economics

### 6.1 Minimum Stake

- Optimizer: 10 SOL
- Monitor: 5 SOL
- Auditor: 20 SOL
- Liquidator: 2 SOL

### 6.2 Slashing Conditions

- Bad proposal (loss +5%): 10% stake slash
- False CB activation: 5% stake slash
- Missed heartbeat (>10 epochs): 1% stake slash / epoch
- Sybil attack: 100% slash
- Collusion: 50% slash

### 6.3 Reward Sources

- Mint/Redeem fees (0.1~0.3%)
- Epoch participation rewards
- Performance bonuses
- Anomaly detection bounties

---

## 7. Reputation System

### 7.1 Gains

- Proposal accepted: +10
- Correct anomaly detection: +20
- Successful audit: +50
- Uptime per epoch: +1

### 7.2 Losses

- Bad proposal: -15
- False alarm: -25
- Missed heartbeat: -5
- Slashing event: -100

### 7.3 Tiers

- Newcomer (0–99): 1.0x
- Established (100–499): 1.5x
- Veteran (500–999): 2.0x
- Elite (1000+): 3.0x + governance weight

### 7.4 Decay

- -1% per week inactive

---

## 8. Security Model

- Sybil attack vectors + stake-weighted mitigation
- Collusion detection (proposal correlation)
- MEV protection (commit-reveal)
- Agent impersonation prevention (Ed25519 signatures)
- Eclipse attack on watchdog (M-of-N + SAFE_MODE)
- Economic attacks (griefing analysis)

---

## 9. Integration Guide

### 9.1 OpenClaw Skill: `microstable-agent`

- Capability: ACP v1 send/receive, proposals, anomaly reporting
- Required fields: agent_id, agent_type, stake, pubkey

### 9.2 Python SDK: `microstable-sdk`

- `MicrostableClient` (REST + Solana)
- `submit_proposal(weights, fee, evidence)`
- `report_anomaly(type, evidence)`

### 9.3 REST Wrapper

- POST `/acp/submit_proposal`
- POST `/acp/report_anomaly`
- GET `/acp/state`

### 9.4 Minimal Agent Examples

**Optimizer (50 lines)**

```python
from microstable_sdk import MicrostableClient

client = MicrostableClient("https://api.microstable.dev")
agent_id = "opt_001"
weights = [0.4,0.3,0.2,0.1]

client.submit_proposal(
    agent_id=agent_id,
    epoch=42,
    weights=weights,
    mint_fee=0.002,
    evidence={"loss_estimate":0.0028, "backtest_ticks":1000}
)
```

**Monitor (30 lines)**

```python
from microstable_sdk import MicrostableClient

client = MicrostableClient("https://api.microstable.dev")
client.report_anomaly(
    agent_id="mon_001",
    alert_type="PEG_DEVIATION",
    evidence={"snapshot": {...}, "oracle": {...}, "timestamp": 123}
)
```

---

## 10. Migration Plan

1. **Phase 1 → Phase 2**: core protocol unchanged, only new modules added.
2. **Backward compatibility**: 기존 3-agent 시스템 유지.
3. **Feature flags**: ACP, Registry, Tournament 단계적 활성화.
4. **Emergency rollback**: registry freeze + revert to 3-agent mode.

---

## Appendix: Checklist

- [x] Agent registry 설계
- [x] ACP v1 프로토콜 정의
- [x] Optimization tournament 구조
- [x] Federated watchdog M-of-N
- [x] Stake/Slash/Rep 시스템
- [x] Integration & migration 계획
