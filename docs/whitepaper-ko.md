# microstable: 자기진화형 다중 담보 스테이블코인 프로토콜

**버전(Version)**: Draft v0.1  
**상태(Status)**: 연구 백서(Research Whitepaper, Educational / Hobby Project)  
**대상 체인(Target Chain, Phase 2+)**: Solana

> "This file is the complete algorithm. Everything else is just efficiency."
>
> "이 파일이 완전한 알고리즘이다. 그 외의 모든 것은 효율의 문제일 뿐이다."

---

## 1. 초록(Abstract)

스테이블코인(Stablecoin)은 탈중앙금융(DeFi)의 핵심 인프라가 되었지만, 기존 설계는 담보 집중 위험(Collateral Concentration Risk), 거버넌스 지연(Governance Latency), 오라클 의존성(Oracle Dependency), 체제 전환(Regime Shift)에 취약한 정적 위험 파라미터(Static Risk Parameters)라는 구조적 한계를 반복적으로 노출해 왔다. 본 백서는 비트코인 백서의 명시적 규칙 기반 정신과 Karpathy의 microgpt/micrograd가 보여준 소형 미분가능 프로그래밍(Differentiable Programming) 접근을 결합한 **microstable**을 제안한다.

microstable은 스테이블코인 담보 바스킷(예: USDC, USDT, DAI)을 유지하면서, 미분가능한 위험-페그 손실함수(Risk-and-Peg Loss Function)에 대한 경사하강법(Gradient Descent)으로 바스킷 가중치와 일부 프로토콜 파라미터를 연속적으로 최적화한다. 또한 자동 리밸런싱(Automatic Rebalancing)과 결정론적 안전장치(회로차단기(Circuit Breaker), 경계 투영(Bounded Parameter Projection), 민트/리딤 스로틀(Mint/Redeem Throttle))를 결합해 스트레스 상황에서도 지급여력(Solvency)을 보전하도록 설계한다.

본 문서는 결정론적 온체인 실행 커널(On-Chain Execution Kernel)과 후보 업데이트를 계산·검증 가능한 범위로 제출하는 오프체인 최적화 엔진(Off-Chain Optimization Engine)의 2계층 아키텍처를 제시한다. 또한 순수 Python 시뮬레이터(`microstable.py`)에서 Solana devnet의 Rust/Anchor 배포로 이어지는 구현 경로를 포함해 설계 목표, 수학적 정식화, 보안 모델을 설명한다.

## 2. 서론(Introduction)

비트코인은 통화 규칙(Monetary Rules)을 기관의 재량이 아닌 투명한 프로토콜 로직(Protocol Logic)으로 구현할 수 있음을 보여주었다. 한편 microgpt 같은 교육용 소형 구현은 명시적이고 작은 코드에서도 복잡한 동작이 창발할 수 있음을 입증했다.

microstable은 이 두 흐름을 결합한다. 메커니즘은 작고 검증 가능하게 유지하되, 시장 피드백을 반영해 내부 파라미터를 **제약되고 감사 가능한 방식**으로 적응시킨다.

### 2.1 문제의식(Motivation)

현재 스테이블코인 설계는 대체로 세 범주로 나뉜다.

1. **법정화폐 담보형 중앙 발행사(Fiat-backed centralized issuers)**: 단기 안정성은 높지만 수탁·동결·규제 집중 리스크가 크다.
2. **초과담보형 탈중앙 시스템(Over-collateralized decentralized systems)**: 견고하나 자본효율이 낮고, 거버넌스 조정이 시장 변화보다 느린 경우가 많다.
3. **알고리즘형/부분 알고리즘형(Algorithmic or partially algorithmic designs)**: 자본효율을 지향하지만 역사적으로 반사적 붕괴(Reflexive Collapse)에 취약했다.

공통된 약점은 파라미터 경직성(Parameter Rigidity)이다. 위험 임계치, 수수료 곡선, 담보 가중치는 수동·간헐적으로 갱신되는 반면 시장 상태는 연속적으로 변화한다.

### 2.2 핵심 명제(Thesis)

microstable의 핵심 명제는 다음과 같다. 스테이블코인 프로토콜은 결정론적·투명성을 유지하면서도 **작고 경계가 있는 경사 기반 업데이트(Bounded Gradient-Based Updates)**를 고빈도로 수행할 수 있다. 즉, 주기적 수동 거버넌스 대신 페그 품질(Peg Quality), 지급여력 마진(Solvency Margin), 담보 다변화(Collateral Diversification), 시장 충격비용(Market Impact Cost)을 명시적 목적함수로 둔다.

## 3. 배경(Background)

### 3.1 알고리즘형 스테이블코인 실패의 교훈 (UST/Luna)

UST/Luna 붕괴는 세 가지 구조적 실패 모드를 드러냈다.

- 신뢰 충격(Confidence Shock) 하에서의 반사적 민트/번 피드백 루프(Reflexive Mint/Burn Feedback Loop)
- 유동성 증발(Liquidity Evaporation)과 슬리피지 증폭(Slippage Amplification)
- 급격한 디페그 상황에서의 하드 안전제약(Hard Safety Constraints) 부재

핵심 교훈은 단순히 “알고리즘형은 나쁘다”가 아니라, **강한 회로차단기 없이 무제한 반사성(Unbounded Reflexivity)을 허용하면 치명적**이라는 점이다.

### 3.2 기존 접근(Existing Approaches)

- **DAI (MakerDAO)**: 초과담보 부채 포지션과 보수적 위험관리로 회복탄력성은 높지만, 파라미터 갱신이 거버넌스 중심이라 시장 미시구조 변화보다 느릴 수 있다.
- **FRAX (역사적 하이브리드 모델)**: 부분담보 + 알고리즘 요소로 적응성을 추구했지만, 시장 신뢰와 외부 유동성에 크게 의존한다.
- **mStable**: 스테이블 자산 바스킷 집계를 통해 노출 다변화와 스왑 효율을 강조한다.

microstable은 바스킷 다변화와 초과담보 규율을 계승하면서, 미분가능 파라미터 적응(Differentiable Parameter Adaptation)을 1급 원리로 도입한다.

## 4. 시스템 설계(System Design)

### 4.1 다중 담보 바스킷(Multi-Collateral Basket)

담보 집합(Collateral Set)을 다음과 같이 둔다.

$$
\mathcal{C} = \{c_1, c_2, \dots, c_n\}
$$

바스킷 가중치(Basket Weights)는

$$
\mathbf{w}_t = (w_{1,t}, \dots, w_{n,t}), \quad w_{i,t} \ge 0, \quad \sum_i w_{i,t} = 1.
$$

담보 가치는 오라클 가격(Oracle Price)으로 평가하고, 자산별 위험계수(Asset-Specific Risk Coefficient)에 따른 헤어컷(Haircut)을 반영한다.

#### 4.1.1 현실적 바스킷 구성 원칙(Recent Market Reality)

최근 시장 구조를 반영하면, 글로벌 스테이블코인 유통의 대부분은 달러계(USD-denominated)이며 비중은 약 99% 수준이다. 유로계(EUR)는 소규모로 존재하지만 원화계(KRW)·위안화계(CNY) 스테이블코인은 사실상 유의미한 시장 깊이(Market Depth)를 형성하지 못했다.

따라서 초기 설계의 실무적 우선순위는 **통화 분산(Currency Diversification)** 자체보다, 다음 두 축이다.

1. **발행사 분산(Issuer Diversification)**
2. **자산군 분산(Asset-Class Diversification)**

예시 바스킷:

- `USDC + USDT + DAI + PAXG + wBTC + EURC`

이는 달러 스테이블 중심 유동성을 활용하면서도, 금 토큰(PAXG)과 비트코인 래핑 자산(wBTC), 유로 노출(EURC)을 통해 스트레스 국면의 상관위험(Correlation Risk)을 완화하려는 절충안이다.

### 4.2 미분가능 프로토콜 파라미터(Differentiable Protocol Parameters)

파라미터 벡터(Parameter Vector)를 다음과 같이 정의한다.

$$
\boldsymbol{\theta}_t = [\text{targetCR}_t, \text{mintFee}_t, \text{redeemFee}_t, \mathbf{w}_t, \ldots]
$$

여기서 각 성분은 시뮬레이터의 경량 자동미분 프리미티브(`Value` 클래스)로 표현된다. 구현의 목표는 프레임워크 복잡성보다 가시성(Inspectability)이다.

### 4.3 경사하강 기반 자기진화 리밸런싱(Self-Evolving Rebalancing via Gradient Descent)

각 리밸런싱 에폭(Rebalance Epoch)에서 최적화기는 다음을 계산한다.

$$
\mathbf{g}_t = \nabla_{\boldsymbol{\theta}} \mathcal{L}_t
$$

그리고 가능한 경계로 투영(Projection)하는 업데이트(예: Adam)를 적용한다.

$$
\boldsymbol{\theta}_{t+1} = \Pi_{\Omega}\left(\boldsymbol{\theta}_t - \alpha_t \cdot \text{AdamStep}(\mathbf{g}_t)\right)
$$

여기서 $\Pi_{\Omega}$는 제약(수수료 범위, 담보 한도, 단체(simplex) 가중치, 최소 담보비율(Collateral Ratio))을 강제한다.

### 4.4 손실함수 설계(Loss Function Design)

대표 목적함수는 다음과 같다.

$$
\mathcal{L}_t =
\lambda_p (p_t - 1)^2
+ \lambda_{cr} \max(0, CR_{\min} - CR_t)^2
+ \lambda_{vol}\, \mathrm{Var}(\Delta NAV_{t:t+H})
+ \lambda_{turn}\, \|\mathbf{w}_t - \mathbf{w}_{t-1}\|_1
+ \lambda_{conc}\, \sum_i w_{i,t}^2
+ \lambda_{orc}(1-q_t)^2
$$

여기서:

- $p_t$: 프로토콜 토큰 시장가격(Market Price)
- $CR_t$: 유효 담보비율(Effective Collateral Ratio)
- $NAV$: 바스킷 순자산가치(Net Asset Value)
- $q_t$: 오라클 신뢰점수(Oracle Confidence Score)
- $\lambda_*$: 조정 가능한 위험선호 계수(Tunable Risk Preference Coefficients)

해석:

- 페그를 1에 가깝게 유지한다.
- 과소담보(Under-Collateralization)를 강하게 벌점화한다.
- 경로 변동성과 턴오버를 낮춘다.
- 단일 발행사/자산 집중을 회피한다.
- 오라클 신뢰도 하락 시 위험선호를 자동으로 보수화한다.

### 4.5 회로차단기(Circuit Breakers)

microstable은 “연속 최적화만으로 동작하는 시스템”이 아니다. 본질은 **하드 가드레일(Hard Guardrails) 내부의 최적화**다.

회로차단기 분류:

1. **디페그 차단기(Depeg Breaker)**: 자산 디페그가 임계치 $\delta$를 넘어 $\tau$ 기간 지속되면 해당 자산의 최대 비중을 낮추고 민트 확장을 일시 중지한다.
2. **담보 스트레스 차단기(Collateral Stress Breaker)**: 스트레스 시뮬레이션에서 예상 $CR$이 안전 하한을 위협하면 targetCR을 인상하고 민트 경로를 조인다.
3. **오라클 차단기(Oracle Breaker)**: 피드 괴리/지연이 허용 범위를 넘으면 최적화 업데이트를 동결하고 보수적 정적 프로파일로 전환한다.
4. **유동성 차단기(Liquidity Breaker)**: 리밸런싱 슬리피지 추정치가 상한을 초과하면 재배치를 다중 에폭으로 분산한다.

### 4.6 바스킷 자동 편입/퇴출 레벨(Automation Levels)

최근 논의 내용을 반영해 자동화 범위를 3단계로 구분한다.

- **Lv1 (비중 자동화)**: 자산 목록은 고정하고 가중치만 자동 조정한다. 가장 보수적이며 초기 운영 권장안.
- **Lv2 (조건부 편입/퇴출)**: 사전 정의된 조건(유동성, 디페그 이력, 발행사 리스크 점수 등)을 충족할 때만 자동 편입/퇴출을 허용한다.
- **Lv3 (완전 자율 편입/퇴출)**: 에이전트가 신규 자산 탐색·평가·편입을 전면 자동 수행한다.

정책 권고: **Lv3은 현재 단계에서 비권장**이다. 모델 리스크(Model Risk), 데이터 독성(Data Poisoning), 책임 경계 불명확성(Accountability Ambiguity)이 크므로, 실배포에서는 Lv1~Lv2와 강한 거버넌스 경계(Governance Boundary)를 유지해야 한다.

## 5. 아키텍처(Architecture)

### 5.1 온체인 vs 오프체인 책임 분리(On-Chain vs Off-Chain Responsibilities)

**온체인(결정론적 커널, deterministic kernel)**
- 담보 잔고의 수탁/회계(Custody/Accounting)
- 민트/리딤 정산 규칙(Mint/Redeem Settlement Rules)
- 경계와 불변식(Invariants) 강제
- 회로차단기 상태머신(State Machine)
- 제안된 파라미터 업데이트의 수락/거절

**오프체인(최적화 계층, optimization layer)**
- 오라클·시장 텔레메트리 수집(Ingest)
- 그래디언트 및 후보 업데이트 계산
- 변화 폭이 제한된 서명 제안(Signed, Bounded Update Proposal) 생성
- 키퍼 네트워크(Keeper Network)를 통한 제출

이 분리는 정산의 결정론(Deterministic Settlement)을 유지하면서, 온체인 비용 폭증 없이 고급 계산을 가능하게 한다.

### 5.2 Solana를 선택한 이유(Why Solana)

Solana 선택 근거:

- 빈번한 리밸런싱 체크포인트를 처리할 높은 처리량(High Throughput)
- 반복적 소규모 업데이트에 유리한 낮은 트랜잭션 비용(Low Cost)
- 준실시간 안전조치를 지원하는 빠른 최종성(Fast Finality)
- 성숙한 오라클 생태계(예: Pyth / Switchboard 패턴)

프로토콜은 Solana 프로그램 + PDA + crank/keeper 실행 구조에 자연스럽게 매핑된다.

## 6. 보안 분석(Security Analysis)

### 6.1 그래디언트 조작 공격(Gradient Manipulation Attacks)

공격자는 입력 데이터를 왜곡해 최적화가 자신에게 유리한 할당으로 이동하도록 유도할 수 있다.

완화책(Mitigations):
- 클리핑 오차(Clipped Errors)와 다중 윈도우 통계(Multi-window Statistics)를 활용한 강건 손실항(Robust Loss Terms)
- 에폭별 최대 파라미터 변화량 제약(Max Per-Epoch Delta Constraints)
- 괴리 검사를 포함한 오라클 앙상블(Ensemble Oracles)
- 대규모 업데이트에 대한 지연 활성화/2단계 커밋(Delayed Activation / Two-Step Commit)

### 6.2 담보 리스크(중앙화 스테이블코인 동결)

바스킷 구성 자산은 발행사 동결·제재 위험을 내포할 수 있다.

완화책:
- 손실함수/제약식 내 자산별 동결 위험 점수 반영
- 발행사 집중도 하드 캡(Hard Issuer Concentration Caps)
- 동결·의심 담보 비중을 줄이는 비상 마이그레이션 프로파일(Emergency Migration Profile)
- 비영향 준비금 우선의 상환 정책(Redemption Policy)

### 6.3 오라클 리스크(Oracle Risk)

실패 모드: 가격 정체(Stale Price), 피드 조작(Manipulated Feed), 가용성 저하(Liveness Loss)

완화책:
- 신뢰도 임계치를 둔 다중 소스 중앙값(Median-of-Sources + Confidence Threshold)
- 정체성(Staleness)·하트비트(Heartbeat) 검증
- 보수적 정적 파라미터로의 자동 폴백(Fallback)
- 온체인에 노출되는 명시적 “oracle degraded” 상태

### 6.4 데스 스파이럴 방지(Death Spiral Prevention)

본 프로토콜은 설계 차원에서 스트레스 상황의 반사적 팽창을 억제한다.

- 페그 < 임계치일 때 민트 스로틀링(Mint Throttling)
- 변동성 급등 시 동적 담보비율 인상(Dynamic CR Increases)
- 뱅크런 동학 완화를 위한 상환 큐 제어(Redemption Queue Controls)
- Phase 1/2에서 무제한 내생 거버넌스 토큰 반사성(Unbounded Endogenous Reflexivity) 미채택

## 7. 시뮬레이션 결과(자리표시자) (Simulation Results - Placeholder)

본 절은 `microstable.py`에서 생성될 정량 결과를 위한 자리표시자다.

보고 예정 지표:

- 페그 오차(Peg Error): $|p_t-1|$의 MAE / RMSE
- 꼬리위험(Tail Risk): 95% / 99% 최악 괴리
- 지급여력 통계(Solvency Statistics): 최소/중앙값 $CR_t$
- 턴오버 및 거래비용 프록시(Transaction-Cost Proxy)
- 단일/다중 담보 디페그 충격 시 최대낙폭(Drawdown)
- 스트레스 이벤트 이후 회복시간(Recovery Time)

시나리오 세트:

1. 정상 저변동성 시장
2. 단일 담보 디페그(예: USDT 스트레스)
3. 다중 담보 상관 디페그
4. 오라클 장애/정체 피드
5. 적대적 그래디언트 조작 시도

## 8. 기존 접근과의 비교(Comparison with Existing Approaches)

| 차원(Dimension) | DAI 유사 | FRAX 유사(역사적 하이브리드) | mStable형 바스킷 | microstable |
|---|---|---|---|---|
| 담보화(Collateralization) | 초과담보 | 분할/하이브리드 | 바스킷 집계 | 바스킷 + 적응형 CR |
| 파라미터 업데이트 | 거버넌스 에폭 중심 | 정책/컨트롤러 의존 | 대체로 규칙/정적 | 경사 기반·경계 제한·고빈도 |
| 다변화(Diversification) | 중간 | 중간 | 스테이블 바스킷에서 높음 | 높음 + 위험인식 집중도 벌점 |
| 반사성 리스크(Reflexivity Risk) | 낮음 | 중간/높음(모델 의존) | 낮음 | 차단기 + 경계로 통제 |
| 오라클 의존도 | 높음 | 높음 | 중간/높음 | 높음(오라클 신뢰 벌점으로 완화) |
| 1차 혁신성(Primary Novelty) | 보수적 CDP 모델 | 자본효율 실험 | 바스킷 UX/자본 라우팅 | 미분가능 자기진화 정책 |

## 9. 한계 및 향후 과제(Limitations & Future Work)

1. **모델 리스크(Model Risk)**: 손실함수 오설계 시 잘못된 목표를 최적화할 수 있다.
2. **데이터 리스크(Data Risk)**: 최적화 품질은 오라클 품질·지연에 상한이 있다.
3. **해석가능성(Interpretability)**: 잦은 파라미터 갱신은 가시성 체계가 약하면 인간의 이해가능성을 떨어뜨린다.
4. **거버넌스 경계(Governance Boundary)**: 허용 자산, 하드 캡 등은 명시적 거버넌스 판단으로 남겨야 한다.
5. **적대적 공진화(Adversarial Co-adaptation)**: 공격자는 결정론적 업데이트 로직에 맞춰 전략을 공진화할 수 있다.

향후 과제:
- 불변식 강제 로직의 정형검증(Formal Verification)
- 강건 최적화(Robust Optimization: CVaR, 적대적 학습, 분포이동 테스트)
- 오프체인 최적화 실행에 대한 암호학적 증명/증언(Cryptographic Attestations)
- 표준화된 위험 정규화를 갖춘 멀티체인 담보 추상화(Multi-chain Collateral Abstraction)

## 10. 결론(Conclusion)

microstable의 제안은 단순하다. 스테이블코인의 규칙은 명시적이어야 하지만 정적일 필요는 없다. 프로토콜은 정산의 결정론을 유지한 채, 투명한 경사 기반 업데이트로 경계가 설정된 위험 파라미터를 적응시킬 수 있다.

프로젝트는 의도적으로 작게 시작한다. 전체 메커니즘을 단일 파일에서 점검 가능한 의존성 없는 Python 시뮬레이터로 출발하고, 스트레스 환경에서의 견고성이 입증될 때에만 Solana로 확장한다. 이때도 엄격한 불변식 검증과 보수적 롤아웃 통제를 전제로 한다.

요약하면 microstable은 다음 공학 원칙을 따른다. 알고리즘은 이해 가능해야 하고, 안전 레일은 단단해야 하며, 최적화는 지급여력의 하인이어야지 대체재가 되어서는 안 된다.

## 11. 참고문헌(References)

1. S. Nakamoto, *Bitcoin: A Peer-to-Peer Electronic Cash System*, 2008.  
2. A. Karpathy, *micrograd / microgpt educational implementations* (public repositories and lectures).  
3. MakerDAO Documentation, *DAI and Collateral Risk Framework*.  
4. FRAX Documentation and historical design notes (fractional-algorithmic model evolution).  
5. mStable Documentation, *Basket-based stable asset design*.  
6. Post-mortem analyses of UST/Luna collapse (2022) from industry research reports.
