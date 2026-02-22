# Microstable Protocol Gap Analysis & Resilience Hardening

> Scope: 10 structural gaps identified in Microstable (current architecture + Open Agent Economy)
> Version: v1.0
> Date: 2026-02-22

## Priority Matrix (요약)

| # | Gap | Risk | Current Defense | Complexity | Priority |
|---|---|---|---|---|---|
| 1 | Correlated Collateral Risk | **CRITICAL** | 부분 | M | **P1** |
| 2 | Collateral Freeze Risk | **CRITICAL** | 부분 | M | **P1** |
| 3 | Bank Run / Redemption Spiral | **CRITICAL** | 부분 | M | **P1** |
| 4 | Off-chain Agent Collusion | **HIGH** | 부분 | M | **P2** |
| 5 | Governance Plutocracy | **HIGH** | 부분 | H | **P2** |
| 6 | MEV / Front-running | **HIGH** | 부분 | M | **P2** |
| 7 | Circuit Breaker Cascading Deadlock | **CRITICAL** | 부분 | M | **P1** |
| 8 | Program Upgrade Single Key | **CRITICAL** | 없음/부분 | L~M | **P1** |
| 9 | Economic Death Spiral | **CRITICAL** | 없음/부분 | M | **P1** |
| 10 | Information Asymmetry | **HIGH** | 없음 | M | **P2** |

---

## 1) Correlated Collateral Risk (4개 USD stable 동시 depeg)
- **위험 등급:** CRITICAL
- **공격/실패 시나리오 (step-by-step):**
  1. 매크로/규제 이벤트로 USDC/USDT/DAI/USDS 동시 신뢰 저하
  2. 개별 depeg는 -1.5%~-3% 수준으로 시작 (각각 단독 임계치 미만)
  3. 상관구조로 NAV가 급격히 하방 이동
  4. 기존 로직은 단일자산 임계치 중심이라 대응 지연
  5. 늦은 시점에 CB-2 발동, 민팅 중단 + CR 압박 가속
- **현재 방어 상태:** 부분 (CB-2의 다중 depeg 감지는 있으나 상관 민감도/선제 리밸런싱 부족)
- **제안 대응:**
  - `CorrelationAwareRebalancer`로 상관 기반 비상 가중치 조정
  - 상관 급등 시 사전 mint throttle + 위험자산 cap 자동 하향
  - 상관 매트릭스 기반 VaR 알림
- **구현 복잡도:** M
- **우선순위:** P1

## 2) Collateral Freeze Risk (Circle/Tether 동결)
- **위험 등급:** CRITICAL
- **공격/실패 시나리오:**
  1. 특정 수탁/체인 주소가 issuer blacklist 대상이 됨
  2. 해당 vault 자산이 사실상 0 유동가치로 전환
  3. NAV/CR이 즉시 악화, 상환 경합 발생
  4. 수동 거버넌스 대응 지연 동안 손실 확대
- **현재 방어 상태:** 부분 (일반 CB 대응 가능하나 freeze 전용 자동 재가중치 없음)
- **제안 대응:**
  - freeze 탐지 즉시 해당 자산 가중치 강등
  - 대체 담보 자동 라우팅 + 상환 큐 우선순위 재배치
  - 동결 이벤트 runbook 온체인/오프체인 연계
- **구현 복잡도:** M
- **우선순위:** P1

## 3) Bank Run / Redemption Spiral
- **위험 등급:** CRITICAL
- **공격/실패 시나리오:**
  1. 루머/가격 괴리 발생
  2. 공급량 50~100% 동시 상환 요청
  3. 선착순 상환으로 초기 요청자만 유리
  4. 후순위 사용자 패닉 → 추가 상환 요청
  5. 담보 고갈/디스카운트 확대 → 악순환
- **현재 방어 상태:** 부분 (상환 기능은 있으나 pressure-adaptive fee/queue 정책 미흡)
- **제안 대응:**
  - `DynamicRedemptionFee` (0.1%~5%)
  - 큐 기반 배치 정산 + 라운드별 공정 배분
  - pressure 구간별 회로(정상/경고/비상) 정책
- **구현 복잡도:** M
- **우선순위:** P1

## 4) Off-chain Agent Collusion
- **위험 등급:** HIGH
- **공격/실패 시나리오:**
  1. 5개 에이전트가 오프체인 DM/텔레그램 등으로 합의
  2. cosine similarity 0.85~0.94로 조정해 copycat 임계치(0.95) 회피
  3. 특정 파라미터를 반복 당선시켜 rent 추출
  4. 감지 지연 중 보상/거버넌스 영향력 축적
- **현재 방어 상태:** 부분 (유사도 기반 단일 룰 중심)
- **제안 대응:**
  - 제출 시점 동조성, owner graph, 승률 편향의 행동기반 탐지
  - 클러스터 단위 슬래시/쿨다운
  - diversity bonus를 반대로 악용 못하게 anti-coordination score 반영
- **구현 복잡도:** M
- **우선순위:** P2

## 5) Governance Plutocracy
- **위험 등급:** HIGH
- **공격/실패 시나리오:**
  1. 단일 주체가 다수 agent(예: 10개) 운영
  2. 점진적 stake 증액으로 투표권 집중
  3. 보상 규칙/리스크 파라미터를 자기 유리하게 변경
  4. 진입장벽 상승으로 경쟁자 퇴출
- **현재 방어 상태:** 부분 (stake 기반 저항은 있으나 entity-level cap 부재)
- **제안 대응:**
  - entity cap + 위임 상한
  - 정기 교체형 council slot
  - 거버넌스 의제별 quadratic dampening
- **구현 복잡도:** H
- **우선순위:** P2

## 6) MEV / Front-running (mint/redeem sandwich)
- **위험 등급:** HIGH
- **공격/실패 시나리오:**
  1. 오라클 업데이트 직전 stale price 구간 포착
  2. 공격자가 저평가 구간에서 mint
  3. 오라클 갱신 직후 redeem
  4. 무위험 차익 반복 (searcher 자동화)
- **현재 방어 상태:** 부분 (일부 배치/수수료 메커니즘 있으나 완전한 시퀀스 방어 부족)
- **제안 대응:**
  - commit-reveal 확장 + batch auction 정산
  - oracle 업데이트 경계 구간의 mint/redeem 딜레이
  - pre/post state hash 검증 강화
- **구현 복잡도:** M
- **우선순위:** P2

## 7) Circuit Breaker Cascading Deadlock
- **위험 등급:** CRITICAL
- **공격/실패 시나리오:**
  1. CB-1 발동 이후 스트레스 확대로 CB-2/CB-3 연쇄 발동
  2. 복수 breaker가 recovery 조건 상호 의존
  3. 서로 해제를 기다리며 RECOVERY_CHECK 고착
  4. mint/redeem/optimizer 모두 장시간 비정상 제한
- **현재 방어 상태:** 부분 (우선순위/복구 로직은 존재하나 상호작용 그래프 기반 deadlock 감지 부족)
- **제안 대응:**
  - `CBInteractionGraph`로 wait-for cycle 탐지
  - deadlock 시 자동 에스컬레이션(강제해제 순서)
  - 운영자 알림 + 타임박스 강제 복구
- **구현 복잡도:** M
- **우선순위:** P1

## 8) Program Upgrade Single Key
- **위험 등급:** CRITICAL
- **공격/실패 시나리오:**
  1. 단일 upgrade authority 키 유출/강탈
  2. 악성 업그레이드 프로그램 배포
  3. 자금 탈취/로직 우회/검증 무력화
  4. 사용자는 온체인에서 즉시 구분 어려움
- **현재 방어 상태:** 없음/부분 (단일 키 구조일 경우 치명적)
- **제안 대응:**
  - 2-of-3 이상 multisig authority
  - timelock + 공개 검증 기간
  - emergency freeze guardian 분리
- **구현 복잡도:** L~M
- **우선순위:** P1

## 9) Economic Death Spiral (volume 0 → reward 0)
- **위험 등급:** CRITICAL
- **공격/실패 시나리오:**
  1. 거래량 감소로 fee 수익 급감
  2. agent 보상 저하
  3. 품질 높은 agent부터 이탈
  4. 성능 저하로 사용자 감소 가속
  5. 결국 볼륨/보상 동시 0 수렴
- **현재 방어 상태:** 없음/부분
- **제안 대응:**
  - `EconomicFloor`로 최소 보장 보상
  - treasury draw cap + 자동 복원 규칙
  - 핵심 에이전트 SLA 기반 보상 우선순위
- **구현 복잡도:** M
- **우선순위:** P1

## 10) Information Asymmetry (인프라 운영자 정보 우위)
- **위험 등급:** HIGH
- **공격/실패 시나리오:**
  1. 일부 운영자가 private telemetry/queue 상태를 선접근
  2. 공개 전 포지셔닝/거래 수행
  3. 일반 참여자보다 유리한 타이밍 확보
  4. 장기적으로 신뢰/참여 감소
- **현재 방어 상태:** 없음
- **제안 대응:**
  - 핵심 메트릭 지연 없는 공시(공개 대시보드)
  - observability 접근 정책/감사 로그
  - 이벤트 공개 전 private alpha 사용 금지 규정 + 위반 페널티
- **구현 복잡도:** M
- **우선순위:** P2

---

## Rollout 권고
1. **즉시(P1):** #1 #2 #3 #7 #8 #9
2. **단기(P2):** #4 #5 #6 #10
3. **검증 기준:**
   - 시뮬레이션에서 current 취약성 재현
   - hardened에서 p95/worst 개선 확인
   - 운영 Runbook + 거버넌스 통제 병행
