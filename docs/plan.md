# microstable 실행 계획 (Plan)

- 문서 버전: v0.1
- 기준: Phase 1 우선(순수 시뮬레이션), Phase 2는 devnet 검증

## 1. 전체 타임라인 (제안)

- **W1~W2**: Phase 1 설계 고정 + `microstable.py` 구현
- **W3**: 시나리오 검증/지표 수집/백서 결과 섹션 초안 반영
- **W4~W6**: Phase 2 Anchor 프로그램 프로토타입 + devnet 배포
- **W7**: keeper/오라클/관측 도구 통합
- **W8**: 공개 테스트 리허설 및 다음 단계 의사결정
- **W9**: Agent 인터페이스 통합 + 자율 거버넌스 리허설

> 실제 일정은 주당 투입시간(취미 프로젝트)에 맞춰 탄력 조정.

---

## 2. 마일스톤

### M1. 알고리즘 동결 (W1)
- 산출물:
  - `spec.md` v1 확정
  - `whitepaper.md` v0.1 완성
  - 수식/파라미터 기준선 동결
- 종료 기준:
  - 핵심 수식/제약/서킷브레이커 정의 확정

### M2. Phase 1 코드 완성 (W2)
- 산출물:
  - `microstable.py` (<=500줄, dependency zero)
  - 기본 시나리오 파일(6종)
- 종료 기준:
  - 시뮬레이터 실행 성공
  - 로그/지표 출력 가능

### M3. Phase 1 검증 리포트 (W3)
- 산출물:
  - 시나리오별 성능표
  - 실패 케이스 분석
  - `whitepaper.md` Section 7 업데이트 자료
- 종료 기준:
  - 최소 3회 반복 실행 결과 재현성 확보

### M4. Solana 커널 프로토타입 (W4~W5)
- 산출물:
  - Anchor program skeleton
  - 상태계정/PDA/instruction 틀 완성
- 종료 기준:
  - initialize/mint/redeem/update 기본 경로 통과

### M5. Devnet 통합 (W6~W7)
- 산출물:
  - keeper 업데이트 제출 스크립트
  - 오라클 검증 로직
  - breaker 상태전이 테스트 결과
- 종료 기준:
  - devnet 장기 실행(최소 24h) 안정 동작

### M6. 공개 준비 (W8)
- 산출물:
  - 대시보드 최소버전
  - 테스트 가이드 문서
  - 리스크/한계 정리
- 종료 기준:
  - 외부 테스터가 시나리오를 독립 재현 가능

### M7. Agent 인터페이스 통합 (W9)
- 산출물:
  - Optimizer-Keeper / Watchdog / Auditor 인터페이스 스펙
  - 3/3 합의 + 48시간 타임락 자동 실행 모듈
  - Agent 장애(1/2/3 다운) 대응 런북
- 종료 기준:
  - 다중 Agent 합의/거부/보류 플로우 E2E 검증
  - 수수료 자동 분배(PDA) 및 자기지속 조건 모니터링 검증

---

## 3. 단계별 우선순위

### Phase 1 우선순위 (Must)
1. `Value` autograd 정확성
2. Loss/Optimizer/Projection 안전성
3. Circuit Breaker 확실한 발동 규칙
4. 재현 가능한 시나리오 실행기

### Phase 2 우선순위 (Should)
1. 온체인 불변식 강제
2. 업데이트 bounded check
3. 오라클 장애 안전모드
4. 관측 가능한 이벤트 로그

### Phase 3 우선순위 (Could)
1. UI/시각화 개선
2. 외부 테스트 자동화
3. 문서 다국어 정리

---

## 4. 의사결정 게이트 (Go / No-Go)

### Gate A (Phase 1 → Phase 2)
- 조건:
  - 6개 시나리오 모두 비발산
  - 정상장 peg MAE 목표 달성
  - 스트레스장 CR 하한 위반률 기준 이내
- 미충족 시:
  - Loss 계수/제약 재튜닝 후 Phase 1 반복

### Gate B (Phase 2 → 공개 테스트)
- 조건:
  - devnet 24h 무중단
  - breaker 오작동률 기준 이내
  - 온체인 invariant 위반 0건
- 미충족 시:
  - 온체인 제약 강화 및 keeper 전략 보수화

### Gate C (공개 테스트 → Mainnet 검토)
- 조건:
  - 보안/경제/법무 리뷰 완료
  - 제한된 캡에서 안정성 데이터 확보
- 미충족 시:
  - Mainnet 보류 (연구 프로젝트 상태 유지)

---

## 5. 측정 지표 (KPI)

1. **Peg 품질**: MAE, RMSE, tail deviation
2. **건전성**: 최소/평균 CR, 하한 위반률
3. **적응 안정성**: 파라미터 변화율, turnover
4. **안전장치 성능**: breaker 탐지율/오탐률/복구시간
5. **운영성**: 재현성, 장기 실행 안정성

---

## 6. 리소스 및 제약

- 인력: 1인(취미/교육 프로젝트)
- 제약:
  - 초기 구현은 단순성 우선
  - 실자금/실거래 연동 금지(Phase 1)
  - 복잡한 ML 의존성 도입 금지

---

## 7. 산출물 체크포인트

- [ ] `whitepaper.md` 초안 완료
- [ ] `spec.md` Lv5 완료
- [ ] `tasks.md` 체크리스트 확정
- [ ] `microstable.py` 구현
- [ ] Phase 1 결과 섹션(whitepaper Sec.7) 업데이트
- [ ] Anchor 프로토타입
- [ ] devnet 통합 테스트 리포트
- [ ] M7 Agent 인터페이스 통합 검증 리포트
