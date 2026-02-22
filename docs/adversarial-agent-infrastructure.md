# Microstable Adversarial Agent Infrastructure

## 목적
Open Agent Economy에서 AI 에이전트 간 자율 공격/방어를 **상시(24/7) 내장 운영**하기 위한 Red Team + Blue Team 아키텍처와 시뮬레이션 스펙.

---

## 1) Threat Model — Agent vs Agent

### 공격자 능력 모델
- **Speed**: 1 action / ms
- **Parallelism**: 10,000 concurrent sybil agents
- **Intelligence**: full source visibility + real-time state analysis
- **Persistence**: non-stop operation, zero fatigue
- **Adaptability**: reward-based policy mutation / RL adaptation
- **Resources**: up to $1M attack budget
- **Coordination**: instant swarm-level strategy sync
- **Simulation**: pre-attack MC search (1M+ windows)

### 공격자 Tier
1. Tier-1 Script bot — 반복형
2. Tier-2 Smart bot — 규칙/패턴 적응
3. Tier-3 RL adversary — 성공률 최적화
4. Tier-4 Multi-agent swarm — 협조형 분산 공격
5. Tier-5 State-level adversary — 고자원 + 내부자 + full code analysis

---

## 2) Agent Red Team Architecture

### 2.1 Attack Generator
- 기존 26개 공격 벡터를 base library로 사용
- Mutation engine
  - Parameter mutation (intensity/budget/stealth)
  - Composition (2~3 chain, max depth 5)
  - Timing mutation (epoch boundary / pre/post)
  - Scale mutation (1 → 100 → 10,000)
- Novel attack discovery
  - Fuzzing action sequence로 invariant violation 탐색
  - Coverage-guided state-space 탐색 우선순위화
  - Gradient-like fitness 증대 방향으로 진화(실행 결과 기반)

### 2.2 Attack Executor
- 시뮬레이션 환경에서 공격 실행
- 결과 분류: success / partial_success / failed / blocked
- 성공 케이스 exploit record 자동 저장
- 실패 원인(시그니처 차단, 방어 강화 등) 기록

### 2.3 Attack Swarm
- Sybil swarm: 대량 등록/투표 조작
- Drain swarm: micro-claim 반복
- Eclipse swarm: monitor 고립 시도
- Timing swarm: epoch 경계 race 유도

### 2.4 Evolutionary Attack Engine
- Population 기반 세대 진화
- Fitness 평가 (tier/intensity/stealth/scale/novelty)
- Crossover + mutation + selection
- 100 세대 기준 상위 attack set 산출

---

## 3) Agent Blue Team Architecture

### 3.1 Anomaly Detection
- Statistical
  - registration burst/gradual sybil
  - micro-claim drain
- Graph
  - sybil cluster(connected component)
  - circular flow(3-cycle)
- Behavioral
  - agent baseline(mean/std)
  - deviation z-score
  - progressive trust
- Adaptive threshold
  - FP 누적 시 탐지 임계 자동 상향

### 3.2 Response Engine
- 자동 대응
  - sybil burst → mass slash + registration freeze
  - collusion → quarantine
  - drain → rate limit/cooldown
  - eclipse → backup consensus
- 상향 대응(escalation)
  - SAFE_MODE
  - treasury lock
  - emergency freeze
- Idempotent response 보장(alert 재수신 시 noop)

### 3.3 Forensics
- root-cause chain 생성
- impact 정량화(loss/duration/victims)
- 공격 signature 생성 및 DB 축적
- replay 차단에 signature 재사용

### 3.4 Adaptive Defense
- 성공 공격을 signature로 즉시 학습/차단
- defense_strength와 learned_bias 점진 증가
- Red-Blue loop 반복으로 immunity 상승

---

## 4) Red-Blue Adversarial Loop

```text
Red generates attack
 -> execute
 -> if success: forensics + signature + response + defense learn
 -> if fail: red mutates/evolves
Repeat
```

### Immunity Score
- Formula baseline: `1 - (successful_attacks / total_attacks)`
- Campaign metric: round-by-round immunity series와 convergence 추적
- 목표: Tier별 방어율 상향 + 장기 immunity > 0.95~0.99

---

## 5) Metrics
- Attack success rate by tier
- MTTD / MTTR
- FPR / TPR
- SAFE_MODE downtime
- Financial impact & attacker profit
- Peg MAE
- Treasury minimum
- Survival / uptime
- Defense coverage
- Signature DB growth

---

## 6) 구현 파일
- Simulation: `adversarial_agents.py`
- Tests: `test_adversarial_agents.py` (100 TCs)
- Results:
  - `outputs/adversarial-test-run.log`
  - `outputs/adversarial-agent-results.json`
  - `outputs/adversarial-agent-report.md`

---

## 7) 재현성/제약
- Python 3.12+
- 외부 의존성 최소 (stdlib 중심)
- seed 고정 재현 가능
- 기존 `microstable.py` 비수정
