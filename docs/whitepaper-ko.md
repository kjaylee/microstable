# microstable: 자기진화형( Self-Evolving ) 에이전트 네이티브( Agent-Native ) 다중 담보 스테이블코인 프로토콜

**버전(Version)**: Draft v0.2  
**상태(Status)**: 연구 백서(Research Whitepaper, Educational / Hobby Project)  
**런타임 아키텍처(Runtime Architecture)**: Solana 온체인 프로그램 + Rust 오프체인 키퍼 데몬(Keeper Daemon)  
**프로그램 ID (Devnet)**: `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`

> "프로토콜은 작고 검증 가능하게 유지하고, 적응은 경계(bound) 안에서 감사 가능하게 만든다."

---

## 1. 초록(Abstract)

스테이블코인(Stablecoin)은 핵심 결제 인프라가 되었지만, 여전히 담보 집중(Collateral Concentration), 거버넌스 지연(Governance Latency), 오라클 취약성(Oracle Fragility), 정적 파라미터(Static Parameter) 문제를 반복한다. **microstable**은 이를 위해 “작고 검증 가능한 구조 + 경계가 있는 적응”을 제안한다.

Draft v0.2는 세 가지 확장을 포함한다. 첫째, 허가 없는 참여(Permissionless Participation)를 지원하는 **Open Agent Economy (OAE)**. 둘째, Tier 0→3 단계 검증을 수행하는 **Agent Intelligence Gate (AIG)**. 셋째, Solana devnet 배포, Pyth 연동, SPL Token E2E 검증, 그리고 Purple/Red/Crimson 보안 순환 결과를 반영한 실제 구현 진전이다.

핵심 원칙은 동일하다. 최적화(Optimization)는 지급여력(Solvency)의 대체재가 아니라 보조 수단이어야 한다.

## 2. 서론(Introduction)

Bitcoin은 통화 규칙을 프로토콜로 구현할 수 있음을 보여주었다. 동시에 작은 코드에서도 적응적 동작이 가능하다는 점은 modern AI tooling이 보여주었다. microstable은 이 두 흐름을 안정적 통화 프로토콜로 결합한다.

### 2.1 동기(Motivation)

스테이블코인은 이제 DeFi를 넘어 에이전트 경제(Agent Economy)의 결제 레이어가 되고 있다. 에이전트 중심 환경에서는 다음이 필수다.

- 기계가 검증 가능한 결정론(Determinism),
- 빠르지만 제한된 파라미터 적응,
- 스트레스 상황에서의 하드 안전장치(Hard Safety Guardrail),
- 개방성(Permissionless)과 책임성(Accountability)의 동시 달성.

### 2.2 핵심 명제(Thesis)

프로토콜 파라미터 $\theta_t$는 손실함수 $\mathcal{L}_t$를 따라 업데이트하되, 항상 안전 집합 $\Omega$로 투영한다.

$$
\theta_{t+1}=\Pi_{\Omega}\left(\theta_t-\alpha_t\nabla_{\theta}\mathcal{L}_t\right)
$$

즉, 적응은 허용하되 무제한 반사성(Unbounded Reflexivity)은 허용하지 않는다.

## 3. 배경(Background)

### 3.1 실패 사례의 교훈

UST/Luna는 다음을 보여줬다.

1. 신뢰 붕괴 시 반사적 민트/번 루프,  
2. 유동성 고갈과 슬리피지 증폭,  
3. 빠른 디페그 국면에서의 안전장치 부재.

### 3.2 기존 접근과 한계

- DAI 계열: 보수적이고 견고하지만 적응 속도가 느릴 수 있음.
- FRAX 계열(역사적 하이브리드): 유연하지만 신뢰 민감도 큼.
- mStable 계열: 바스킷 다변화 강점.

microstable은 바스킷 강점을 유지하면서 에이전트 네이티브(Agent-Native) 적응 제어를 추가한다.

### 3.3 에이전트 네이티브 맥락

에이전트 네이티브 프로토콜은 API 개방성만으로는 부족하다. 신원(Identity), 스테이킹(Staking), 평판(Reputation), 슬래싱(Slashing)이 결합된 운영 구조가 필요하다.

## 4. 시스템 설계(System Design)

### 4.1 담보 바스킷(Collateral Basket)

담보 집합 $\mathcal{C}=\{c_1,\dots,c_n\}$, 가중치 $w_{i,t}$에 대해:

$$
\sum_i w_{i,t}=1,\quad w_{i,t}\ge 0,
$$

그리고 자산별 상한 $w_i^{\max}$, 헤어컷(Haircut) 및 위험계수를 적용한다.

### 4.2 파라미터 벡터(Parameter Vector)

$$
\theta_t=[\text{targetCR}_t,\text{mintFee}_t,\text{redeemFee}_t,\mathbf{w}_t,\dots]
$$

각 업데이트는 스텝당 변화량 제한(Delta Cap)을 만족해야 한다.

### 4.3 손실함수(Loss Function)

$$
\mathcal{L}_t=
\lambda_p(p_t-1)^2
+\lambda_{cr}\max(0,CR_{\min}-CR_t)^2
+\lambda_{vol}\,\mathrm{Var}(\Delta NAV)
+\lambda_{turn}\|\mathbf{w}_t-\mathbf{w}_{t-1}\|_1
+\lambda_{conc}\sum_i w_{i,t}^2
+\lambda_{orc}(1-q_t)^2
$$

해석: 페그 유지, 과소담보 억제, 집중도·턴오버 감소, 오라클 저신뢰 구간 보수화.

### 4.4 최적화와 안전 투영

Gradient/Adam 업데이트는 다음을 모두 통과해야 반영된다.

- gradient clipping,
- bounded delta,
- simplex + cap projection,
- safety gate.

### 4.5 회로 차단기(Circuit Breaker)

- **CB-1**: 단일/국소 디페그 대응,
- **CB-2**: 시스템 스트레스 시 민팅 제한/중단,
- **CB-3**: 오라클 열화 시 보수 모드,
- **CB-4**: 수치 불안정 시 롤백.

### 4.6 유동성 실행 제약

수치 안정성(CB-4)과 시장 실행 품질(슬리피지, 턴오버 슬라이싱)은 분리해 관리한다.

## 5. 아키텍처(Architecture)

v0.2는 **2계층 구조(Two-Layer Architecture)**를 명시한다.

### 5.1 온체인(Solana)

- 수탁/회계,
- 민트/리딤 상태 전이,
- 불변식(Invariant) 강제,
- 회로차단기 상태머신,
- 파라미터 제안 수락/거절.

### 5.2 오프체인(Rust keeper daemon)

- 오라클 수집,
- 리밸런스 제안 계산,
- 모니터/워치독 루프,
- 키퍼 쿼럼 제출.

구현 경로: `solana/keeper/` (`microstable-keeper`).

### 5.3 Python 시뮬레이션 상태

`simulation/`의 Python 모델은 **아카이브(Archived) 참조/검증 자산**이며, 운영 런타임 컴포넌트는 아니다.

## 6. 보안 분석(Security Analysis)

v0.2 보안 분석은 가정 기반이 아니라 다회차 Purple/Red/Crimson 실측 결과를 반영한다.

### 6.1 실전에서 관측된 공격군

- 보상/회계 조작,
- 신원/권한 우회,
- 오라클 신선도/바인딩 악용,
- 토너먼트·시빌(Sybil) 보상 왜곡,
- 워치독 컨센서스 악용,
- NaN/Inf 숫자 독성 입력,
- 라이프사이클·거버넌스 레이스.

### 6.2 방어 원칙

1. 하드 인바리언트 우선(Hard Invariant First),
2. 다중 권한 검증(Layered Authorization),
3. 재생공격 방지(Replay Safety),
4. 경제 규칙 의미 일치(Slash/Reward Semantics),
5. 비정상 상태 보수 모드(Fallback Conservative Mode).

### 6.3 잔여 리스크

남은 주요 리스크는 경제적 그리핑(Economic Griefing)과 복합 엣지 케이스다. 따라서 보안은 일회성 감사가 아니라 순환 운영(Continuous Cycling)으로 다뤄야 한다.

## 7. Open Agent Economy

(OAE 문서 1~3절 기반)

### 7.1 참여 모델

- 허가 없는 등록(Permissionless Registration),
- 역할 분화(Optimizer / Monitor / Auditor / Liquidator),
- 스테이킹 + 평판 기반 운영.

### 7.2 Agent Registry

Agent Registry(PDA)는 stake, reputation, 제안/채택 이력, 상태(Active/Cooldown/Slashed/Deregistered)를 기록한다.

### 7.3 ACP (Agent Communication Protocol)

ACP v1은 JSON-RPC 형태의 서명 메시지(Signature Message) 기반으로 제안 제출, 이상 보고, 투표, 보상 청구를 표준화한다.

### 7.4 최적화 토너먼트(Optimization Tournament)

Commit-Reveal 호환 구조, copycat penalty, 최소 stake, stake-weighted reputation을 통해 경쟁형 제안 선택을 수행한다.

## 8. Agent Intelligence Gate

AIG는 “등록 가능”과 “운영 신뢰”를 분리해 단계적으로 검증한다.

### 8.1 Tier 0→3

- **Tier 0**: 역사 시나리오 시험,
- **Tier 1**: 100 epoch 샌드박스,
- **Tier 2**: 제한 권한 probation(최소 30 epoch),
- **Tier 3**: 정식 참여 + 상시 감시.

### 8.2 AgentScore

$$
\mathrm{Score}=0.35Q_{opt}+0.20Q_{lat}+0.20Q_{safe}+0.15Q_{adv}+0.10Q_{cons}
$$

점수 기반 승급/강등으로 저품질·악성 에이전트 영향을 줄인다.

### 8.3 통합 정책

AIG는 등록 시점과 운영 시점 모두 적용되어 OAE의 개방성과 안전성을 함께 유지한다.

## 9. Protocol Resilience

(Protocol gap priority matrix 기반)

### 9.1 10개 구조적 갭과 우선순위

| 갭(Gap) | 위험(Risk) | 우선순위(Priority) | 대응 방향 |
|---|---|---|---|
| Correlated collateral risk | CRITICAL | P1 | 상관 기반 리밸런싱 + 선제 cap 조정 |
| Collateral freeze risk | CRITICAL | P1 | 동결 감지 시 자동 강등/라우팅 |
| Bank run / redemption spiral | CRITICAL | P1 | 동적 상환 수수료 + 큐 정산 |
| Off-chain collusion | HIGH | P2 | 행동 기반 클러스터 탐지 |
| Governance plutocracy | HIGH | P2 | 엔티티 cap + 거버넌스 감쇠 |
| MEV / front-running | HIGH | P2 | 확장 commit-reveal + batch 정산 |
| CB cascading deadlock | CRITICAL | P1 | interaction graph + 강제 복구 순서 |
| Single-key upgrade risk | CRITICAL | P1 | multisig + timelock |
| Economic death spiral | CRITICAL | P1 | 경제적 하한(Economic Floor) |
| Information asymmetry | HIGH | P2 | 실시간 공시 + 감사 로그 |

### 9.2 해석

상위 P1 항목은 성능 최적화가 아니라 생존성(survivability)과 지급여력 보전의 핵심 제어 영역이다.

## 10. Adversarial Infrastructure

(Adversarial infra 문서 1~2절 기반)

### 10.1 위협 모델(Threat Model)

공격자는 고속, 대규모 병렬, 지속 운영, 적응형 전략(강화학습 포함), 스웜 협조를 가정한다.

### 10.2 Red/Blue 내장 구조

- **Red**: mutation/composition 기반 공격 생성, 스웜 실행, 진화형 탐색.
- **Blue**: 통계·그래프·행동 탐지, 자동 대응, 포렌식 시그니처, 적응형 방어.

### 10.3 항취성(Antifragility)

성공 공격은 즉시 서명·정책으로 환류되어 다음 라운드 방어력을 높인다.

$$
\mathrm{Immunity}=1-\frac{\text{successful attacks}}{\text{total attacks}}
$$

보고된 immunity score는 **1.0**이다.

## 11. Security Audit Results

### 11.1 연속 순환 방식(Continuous Cycling)

고정형 1회 감사가 아니라 Purple/Red 발견과 Blue 패치를 반복하는 순환형 방법을 사용했다.

캠페인 체인:

- Purple v1: **27 findings**
- Blue v2: **27 patched**
- Purple v2: **28 findings**
- Red v3: **16 successful / 36 attempts**
- Blue v3: **full patch cycle**
- Purple v3: **23 findings**
- Red v4: **13 successful / 24 attempts**
- Crimson: **20 successful / 27 attempts**

### 11.2 모듈별 테스트 총합

- **Core**: 71/71 PASS
- **Mega Stress**: 8000/8000 PASS
- **Open Agent Economy**: 115/115 PASS
- **Adversarial Infrastructure**: 100/100 PASS
- **Agent Intelligence Gate**: 54/54 PASS
- **Protocol Resilience**: 98/98 PASS

운영 탄력성 결과:

- Chaos engineering: **8/8 PASS**
- Degradation tests: **5/5 PASS**

## 12. 시뮬레이션 결과(Simulation Results)

본 절은 v0.1 Gate A + Monte Carlo 구조를 유지하고 mega stress 결과를 함께 제시한다.

### 12.1 Gate A 설정

- 범위: **100 seeds × 6 scenarios × 80 ticks**
- 기준:
  - peg MAE < 0.0015
  - CR violation < 1%
  - breaker false positive < 5%

### 12.2 Gate A 결과 (시나리오별 100회)

| Scenario | pass_count | fail_count | Gate A | peg MAE worst | CR_min worst (lowest) | CR violation worst | FP worst |
|---|---:|---:|---:|---:|---:|---:|---:|
| normal | 100 | 0 | PASS | 0.000366 | 1.201000 | 0.000000% | 0.000000% |
| single_depeg | 100 | 0 | PASS | 0.000460 | 1.201000 | 0.000000% | 0.000000% |
| multi_depeg | 100 | 0 | PASS | 0.001171 | 1.202422 | 0.000000% | 0.000000% |
| volatile | 100 | 0 | PASS | 0.000504 | 1.201000 | 0.000000% | 0.000000% |
| gradient_attack | 100 | 0 | PASS | 0.000389 | 1.201000 | 0.000000% | 0.000000% |
| oracle_failure | 100 | 0 | PASS | 0.000639 | 1.202554 | 0.000000% | 0.000000% |

Worst peg MAE: **0.001171** (`multi_depeg`).

### 12.3 Monte Carlo KPI (mean / median / p5 / p95 / worst)

#### (A) peg MAE

| Scenario | mean | median | p5 | p95 | worst |
|---|---:|---:|---:|---:|---:|
| normal | 0.000336 | 0.000336 | 0.000321 | 0.000353 | 0.000366 |
| single_depeg | 0.000405 | 0.000406 | 0.000378 | 0.000439 | 0.000460 |
| multi_depeg | 0.001113 | 0.001113 | 0.001083 | 0.001143 | 0.001171 |
| volatile | 0.000405 | 0.000406 | 0.000354 | 0.000457 | 0.000504 |
| gradient_attack | 0.000334 | 0.000334 | 0.000307 | 0.000366 | 0.000389 |
| oracle_failure | 0.000585 | 0.000586 | 0.000558 | 0.000614 | 0.000639 |

#### (B) CR_min

| Scenario | mean | median | p5 | p95 | worst (lowest) |
|---|---:|---:|---:|---:|---:|
| normal | 1.201000 | 1.201000 | 1.201000 | 1.201000 | 1.201000 |
| single_depeg | 1.201000 | 1.201000 | 1.201000 | 1.201000 | 1.201000 |
| multi_depeg | 1.202941 | 1.202961 | 1.202601 | 1.203325 | 1.202422 |
| volatile | 1.201000 | 1.201000 | 1.201000 | 1.201000 | 1.201000 |
| gradient_attack | 1.201000 | 1.201000 | 1.201000 | 1.201000 | 1.201000 |
| oracle_failure | 1.202944 | 1.202959 | 1.202689 | 1.203232 | 1.202554 |

### 12.4 Mega stress

- 범위: **80 scenarios × 100 Monte Carlo = 8,000 runs**
- 결과: **ALL PASS (8000/8000)**
- 최대 MAE: **0.02684**
- crash/NaN/Inf: **0**

## 13. Devnet Deployment

### 13.1 핵심 식별자

- Program ID: `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`
- MSTB mint: `EZUwC88f1s3k9prgv5DGY6wML8giBqdpRxoA2rLtGA6R`

### 13.2 Pyth 연동

다음 피드가 devnet에서 연동되었다.

- USDC/USD
- USDT/USD
- DAI/USD

### 13.3 SPL Token E2E

`solana/tests/devnet-e2e.ts`에서 migrate → oracle update → mint → redeem 흐름과 계정 일관성(accounting consistency)을 확인했다.

## 14. Agent Integration

### 14.1 MCP Server

- 패키지: `microstable-mcp-server@0.1.0` (npm)
- 목적: MCP 기반 외부 에이전트 연동.

### 14.2 ClawHub Agent Skill

- `microstable-agent` 스킬 프로파일을 통해 ACP 제출/이상 보고/상태 조회 연동.

### 14.3 설계 원칙

연동은 자동화 친화적이되 내부 규칙은 계속 inspectable/auditable 상태를 유지한다.

## 15. 비교(Comparison)

| Dimension | DAI-like | FRAX-like (historical hybrid) | mStable-style basket | microstable v0.2 |
|---|---|---|---|---|
| 담보 모델(Collateral model) | Over-collateralized | Fractional/hybrid | Basket aggregation | Basket + adaptive CR |
| 파라미터 갱신(Parameter updates) | Governance epochs | Controller dependent | Mostly static | Bounded gradient updates |
| 회로차단기 체계(CB formalism) | Moderate | Model-dependent | Limited | Explicit multi-CB machine |
| 에이전트 네이티브(Agent-native participation) | Low | Low/Medium | Low | **High (OAE + AIG)** |
| 적대적 순환(Adversarial feedback loop) | Limited | Limited | Limited | **Embedded Red/Blue loop** |
| 런타임 구조(Runtime architecture) | On-chain heavy governance | Mixed | App-layer routing | Solana kernel + Rust keeper |

## 16. 한계와 향후 과제(Limitations & Future Work)

1. 목적함수 오설계(Objective misspecification) 리스크,
2. 오라클/데이터 품질 의존,
3. 경제적 게임화(Economic gameability),
4. 시뮬레이션-온체인 동작 차이(Cross-layer divergence),
5. 지분 집중 기반 거버넌스 압력.

우선 과제:

- 핵심 상태 전이 정형검증(Formal Verification),
- 오라클 신뢰·신선도 강화,
- 권한 경로(auth path) 일관화,
- 상관 리스크 중심 스트레스 확장,
- 에이전트 행위 투명성 도구 강화.

## 17. 결론(Conclusion)

microstable v0.2의 핵심은 “완성 선언”이 아니라 “방법론 선언”이다.

- 메커니즘은 작게,
- 적응은 경계 안에서,
- 안전은 타협 없이,
- 보안 검증은 연속적으로.

즉, 개방형 에이전트 참여를 허용하되, 무제약 자동화가 아니라 검증 가능한 규칙 기반 자동화로 운영한다.

## 18. 재현성 및 참고문헌(Reproducibility & References)

### 18.1 재현성(Reproducibility)

- 본 백서 기준 커밋(commit): `f9f5dae`
- 주요 아티팩트:
  - `simulation/outputs/open-agent-economy-test-report.md`
  - `simulation/outputs/adversarial-agent-report.md`
  - `simulation/outputs/chaos/chaos-summary.md`
  - `simulation/outputs/chaos/degradation-test-results.json`
  - `simulation/outputs/mega-stress-report.md`

### 18.2 참고문헌(References)

1. S. Nakamoto, *Bitcoin: A Peer-to-Peer Electronic Cash System*, 2008.  
2. A. Karpathy, *micrograd/microgpt educational implementations*.  
3. MakerDAO 관련 문서와 리스크 프레임워크.  
4. FRAX 역사적 설계 문서.  
5. mStable 바스킷 설계 문서.  
6. UST/Luna 등 알고리즘형 스테이블코인 사후 분석 자료.  
7. microstable 내부 문서(OAE, AIG, gap analysis, adversarial infra, security reports).
