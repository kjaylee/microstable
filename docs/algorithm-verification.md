# microstable 알고리즘 수학적 검증 리포트 (M1)

- 대상 문서
  - `specs/microstable/whitepaper.md`
  - `specs/microstable/spec.md`
- 검증 성격: **명세 기반 정적 수학 검증** (구현 코드 미포함)
- 작성 시각(로컬): 2026-02-22 KST

---

## 0. 총평 (요약)

| 검증 항목 | 판정 | 요약 |
|---|---|---|
| 1. Value autograd 정확성 | **CONCERN** | 기본 미분식/체인룰은 정합적이나, `**`, `relu(0)`, `clamp`, 수치 안정성 경계에서의 gradient 정의가 불충분 |
| 2. Loss 함수 미분 가능성 | **CONCERN** | 6개 항 모두 거의-모든 점에서 미분 가능. 단, hinge/L1/Var의 경계·경로 정의가 명세상 불완전 |
| 3. Adam + Bounded Projection | **CONCERN** | 투영 최적화 구조는 타당. 다만 상수 LR Adam + 다중 cap/투영 조합의 수렴 보장은 명세만으로 불충분 |
| 4. 서킷브레이커 상태머신 | **FAIL** | 발동 조건은 있으나 복구 조건, 동시 발동 우선순위, 충돌 해소 규칙 부재로 모순/진동 가능성 배제 불가 |
| 5. 경제적 안전성 | **CONCERN** | 조치 방향은 합리적이나 “death spiral 불가능성”/oracle 장애 시 solvency 보장은 형식적 가정과 엄밀 규칙이 추가로 필요 |

> 불확실/동역학 의존 항목은 아래에 **“시뮬레이션으로 검증 필요”**로 명시.

---

## 1. Value autograd 정확성 검증

### 1.1 연산별 해석적 미분과의 일치성

노드 출력 $z$를 입력 $(x,y)$의 함수로 둘 때, 로컬 미분은 다음과 같다.

- 덧셈: $z=x+y$
  $$
  \frac{\partial z}{\partial x}=1,\quad \frac{\partial z}{\partial y}=1
  $$
- 뺄셈: $z=x-y$
  $$
  \frac{\partial z}{\partial x}=1,\quad \frac{\partial z}{\partial y}=-1
  $$
- 곱셈: $z=xy$
  $$
  \frac{\partial z}{\partial x}=y,\quad \frac{\partial z}{\partial y}=x
  $$
- 나눗셈: $z=x/y$
  $$
  \frac{\partial z}{\partial x}=\frac{1}{y},\quad \frac{\partial z}{\partial y}=-\frac{x}{y^2}
  $$
- 거듭제곱(상수 지수 가정): $z=x^a$
  $$
  \frac{\partial z}{\partial x}=a x^{a-1}
  $$
- 쌍곡탄젠트: $z=\tanh x$
  $$
  \frac{\partial z}{\partial x}=1-\tanh^2 x
  $$
- 지수: $z=e^x$
  $$
  \frac{\partial z}{\partial x}=e^x
  $$
- 로그: $z=\log x$ ($x>0$)
  $$
  \frac{\partial z}{\partial x}=\frac{1}{x}
  $$
- ReLU: $z=\max(0,x)$
  $$
  \frac{\partial z}{\partial x}=\begin{cases}
  1,& x>0\\
  0,& x<0
  \end{cases}
  $$
  $x=0$에서는 미분 불능이며 subgradient $[0,1]$.

**판정:** 위 식 자체는 표준 해석 미분과 일치 (**PASS**).

### 1.2 Chain rule / DAG reverse 정확성

손실 $L$에 대해 계산 그래프가 DAG이고, 역전파를 위상정렬 역순으로 수행하면
$$
\frac{\partial L}{\partial v} = \sum_{u\in \text{children}(v)} \frac{\partial L}{\partial u}\frac{\partial u}{\partial v}
$$
를 각 노드에서 누적한다. DAG이므로 모든 경로가 유한하며, 역순 처리 시 상위 노드의 upstream gradient가 먼저 확정되어 체인룰이 정확히 구현된다.

**판정:** 명세의 “topological sort → reverse + grad 누적”은 수학적으로 정합 (**PASS**).

### 1.3 수치 안정성 에지케이스

명세상 가드:
- `log(x)`에서 $x\le 0$ 방지 epsilon clamp
- `/x`에서 분모 epsilon 하한
- NaN/Inf 시 rollback + breaker

검토 결과:
1. `log(max(x,\varepsilon))`를 하드 클램프로 구현하면 $x\le\varepsilon$ 구간에서 gradient가 0이 되어 학습 편향 가능.
2. $1/x$는 $|x|\to 0$에서 기울기 폭증. 단순 하한 클램프만으로는 부호/연속성 처리 불명확.
3. `exp(x)`는 $x\to\infty$ overflow. 클램프 필요하나 하드 클램프는 gradient saturation 유발.
4. `**` 연산에서 $x<0$, 비정수 지수일 때 실수영역 정의 불가. 도메인 제약 필요.
5. `relu(0)`, `clamp` 경계 subgradient 정책 미정.

**항목 판정:** **CONCERN**

### 1.4 수정 제안

- `log`: 
  $$
  \log(x) \to \log(\operatorname{softplus}(x)+\varepsilon)
  $$
  또는 하드클램프 사용 시 경계 gradient 정책 문서화.
- division: 
  $$
  y_{safe}=\operatorname{sign}(y)\max(|y|,\varepsilon),\quad z=x/y_{safe}
  $$
- `exp`: 입력 클립 범위 명시 (예: $x\in[-20,20]$), overflow 전 차단.
- `pow`: “상수 지수만 허용” 또는 “기저 $x>0$ 강제” 명시.
- `relu(0)`, `|x|` at 0, `max` 경계점 subgradient를 고정 규칙(예: 0)으로 명세.

---

## 2. Loss 함수 미분 가능성 검증

손실:
$$
\mathcal{L}_t=
\lambda_p(p_t-1)^2
+\lambda_{cr}\max(0,CR_{min}-CR_t)^2
+\lambda_{var}\operatorname{Var}(\Delta NAV_{t:t+H})
+\lambda_{turn}\|w_t-w_{t-1}\|_1
+\lambda_{conc}\sum_i w_{i,t}^2
+\lambda_{orc}(1-q_t)^2
$$

### 2.1 6개 항의 gradient well-defined 여부

- Peg 항: 
  $$
  \nabla_\theta \lambda_p(p_t-1)^2 = 2\lambda_p(p_t-1)\nabla_\theta p_t
  $$
  (미분 가능)
- CR 힌지 제곱항: 거의-모든 점에서 미분 가능, 경계는 subgradient 필요.
- 분산항: 통계량 자체는 미분 가능하나 $\Delta NAV$의 시간전개 경로가 필요.
- Turnover L1: 0에서 미분 불능, subgradient 필요.
- Concentration L2: 
  $$
  \partial /\partial w_i\left(\sum_j w_j^2\right)=2w_i
  $$
- Oracle 항: 
  $$
  \nabla_\theta \lambda_{orc}(1-q_t)^2=-2\lambda_{orc}(1-q_t)\nabla_\theta q_t
  $$
  (단, $q_t$가 외생이면 해당 경로는 0)

**판정:** 기본 구조는 타당하나 경계점 처리/동역학 경로 정의가 불충분 → **CONCERN**.

### 2.2 $\max(0,CR_{min}-CR_t)^2$ 경계

$u=CR_{min}-CR_t$라 두면 $f(u)=\max(0,u)^2$.
$$
f'(u)=\begin{cases}
0,&u<0\\
2u,&u>0
\end{cases}
$$
$u=0$에서 좌우미분 모두 0이므로 실제로 $C^1$ (1차 미분 연속)이다. 따라서
$$
\nabla_\theta f = -2\max(0,CR_{min}-CR_t)\nabla_\theta CR_t
$$
로 안전하게 구현 가능.

**판정:** **PASS**.

### 2.3 $\|w_t-w_{t-1}\|_1$ 미분 가능성

성분별 $d_i=w_{i,t}-w_{i,t-1}$에 대해
$$
\frac{\partial |d_i|}{\partial d_i}=\operatorname{sign}(d_i),\quad d_i\neq 0
$$
$d_i=0$에서 subgradient $[-1,1]$.

현재 명세에는 경계 정책 부재.

**판정:** **CONCERN**.

**수정 제안:**
- 고정 subgradient: $d_i=0$에서 0 사용(안정적)
- 또는 smooth 근사:
  $$
  |d_i|\approx \sqrt{d_i^2+\delta^2}
  $$
  (예: $\delta=10^{-6}$)

### 2.4 $\operatorname{Var}(\Delta NAV)$ 미분 경로

$z_k=\Delta NAV_{t+k}$, $\mu=\frac1H\sum_k z_k$이면
$$
\operatorname{Var}(z)=\frac1H\sum_k (z_k-\mu)^2
$$
따라서
$$
\nabla_\theta \operatorname{Var}(z)
=\frac{2}{H}\sum_k (z_k-\mu)\nabla_\theta z_k
$$
문제는 $\nabla_\theta z_k$를 위해 미래 상태천이(시장/정책)의 미분 경로를 명세해야 한다는 점이다.

- 1-step 근사인지
- $H$-step unroll BPTT인지
- 외생 샘플 경로를 상수 취급하는지

명세에 부재.

**판정:** **CONCERN** (**시뮬레이션으로 검증 필요**).

---

## 3. Adam + Bounded Projection 검증

### 3.1 수학적 구조의 타당성

업데이트:
$$
\theta_{t+1}=\Pi_\Omega\left(\theta_t-\alpha\frac{\hat m_t}{\sqrt{\hat v_t}+\epsilon}\right)
$$
이는 “적응형 1차 방법 + 제약집합 투영”으로 표준 형태다.

**판정:** 구조 자체는 **PASS**.

### 3.2 수렴성 분석

일반 비볼록 + 투영 + 상수 학습률 Adam에서는 전역 수렴 보장을 주기 어렵다. 알려진 충분조건(예: diminishing step, AMSGrad류, bounded gradient, Lipschitz-smooth 근사)이 명세에 없음.

또한 gradient clip, per-step cap, simplex projection이 중첩되면 실제 업데이트가
$$
\Delta\theta_t = \Pi_\Omega(\theta_t+u_t)-\theta_t
$$
로 왜곡되어 이론적 Adam 궤적과 달라진다.

**판정:** **CONCERN**.

**수정 제안:**
1. 수렴 지향 설정:
   - $\alpha_t=\alpha_0/\sqrt{t}$ 또는 단계적 decay
   - Adam 대신 AMSGrad 옵션 제공
2. 정지조건 명시:
   $$
   \|\Pi_\Omega(\theta_t-\eta\nabla L)-\theta_t\|_2 < \tau
   $$
3. clip/cap/projection 적용 순서 고정 및 문서화.

### 3.3 Simplex projection 정확성

가중치 제약(비음수, 합 1)에 대한 유클리드 투영은 다음 KKT 해로 정확:

정렬 $u_1\ge\dots\ge u_n$, 
$$
\rho=\max\left\{j: u_j-\frac{1}{j}\left(\sum_{i=1}^j u_i-1\right)>0\right\},
\quad
\tau=\frac{1}{\rho}\left(\sum_{i=1}^{\rho}u_i-1\right)
$$
$$
w_i^*=\max(y_i-\tau,0)
$$

다만 명세에는 알고리즘 형태가 미기재.

**판정:** **CONCERN** (원리상 PASS 가능하나 구현 규격 미정).

**수정 제안:** 위 공식을 spec에 명시하고, $w_i^{max}$까지 포함하는 **capped simplex projection** 절차를 추가.

### 3.4 Per-step delta cap의 수렴 영향

- 장점: 단기 왜곡/공격 영향 상한화.
- 위험: cap이 과도하면 최적점 접근이 매우 느려지거나 제약 경계에서 진동.

최악 영향 상한:
$$
\|\theta_{t+K}-\theta_t\|_\infty \le K\cdot \Delta_{max}
$$
(가중치: $\Delta_{max}=0.02$, 수수료: 10bps)

즉 공격/오차 영향은 선형으로 제한되나, 수렴 속도도 동일하게 제한.

**판정:** **CONCERN** (**시뮬레이션으로 검증 필요**).

---

## 4. 서킷브레이커 상태머신 검증

### 4.1 발동/복구 조건 모순성

- 발동 조건(CB-1~CB-4)은 비교적 명확.
- **복구 조건(recover)**가 수치 임계값/지속시간으로 정의되어 있지 않음.

복구 규칙 미정이면 상태머신의 폐쇄성(어떻게 정상 상태로 돌아오는지) 증명 불가.

**판정:** **FAIL**.

### 4.2 동시 발동 우선순위/상호작용

잠재 충돌 예:
- CB-2: mint 중단
- CB-3: 저민팅 보수프로파일
- CB-4: rollback + LR half (그러나 CB-3는 optimizer off)

동시 발동 시 연산 결합법칙(AND/OR/min/max)이 명세에 없다.

**판정:** **FAIL**.

### 4.3 Zeno(무한 진동) 가능성

현재는 hysteresis, 최소 유지시간, cooldown 명시가 없어 임계치 인근에서 발동/복구 반복 가능.

**판정:** **FAIL** (**시뮬레이션으로 검증 필요**).

### 4.4 수정 제안 (필수)

1. 상태집합/전이표 명문화:
   - 상태: `NORMAL, CB1, CB2, CB3, CB4, COMPOSITE`
   - 전이: `(state, event) -> state'`
2. 우선순위 규칙 명시(예시):
   $$
   CB4 \succ CB3 \succ CB2 \succ CB1
   $$
   단, 액션은 “더 보수적인 제약”으로 병합:
   - `mint_limit = min(active limits)`
   - `optimizer_enabled = AND(active flags)`
   - `weight_cap_i = min(active cap_i)`
3. 히스테리시스 도입:
   - 발동 임계 $\delta_{on}$, 복구 임계 $\delta_{off}$ with $\delta_{off}<\delta_{on}$
4. 최소 유지시간/쿨다운:
   - `min_hold_steps`, `cooldown_steps` 정의.

---

## 5. 경제적 안전성 검증

### 5.1 Death spiral 불가능성

명세의 안전장치(민팅 스로틀, 동적 CR 상향, 상환 큐)는 **반사적 확장 속도**를 줄이는 방향으로 타당하다.

하지만 “불가능성 증명”을 위해선 최소한 다음이 필요:
- 시장 충격 상한, 유동성/슬리피지 함수, 상환 큐 처리율
- 발행/상환 가격 규칙과 지연 모델
- 담보 동결/헤어컷 동역학

현재 문서만으로는 절대 불가능성 정리(정리/증명) 구성 불가.

**판정:** **CONCERN** (**시뮬레이션으로 검증 필요**).

**수정 제안:**
- 정리 형태로 조건부 안전성 명시:
  > 가정 A(충격 상한), B(민팅 한도), C(최소 CR 유지) 하에서, 유한 시간 내 $CR_t<CR_{hard\_min}$ 진입 확률 상계.
- 큐 처리율 제약 및 스트레스 테스트 임계 공개.

### 5.2 Gradient manipulation attack 영향 범위

per-step cap으로 파라미터 변화량이 제한되므로 공격자가 $K$ step 동안 gradient를 왜곡해도
$$
\|\Delta\theta\|_\infty \le K\Delta_{max}
$$
로 상한화 가능. 이는 명세상의 핵심 방어로 합리적.

단, 장기 지속 공격에는 누적 드리프트 가능.

**판정:** **PASS (조건부)**.

**보강 제안:**
- 큰 업데이트 2-phase commit
- 다중 윈도우 robust 통계(clip/Huber)
- 이상구간 탐지 시 optimizer freeze.

### 5.3 Oracle 장애 fallback 모드의 solvency 보장

CB-3는 “optimizer 중지 + 보수 프로파일”만 명시. Oracle 장애 시 발행/상환 가격결정이 stale이면 역선택으로 준비금 유출 가능.

즉, **solvency 보장**을 주장하려면 다음이 추가되어야 함:
- oracle degraded 시 mint 전면중단(또는 매우 강한 haircut)
- redeem은 보수적 가격(최악값) 또는 속도 제한
- 신뢰 회복 전까지 담보평가 보수화

현재 명세로는 보장 증명 불가.

**판정:** **FAIL**.

**수정 제안 (필수):**
1. `oracle_degraded => mint_enabled = 0` (강제)
2. 상환가 산식에 안전할인 적용
3. stale 최대허용시간 초과 시 emergency mode 격상(CB-2와 동등 제약)

---

## 6. 권고 우선순위 (실행용)

### P0 (즉시 반영 필요)
1. CB 상태머신 전이/복구/우선순위 명세화 (FAIL 해소)
2. Oracle degraded 시 발행 중지 + 상환 보수평가 규칙 명시 (FAIL 해소)

### P1 (수학/학습 안정성 보강)
1. `relu(0)`, `L1@0`, `clamp` 경계 subgradient 고정 규칙
2. `pow` 도메인 제약 및 `exp/log/div` 안정화 함수 규격
3. Var 항의 gradient 경로(1-step vs H-step unroll) 명시

### P2 (수렴성 품질 개선)
1. Adam 수렴 보강(AMSGrad 또는 LR decay)
2. simplex/capped-simplex projection 의사코드 스펙 포함
3. cap/clip/projection 순서 고정 및 테스트 케이스화

---

## 7. 결론

- 본 명세는 **핵심 아이디어(미분 기반 적응 + 하드 가드레일)**가 수학적으로 일관된 방향에 있음.
- 다만 생산급 안전성 관점에서, 현재 가장 큰 결함은 **상태머신/오라클 장애 시 규칙의 불완전성**이다.
- 즉시 보완 시, 나머지 CONCERN 항목은 구현·시뮬레이션으로 충분히 검증 가능한 수준이다.

> 동역학·시장미시구조 의존 주장(특히 death spiral 불가능성, 장기 수렴성)은 명세 기반 정적 검증만으로는 완결 불가이며, **시뮬레이션으로 검증 필요**.
