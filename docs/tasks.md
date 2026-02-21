# microstable 작업 분해 (Tasks)

- 상태 표기: `[ ]` 대기 / `[-]` 진행중 / `[x]` 완료
- 원칙: **Spec → Plan → Test Cases → Tasks → Implementation** 순서 준수

---

## A. 문서화 단계

### A1. 백서
- [x] `whitepaper.md` 초안 작성 (영문, 11개 섹션)
- [ ] UST/Luna/DAI/FRAX/mStable 참고문헌 링크 보강
- [ ] Section 7(시뮬레이션 결과) 실측 데이터로 대체
- [ ] 용어 정리표(Glossary) 추가 여부 결정

### A2. Lv5 스펙
- [x] `spec.md` 작성 (한글, 수식 포함)
- [ ] 파라미터 기본값 1차 캘리브레이션
- [ ] 불변식 목록과 테스트 항목 1:1 매핑 검토

### A3. 계획/작업
- [x] `plan.md` 작성
- [x] `tasks.md` 작성
- [ ] 주간 실행 로그 템플릿 추가

---

## B. Phase 1 구현 (`microstable.py`, dependency zero)

### B1. 코어 엔진
- [ ] `Value` 클래스 구현 (`+,-,*,/,**,tanh,exp,log,relu`)
- [ ] topological sort + `backward()` 구현
- [ ] 수치 안정성 가드(epsilon, NaN/Inf 체크)

### B2. 프로토콜 상태/시장 모델
- [ ] `ProtocolState` (담보, 공급량, 파라미터) 정의
- [ ] `MarketEnv` (가격경로, 디페그 이벤트, oracle confidence) 정의
- [ ] 유효담보/CR 계산 함수 구현

### B3. 목적함수/최적화
- [ ] Loss 함수 구현 (peg/cr/var/turnover/concentration/oracle)
- [ ] Adam 업데이트 구현
- [ ] projection(가중치합=1, 상하한, 변화량 제한) 구현

### B4. 서킷브레이커
- [ ] CB-1 단일 담보 디페그 구현
- [ ] CB-2 다중 디페그/스트레스 구현
- [ ] CB-3 오라클 장애 구현
- [ ] CB-4 수치 불안정 롤백 구현

### B5. 실행기/출력
- [ ] 시나리오 러너 구현 (`normal`, `single_depeg`, `multi_depeg`, `volatile`, `gradient_attack`, `oracle_failure`)
- [ ] 지표 출력 (MAE/RMSE/minCR/turnover/breaker)
- [ ] 결과 저장 (`outputs/metrics.csv`, `outputs/events.log`)

### B6. 테스트
- [ ] `test_value.py` (미분 정확성)
- [ ] `test_loss.py` (손실항 계산 검증)
- [ ] `test_optimizer.py` (bounded update 검증)
- [ ] `test_circuit_breaker.py` (발동/복구 검증)

---

## C. Phase 1 검증

### C1. 성능 검증
- [ ] 정상장 peg MAE 목표치 달성 여부 확인
- [ ] 스트레스장 CR 하한 위반률 측정
- [ ] breaker 오탐/미탐률 측정

### C2. 재현성 검증
- [ ] 시드 고정 반복실행(최소 3회)
- [ ] 결과 편차 범위 기록
- [ ] 실패 케이스 재현 스크립트 정리

### C3. 문서 반영
- [ ] `whitepaper.md` Section 7 실측 결과 반영
- [ ] `spec.md` 파라미터 조정 결과 반영

---

## D. Phase 2 준비 (Solana / Anchor)

### D1. 온체인 모델링
- [ ] 계정 구조(PDA) 정의
- [ ] instruction 목록/권한 모델 확정
- [ ] invariant 체크 로직 설계

### D2. 오프체인 컴포넌트
- [ ] keeper 제안 포맷 정의
- [ ] 오라클 집계/검증 모듈 정의
- [ ] 제안 적용 전 시뮬레이션 dry-run 작성

### D3. 통합 테스트
- [ ] devnet 배포 스크립트
- [ ] mint/redeem/update/circuit E2E 테스트
- [ ] 24시간 soak test

---

## E. 완료 정의 (Definition of Done)

### Phase 1 DoD
- [ ] `microstable.py` 단일 파일로 전체 알고리즘 설명 가능
- [ ] 6개 필수 시나리오 실행 성공
- [ ] 핵심 KPI 자동 산출

### Phase 2 DoD
- [ ] devnet invariant 위반 0건
- [ ] breaker 비상 동작 검증 완료
- [ ] 운영 문서(실행/복구/관측) 완비

### Phase 3 DoD
- [ ] 공개 대시보드 제공
- [ ] 외부 테스터 재현성 확보
- [ ] mainnet 검토 여부를 위한 근거 데이터 확보

### Agent 자율운영 DoD (M7)
- [ ] Optimizer-Keeper가 tick 단위로 손실/gradient/Adam/bounded proposal을 자동 제출
- [ ] Watchdog가 오라클 교차검증 및 디페그 감시 후 breaker TX를 자동 제출
- [ ] Auditor가 온체인 불변식 전수검증 및 Keeper 이력 감사를 자동 수행
- [ ] 3/3 Agent 합의 시 48시간 타임락 후 자동 실행 E2E 검증
- [ ] 단일 Agent 거부 시 제안 보류 + 원인 로그 강제 기록 검증
- [ ] Agent 1/2/3 다운 시 SAFE_MODE/FROZEN/REDEEM_ONLY 전이 검증
- [ ] 수수료 분배(30/10/5/55) 및 Agent 자기지속 조건 모니터링 검증
