# Microstable Agent Intelligence Gate (AIG)

문서 버전: v0.1  
문서 상태: Draft (Simulation-first + Solana account 설계)

---

## 1) 목적

Open Agent Economy(OAE)는 스테이킹만으로 참여가 가능하므로, 역량이 낮거나 악의적인 에이전트가 프로토콜 안정성을 훼손할 수 있다.  
AIG는 **참여 전/초기/운영 중** 에이전트 품질을 계층적으로 검증해, 무능·지연·모방·공격 취약 에이전트를 조기 배제한다.

---

## 2) Tier 구조

## Tier 0 — Challenge Exam (필수 사전 시험)

- 대상: 신규 참여 신청 에이전트
- 시험 구성: **10개 역사적 시나리오**
  - 정상장
  - 고변동장
  - 급락(Flash Crash)
  - 단일 자산 Depeg
  - 다중 자산 Depeg
  - 유동성 위기
  - 오라클 지연/스테일
  - 공격성 시장 조작
  - 회복 국면
  - 혼합 스트레스 장
- 제출값(시나리오별):
  - 바스킷 가중치(weights)
  - mint/redeem 수수료 파라미터
  - CR target
- 채점 지표:
  - **peg MAE** (낮을수록 우수)
  - **CR 유지율** (높을수록 우수)
  - **CB 오작동률** (낮을수록 우수)
- 합격 기준:
  - 종합점수 컷오프 + 안전성 최소 기준 충족
  - baseline 대비 **상위 80 percentile 이상**

> Tier 0 불합격 시 등록 불가.

---

## Tier 1 — Sandbox Trial (100 epochs)

- 환경:
  - 가상 자금 기반
  - 실제 프로토콜 로직/제약 동일
  - 다수 에이전트 동시 경쟁
- 측정 항목:
  - 성과 점수(시나리오 적합도)
  - 응답 속도(latency)
  - 일관성(성과 분산)
  - 모방/복제(copycat) 비율
- 탈락 조건 예시:
  - 평균 성과 미달
  - 응답 지연 과다
  - 성과 분산 과다
  - 복제 비율 임계치 초과

---

## Tier 2 — Probation (제한 권한 실전)

- 기간: 최소 **30 epochs** 트랙레코드
- 제한:
  - 소액 스테이킹 상한
  - 보수적 권한(민감 조정 폭 제한)
- 운영:
  - 기존 keeper와 동일 task 수행
  - invariant 위반/CB 회피/지연 지속 감시
- 통과 조건:
  - 평균 점수 + 안전성 + 지연 기준 동시 충족

---

## Tier 3 — Full Participation (정식 참여)

- Tier 2 통과 후 승격
- 제한 해제:
  - 스테이킹 상한 해제
  - 일반 권한 전부 사용 가능
- 지속 모니터링:
  - AgentScore 주기 업데이트
  - 점수 하락/위험행동 탐지 시 자동 강등

---

## 3) AgentScore 정의 (0~100)

최종 점수는 5개 메트릭 가중합으로 계산한다.

1. **Optimization Quality** (peg MAE 기반)
   - peg MAE가 낮을수록 높은 점수
2. **Response Latency** (위기 반응 속도)
   - 평균 지연이 낮을수록 높은 점수
3. **Safety Record** (불변식 위반/CB 회피율)
   - 위반/회피가 적을수록 높은 점수
4. **Adversarial Resilience** (공격 방어율)
   - 공격 시나리오에서의 방어 성공률
5. **Consistency** (성과 분산)
   - 성과 분산이 낮을수록 높은 점수

예시 가중치:

- Optimization Quality: 0.35
- Response Latency: 0.20
- Safety Record: 0.20
- Adversarial Resilience: 0.15
- Consistency: 0.10

Tier 매핑(예시):

- 90~100: Tier 3
- 80~89: Tier 2
- 70~79: Tier 1
- <70: Tier 0

---

## 4) On-chain 구현 설계 (Solana)

### 4.1 AgentScore PDA

- Seed: `"agent_score" + agent_pubkey`
- Account 구조(예시):

```text
AgentScoreRecord {
  agent: Pubkey,
  tier: u8,                      // 0..3
  score_total: u16,              // 0..10000 (2-decimal fixed point)
  optimization_quality: u16,
  response_latency: u16,
  safety_record: u16,
  adversarial_resilience: u16,
  consistency: u16,
  challenge_passed: bool,
  sandbox_passed: bool,
  probation_passed: bool,
  last_updated_epoch: u64,
  downgrade_flag: bool,
}
```

### 4.2 Score 업데이트 instruction

- `initialize_agent_score(agent_pubkey)`
- `submit_agent_score(agent_pubkey, metrics, proof_hash)`
  - 유효성 검증:
    - 범위(0~100)
    - epoch 단조 증가
    - 권한(keeper/oracle signer)
- `finalize_tier(agent_pubkey)`
  - 정책에 따라 승격/유지/강등 적용

### 4.3 Tier 승격/강등 자동화

- 승격 조건:
  - Tier n 필수 조건 + 최소 유지 epoch 충족
- 강등 조건:
  - score threshold 하회
  - safety violation 누적
  - copycat/공격 취약도 급증
- 실행 방식:
  - epoch 마감 시 자동 평가 instruction
  - 결과를 AgentRegistry와 동기화

---

## 5) OAE 통합 정책

- AgentRegistry 등록 시 Tier 0(Challenge Exam) 통과 여부 검증
- 미통과 에이전트는 등록 실패
- 기존 시스템과의 호환을 위해 feature flag 기반 점진 활성화 가능

---

## 6) 구현 아티팩트

- `agent_intelligence_gate.py`
  - `ChallengeExam`
  - `SandboxTrial`
  - `AgentScorer`
  - `IntelligenceGate`
  - `SmartAgent`, `RandomAgent`, `CopyAgent`, `LazyAgent`, `MaliciousAgent`
- `test_agent_intelligence_gate.py`
  - 50+ 테스트케이스
  - 승격/강등/점수 계산/통합 검증 포함
