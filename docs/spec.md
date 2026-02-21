# microstable Lv5 스펙 (Production-grade Draft)

- 문서 버전: v0.1
- 문서 상태: Phase 1 구현 전 최종 설계안
- 범위: **현재는 순수 시뮬레이션(교육/취미)**, 실제 자금 운용 없음
- 핵심 철학: **"This file is the complete algorithm. Everything else is just efficiency."**

---

## 1. 프로젝트 개요

**microstable**은 다중 스테이블코인(USDC/USDT/DAI 등)을 담보 바스킷으로 보유하고, 프로토콜 파라미터(담보비율 목표, 바스킷 가중치, 수수료 등)를 미분 가능한 형태로 모델링한 뒤 gradient descent로 지속 최적화하는 자기진화형 스테이블코인 프로토콜이다. 1차 목표는 `microstable.py` 단일 파일로 알고리즘 전체를 재현하는 시뮬레이터 구축이며, 2차 목표는 동일 규칙을 Solana(Rust/Anchor) 온체인 제약에 맞게 분해하여 devnet에서 안전하게 검증하는 것이다.

## 2. 목표

### 2.1 1차 목표 (Phase 1: Python 시뮬레이션)
- 순수 Python, 의존성 0, 500줄 이내(`microstable.py`)
- `Value` autograd 기반 미분/역전파 구현
- 멀티 담보 바스킷 + 손실함수 + Adam 업데이트 + 서킷브레이커 구현
- 시나리오 테스트(정상/디페그/오라클 장애/공격) 재현

### 2.2 2차 목표 (Phase 2: Solana devnet)
- Rust/Anchor 프로그램으로 핵심 상태머신 이식
- 온체인 불변식(담보비율/가중치합/업데이트 제한) 강제
- 오라클 입력 검증 및 keeper 기반 업데이트 제출
- devnet 공개 테스트 및 관측 대시보드 구축

### 2.3 3차 목표 (Phase 3: Mainnet, 미정)
- 보안감사/경제감사 이후 제한적 mainnet 검토
- 위험도 기반 출시 단계화(whitelist → cap 확장)
- 규제/법무 검토 완료 전 실자금 확대 금지

---

## 3. 핵심 메커니즘 상세

### 3.1 바스킷 구성

#### 3.1.1 지원 담보 목록 (초기)
- `USDC`
- `USDT`
- `DAI`
- `USDS`(또는 동급 탈중앙 달러자산; 환경에 따라 대체 가능)

#### 3.1.2 초기 비율
$$
\mathbf{w}_0 = [0.40, 0.30, 0.20, 0.10]
$$

#### 3.1.3 제약조건
$$
\sum_i w_i = 1, \quad 0 \le w_i \le w_i^{\max}
$$

권장 초기 상한:
- `USDC`: $w_i^{\max}=0.55$
- `USDT`: $w_i^{\max}=0.45$
- `DAI`: $w_i^{\max}=0.45$
- `USDS`: $w_i^{\max}=0.35$

#### 3.1.4 리스크 스코어 반영
각 담보별 리스크 스코어 $r_i \in [0,1]$를 정의하고 유효담보가치를
$$
V_{eff} = \sum_i (w_i \cdot P_i \cdot (1-h_i(r_i)))
$$
로 계산한다. $h_i$는 haircut 함수.

### 3.2 Value 클래스 스펙 (autograd)

`Value`는 micrograd 스타일 스칼라 자동미분 노드.

#### 3.2.1 필수 필드
- `data: float`
- `grad: float`
- `_prev: set[Value]`
- `_op: str`
- `_backward: callable`
- `label: str | None`

#### 3.2.2 지원 연산 (최소)
- 산술: `+`, `-`, `*`, `/`, `**`
- 단항: `neg`
- 비선형: `tanh`, `exp`, `log`, `relu`
- 보조: `clamp(min,max)` (미분가능 근사 또는 구간 내 상수기울기)

#### 3.2.3 역전파
- DAG topological sort 후 reverse order 실행
- 시작점: `loss.backward()`
- 그래디언트 누적 방식: `node.grad += local_grad * upstream`

#### 3.2.4 안정성 규칙
- `log(x)`는 $x \le 0$ 방지용 epsilon 클램프
- `/x`는 분모 하한 epsilon 적용
- NaN/Inf 발생 시 해당 스텝 롤백 + 서킷브레이커 후보 플래그

### 3.3 Loss 함수 정의

프로토콜 목적함수:

$$
\mathcal{L}_t =
\lambda_p (p_t-1)^2
+ \lambda_{cr}\,\max(0,CR_{min}-CR_t)^2
+ \lambda_{var}\,\mathrm{Var}(\Delta NAV_{t:t+H})
+ \lambda_{turn}\,\|\mathbf{w}_t-\mathbf{w}_{t-1}\|_1
+ \lambda_{conc}\,\sum_i w_{i,t}^2
+ \lambda_{orc}(1-q_t)^2
$$

변수 정의:
- $p_t$: 프로토콜 토큰 시장가격
- $CR_t$: 담보비율
- $q_t$: 오라클 신뢰 점수
- $H$: 전망 윈도우 길이

권장 초기 계수:
- $\lambda_p = 5.0$
- $\lambda_{cr} = 20.0$
- $\lambda_{var} = 2.0$
- $\lambda_{turn} = 0.5$
- $\lambda_{conc} = 1.5$
- $\lambda_{orc} = 3.0$

### 3.4 Gradient Descent 파라미터

기본 옵티마이저: Adam

$$
m_t = \beta_1 m_{t-1} + (1-\beta_1)g_t
$$
$$
v_t = \beta_2 v_{t-1} + (1-\beta_2)g_t^2
$$
$$
\hat{m}_t = \frac{m_t}{1-\beta_1^t}, \quad
\hat{v}_t = \frac{v_t}{1-\beta_2^t}
$$
$$
\theta_{t+1} = \Pi_\Omega\left(\theta_t - \alpha \frac{\hat{m}_t}{\sqrt{\hat{v}_t}+\epsilon}\right)
$$

초기값:
- learning rate $\alpha = 0.005$
- $\beta_1 = 0.9$
- $\beta_2 = 0.999$
- $\epsilon = 10^{-8}$
- gradient clip norm: `1.0`
- 파라미터 변화량 상한: 스텝당 `±2%` (가중치), `±10 bps` (수수료)

### 3.5 리밸런싱 트리거 조건

리밸런싱 실행 조건(OR):
1. 주기 트리거: `Δt >= 60초(시뮬레이션 tick)`
2. 페그 이탈: $|p_t-1| > 0.003$ (30 bps)
3. 담보 악화: $CR_t < CR_{target} + 0.02$
4. 오라클 신뢰 하락: $q_t < 0.85$

리밸런싱 제한 조건:
- 단일 실행에서 총 turnover $\le 15%$
- 위험자산 비중 감축 우선, 안정자산 확장은 단계적 반영

### 3.6 서킷브레이커 조건 및 동작

#### 3.6.1 발동 조건 및 악화 동작

##### CB-1 단일 담보 디페그
- 발동 조건: 특정 담보에서 $|P_i-1| > 0.02$가 3틱 연속 지속
- 악화 동작:
  - 해당 담보 상한 즉시 `-50%`
  - 신규 민팅 일시 제한(예: 25% rate limit)
  - 목표 담보비율 `+5%p`

##### CB-2 다중 담보 스트레스
- 발동 조건: 2개 이상 담보 동시 디페그 또는 basket NAV 급락
- 악화 동작:
  - 신규 민팅 일시 중단
  - 상환은 큐 기반 처리(공정 배분)
  - 리밸런싱은 안전모드 파라미터로 고정

##### CB-3 오라클 장애
- 발동 조건: feed stale > 허용시간 또는 소스 간 편차 > 임계치
- 악화 동작:
  - gradient 업데이트 중지
  - 정적 보수 프로파일(고CR, 저민팅)로 전환
  - 관리자/감시자 알림 플래그

##### CB-4 수치 안정성 장애
- 발동 조건: NaN/Inf/발산성 loss 증가
- 악화 동작:
  - 이전 체크포인트로 롤백
  - learning rate 50% 축소 후 재시도

#### 3.6.2 복구 조건 및 단계적 원복

- 공통 원칙: 각 CB는 `min_hold` 경과 후에만 복구 판정을 수행한다.

##### CB-1 복구
- 복구 조건: 해당 담보가
$$
|P_i-1| < 0.005
$$
를 최소 10틱 연속 유지
- 원복 동작: 디페그 시 적용한 상한 `-50%` 제한은 20틱에 걸쳐 단계적으로 원복

##### CB-2 복구
- 복구 조건: 모든 담보 정상 + basket NAV 회복 상태를 최소 20틱 연속 유지
- 원복 동작: 민팅 재개는 rate limit 50%에서 시작하며, 10틱마다 10%p씩 증가
$$
\mathrm{mint\_limit}(\Delta t)=\min\left(1.0,\;0.5+0.1\left\lfloor\frac{\Delta t}{10}\right\rfloor\right)
$$

##### CB-3 복구
- 복구 조건: 오라클 피드 정상화( stale 해소 + 소스 편차 < 임계치 )를 5틱 연속 유지
- 원복 동작: gradient 업데이트 재개 시 learning rate를 기본값의 50%에서 시작
$$
\alpha_{resume}=0.5\,\alpha_{base}
$$

##### CB-4 복구
- 복구 조건: 롤백 성공 후 다음 3틱 연속 loss 감소
$$
\mathcal{L}_{t} > \mathcal{L}_{t+1} > \mathcal{L}_{t+2} > \mathcal{L}_{t+3}
$$
- 원복 동작: learning rate 50% 축소 상태를 10틱 유지한 뒤 기본값으로 원복

#### 3.6.3 동시 발동 우선순위 및 충돌 해소

우선순위:
$$
\mathrm{CB\text{-}4} > \mathrm{CB\text{-}3} > \mathrm{CB\text{-}2} > \mathrm{CB\text{-}1}
$$

- 상위 CB가 활성이면 하위 CB의 **완화 동작은 보류**한다.
- 단, 하위 CB의 **악화 동작은 즉시 적용**한다.
- 복구는 반드시 **역순**으로 수행한다.
  - `CB-4 → CB-3 → CB-2 → CB-1`

#### 3.6.4 Zeno 방지 (히스테리시스 + 쿨다운)

- 발동 후 최소 유지시간(`min_hold`):
  - CB-1: 5틱
  - CB-2: 10틱
  - CB-3: 3틱
  - CB-4: 3틱
- 복구 후 쿨다운: 같은 CB는 복구 후 최소 5틱이 지나야 재발동 가능
- 30틱 내 같은 CB가 3회 발동하면 장기 안전모드(`EXTENDED_ACTIVE`)로 격상하며,
  - 해당 CB의 최소 유지시간을 `min_hold × 3`으로 적용

#### 3.6.5 상태 전이 다이어그램 (텍스트)

```text
NORMAL ──[trigger]──→ ACTIVE ──[min_hold 경과 + 복구 조건]──→ COOLDOWN ──[5틱]──→ NORMAL
                         │                                                           │
                         └──[3회/30틱]──→ EXTENDED_ACTIVE ──[min_hold×3 + 복구]──→ COOLDOWN
```

### 3.7 오라클 요구사항

Phase 1(시뮬레이터):
- 합성 가격 생성기(랜덤워크 + 점프 + 상관 디페그 이벤트)
- oracle confidence score $q_t$ 제공

Phase 2(Solana):
- 최소 2개 독립 소스(Pyth + Switchboard 권장)
- 신선도(staleness) 체크 필수
- 중앙값/가중중앙값 집계
- 소스 편차가 임계치 초과 시 `oracle_degraded=true`

---

## 4. 아키텍처

### 4.1 Phase 1: Python 시뮬레이션 아키텍처

구성 모듈(단일 파일 내 섹션 분리):
1. `Value` autograd 엔진
2. `MarketEnv` (가격/유동성/오라클 상태 생성)
3. `ProtocolState` (담보, 공급량, 파라미터)
4. `LossEngine` (목적함수 계산)
5. `Optimizer` (Adam + projection)
6. `CircuitBreaker`
7. `Runner` (시나리오 실행/로그/리포트)

### 4.2 Phase 2: Solana 온체인 아키텍처

주요 온체인 계정(PDA):
- `GlobalState`: 전역 파라미터, 모드, 버전
- `CollateralVault[i]`: 담보별 보관/상태
- `BasketConfig`: 가중치/상한/위험점수
- `OracleState`: 최근 가격/신뢰도/staleness
- `CircuitState`: breaker 상태머신
- `UpdateProposal`: 오프체인 옵티마이저 제안 저장

핵심 instruction:
- `initialize`
- `mint`
- `redeem`
- `submit_update_proposal`
- `apply_update` (bounded check + invariant check)
- `trigger_circuit_breaker`
- `recover_from_breaker`

### 4.3 데이터 흐름도 (텍스트)

1. 오라클 입력 수집 → 가격/신뢰도 집계  
2. 현재 상태 스냅샷 생성(`CR`, `peg_error`, `weights`)  
3. 손실함수 계산 및 gradient 산출  
4. Adam 업데이트 + 제약 투영  
5. 서킷브레이커 조건 점검  
6. (정상) 파라미터 반영 / (비정상) 안전모드 전환  
7. 로그 기록(지표, 이벤트, 상태전이)  
8. 다음 tick 반복

---

## 5. 테스트 시나리오

### 5.1 정상 시장 (안정)
- 입력: 변동성 낮은 랜덤워크, oracle 정상
- 기대:
  - peg MAE < 0.0015
  - 평균 CR > 목표치
  - turnover 과도 증가 없음

### 5.2 단일 담보 디페그
- 입력: 특정 담보 -5% 급락 이벤트
- 기대:
  - CB-1 발동
  - 디페그 담보 비중 단계적 축소
  - 시스템 CR 임계치 미만 미진입

### 5.3 다중 담보 동시 디페그
- 입력: 2개 담보 동시 -8% 충격
- 기대:
  - CB-2 발동, 민팅 중단
  - 상환 큐 동작
  - 손실 확대 속도 둔화 확인

### 5.4 급격한 시장 변동
- 입력: 고변동 장세(점프 빈도 증가)
- 기대:
  - 학습률/리밸런싱 제한으로 발산 방지
  - peg 복원 시간 SLA 이내

### 5.5 Gradient 조작 공격
- 입력: 악의적 가격 패턴 주입(짧은 스파이크)
- 기대:
  - per-step delta cap으로 급격한 파라미터 왜곡 방지
  - robust loss로 민감도 완화

### 5.6 Oracle 장애
- 입력: stale feed, 소스 불일치 확대
- 기대:
  - CB-3 발동
  - gradient 업데이트 중지 + 보수 프로파일 전환

---

## 6. 리스크 분석

### 6.1 기술적 리스크
- autograd 버그로 잘못된 gradient 전파
- 수치 불안정(overflow/underflow)
- 온체인/오프체인 상태 불일치

대응:
- 단위테스트 + property test
- NaN/Inf guard + checkpoint rollback
- 상태 해시 검증 및 재현 가능한 로그

### 6.2 경제적 리스크
- 동시 디페그/유동성 고갈
- 중앙화 스테이블코인 동결 리스크
- 상환 러시(run) 시 가격괴리 확대

대응:
- 담보 다변화 + 집중도 패널티
- 민팅 제한/상환 큐/동적 CR 상향
- 극단 시나리오 사전 시뮬레이션

### 6.3 규제적 리스크
- 스테이블코인 규제 변화
- 특정 담보의 법적/운영 중단 리스크
- 관할권별 컴플라이언스 상이

대응:
- Phase 1/2는 연구/테스트 한정
- 실자금 전환 전 법무 검토 필수
- 위험 자산 whitelist/blacklist 프로세스 수립

---

## 7. 구현 계획

### 7.1 Phase 1: `microstable.py`
- 목표: 알고리즘 전체를 단일 파일에 구현(<=500줄, dependency zero)
- 산출물:
  - 시뮬레이터 본체
  - 시나리오 러너
  - 결과 요약(텍스트/CSV)
- 완료 조건:
  - 필수 시나리오 6종 통과
  - 핵심 지표 자동 출력

### 7.2 Phase 2: Rust/Anchor 솔라나 프로그램
- 목표: 불변식 강제 가능한 온체인 커널 구현
- 산출물:
  - Anchor program
  - keeper 제출 스크립트
  - devnet 배포 문서
- 완료 조건:
  - devnet에서 mint/redeem/update/circuit 전 플로우 검증

### 7.3 Phase 3: 프론트엔드 + devnet 공개
- 목표: 관측/실험 가능한 대시보드 제공
- 산출물:
  - 상태/지표 시각화 UI
  - 이벤트 타임라인
  - 공개 테스트 가이드
- 완료 조건:
  - 외부 테스터가 시나리오 재현 가능

---

## 8. 파일 구조

```text
specs/microstable/
  ├─ whitepaper.md
  ├─ spec.md
  ├─ plan.md
  └─ tasks.md

microstable/
  ├─ microstable.py
  ├─ README.md
  ├─ scenarios/
  │   ├─ normal.json
  │   ├─ single_depeg.json
  │   ├─ multi_depeg.json
  │   ├─ volatile.json
  │   ├─ gradient_attack.json
  │   └─ oracle_failure.json
  ├─ outputs/
  │   ├─ metrics.csv
  │   └─ events.log
  └─ tests/
      ├─ test_value.py
      ├─ test_loss.py
      ├─ test_optimizer.py
      └─ test_circuit_breaker.py
```

---

## 9. 성공 기준

### Phase 1 성공 기준
- 코드 복잡도: 단일 파일, 의존성 0
- 안정성: 6개 시나리오 모두 비발산
- 성능지표:
  - 정상장 peg MAE < 0.0015
  - 스트레스장에서도 CR 하한 위반률 < 1%
  - breaker 오탐률 < 5%

### Phase 2 성공 기준
- 온체인 불변식 위반 0건
- update proposal 검증 실패 시 안전 롤백 100%
- devnet 장기 러닝(24h) 무중단

### Phase 3 성공 기준
- 외부 재현성 문서 완비
- 대시보드로 핵심 지표 실시간 확인 가능
- 공개 테스트 피드백 반영한 v1.0 스펙 확정


## 10. Agent 기반 자율 거버넌스

### 10.1 Agent 역할 정의

프로토콜 운영은 3개 Agent가 고정 역할을 분담하며, 사람 수동 개입 없이 tick 단위로 동작한다.

1. **Optimizer-Keeper**
   - 매 tick에서 손실함수 $\mathcal{L}_t$ 계산 → gradient 산출 → Adam update → bounded proposal 생성 → 온체인 제출
   - 역할: 파라미터 적응 최적화의 실행 주체
   - 보상: 프로토콜 수수료의 `30%`

2. **Watchdog**
   - 오라클 교차검증, 담보 디페그 감시, 이상 징후 탐지 후 서킷브레이커 트리거 TX 제출
   - 역할: 실시간 리스크 감시 및 비상 정지 트리거
   - 보상: 프로토콜 수수료의 `10%`

3. **Auditor**
   - 온체인 불변식 전수 검증, Keeper 제안 이력 감사, 이상 패턴 탐지 후 Alert + 긴급모드 TX 제출
   - 역할: 사후 감사 + 사전 이상 탐지의 이중 안전 계층
   - 보상: 프로토콜 수수료의 `5%`

### 10.2 Agent 인센티브 메커니즘

- 각 Agent는 Solana PDA 지갑을 독립적으로 부여받고, 수수료는 온체인에서 자동 분배한다.
- 프로토콜 수수료 $F_t$의 분배 규칙은 다음과 같다.

$$
F_t^{OK}=0.30F_t,\quad
F_t^{WD}=0.10F_t,\quad
F_t^{AU}=0.05F_t,\quad
F_t^{TR}=0.55F_t
$$

여기서 $OK, WD, AU, TR$는 각각 Optimizer-Keeper, Watchdog, Auditor, Treasury를 의미한다.

- Agent $a$의 자기 지속 조건은 다음과 같다.

$$
R_{a,t} > C_{a,t},\quad
R_{a,t}=F_{a,t},\quad
C_{a,t}=C^{compute}_{a,t}+C^{gas}_{a,t}
$$

- 수입이 지출보다 작아지는 경우($R_{a,t}<C_{a,t}$), Agent는 실행 빈도를 자동 축소한다.

$$
f_{a,t+1}=\max\left(f_{min},\;f_{a,t}\cdot\min\left(1,\frac{R_{a,t}}{C_{a,t}+\epsilon}\right)\right)
$$

### 10.3 Multi-Agent Governance (사람 완전 배제)

자산 편입/퇴출은 아래 조건을 코드로 고정하며, 사람 승인 단계를 두지 않는다.

- 시가총액: $\mathrm{MCap}_j > 10^9$
- 일 거래량: $\mathrm{Vol}^{24h}_j > 10^8$
- 오라클 피드 수: $N^{oracle}_j \ge 2$
- 최근 90일 디페그 이력: $\mathrm{Depeg90}_j < 0.03$

편입 조건 판정식:

$$
I^{in}_j=\mathbf{1}\left[\mathrm{MCap}_j>10^9\;\land\;\mathrm{Vol}^{24h}_j>10^8\;\land\;N^{oracle}_j\ge2\;\land\;\mathrm{Depeg90}_j<0.03\right]
$$

- $I^{in}_j=1$이면 자동 편입 제안 생성
- 기존 편입 자산이 조건을 이탈하면 자동 퇴출 제안 생성

합의/실행 규칙:

$$
v_{OK}+v_{WD}+v_{AU}=3 \Rightarrow t_{exec}=t_{proposal}+48\mathrm{h}
$$

단일 Agent라도 거부하면 제안을 보류하고 원인 로그를 강제 기록한다.

$$
\exists a\in\{OK,WD,AU\}:v_a=0 \Rightarrow \text{status}=\text{HOLD},\;\text{reason\_log}=1
$$

### 10.4 Agent 장애 대응

- **Agent 1개 다운**
  - 나머지 2개 Agent로 안전모드 운영
  - 업데이트 빈도 축소(예: 기준 빈도의 $50\%$)

- **Agent 2개 다운**
  - 프로토콜 자동 동결
  - 보수 정적 프로파일(고담보/저민팅)로 즉시 전환

- **Agent 3개 다운**
  - 신규 민팅 전면 중단
  - 상환만 허용
  - 온체인 불변식 검증 로직으로만 시스템 보호

장애 상태는 생존 Agent 수 $n_{alive}$에 따라 아래와 같이 결정한다.

$$
\begin{aligned}
n_{alive}=2 &\Rightarrow \text{SAFE\_MODE} \\
n_{alive}=1 &\Rightarrow \text{FROZEN} \\
n_{alive}=0 &\Rightarrow \text{REDEEM\_ONLY}
\end{aligned}
$$

---

## 부록 A. 핵심 불변식

1. 담보가치 불변식
$$
CR_t = \frac{V_{eff,t}}{S_t} \ge CR_{hard\_min}
$$

2. 가중치 합 불변식
$$
\sum_i w_i = 1
$$

3. 변화량 제한 불변식
$$
|w_{i,t+1}-w_{i,t}| \le \Delta w_{max}
$$

4. 오라클 신뢰 불변식
$$
q_t < q_{min} \Rightarrow \text{optimizer\_enabled}=0
$$
