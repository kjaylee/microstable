# microstable: 자기진화형(Self-Evolving) 에이전트 네이티브(Agent-Native) 다중 담보 스테이블코인 프로토콜

**버전(Version)**: Draft v0.3  
**상태(Status)**: 연구 백서(Research Whitepaper, Educational / Hobby Project)  
**런타임 아키텍처(Runtime Architecture)**: Solana 온체인 프로그램(Anchor/Rust) + Rust 오프체인 키퍼 데몬  
**프로그램 ID (Devnet)**: `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`

> "프로토콜은 작고 검증 가능하게 유지하고, 적응은 경계(bound) 안에서 감사 가능하게 만든다."

---

## 1. 초록(Abstract)

스테이블코인은 핵심 결제 인프라이지만, 담보 집중·정책 반응 지연·오라클 열화·긴급 제어 취약성 같은 실패 패턴이 반복된다. **microstable**은 이를 위해 “온체인 결정론 + 오프체인 제한적 적응 + 하드 안전장치” 구조를 제안한다.

Draft **v0.3**는 운영 방향을 다음처럼 명확히 한다.

- **Anchor/Rust 기반 Solana 온체인 커널**,
- 오라클/리밸런스/모니터링을 담당하는 **Rust 키퍼 데몬**,
- Python 시뮬레이션은 운영 컴포넌트가 아닌 **아카이브된 교육·검증 자산**,
- zero-finding 보안 사이클 결과 반영,
- 온체인 명령 표면 및 devnet 식별자 명시,
- 핵심 불변식 중심의 정형검증 힌트 정리.

## 2. 범위 및 고지(Scope and Disclaimer)

본 문서는 교육/취미 목적의 연구 백서이며, 투자·법률·수익 보장을 위한 문서가 아니다. 목표는 프로토콜 설계의 명확성, 안전성 추론, 재현 가능한 엔지니어링이다.

## 3. 아키텍처(v0.3)

### 3.1 온체인 커널: Solana + Anchor/Rust

온체인 프로그램은 다음의 단일 진실원천(Source of Truth)이다.

- 수탁/회계 상태,
- mint/redeem 상태 전이,
- 회로차단기(CB) 상태머신,
- 키퍼 제안의 경계 내 수락,
- 역할·서명 범위 기반 권한 검증.

### 3.2 오프체인 런타임: Rust 키퍼 데몬

키퍼는 정책 경계를 준수하는 오프체인 계산을 수행하고, 감사 가능한 의도(intent)를 온체인에 제출한다.

### 3.3 Python 시뮬레이션 상태

`simulation/`의 Python 모델은 **아카이브된 교육/검증 하네스**로 유지되며, 운영 런타임 구성요소가 아니다.

## 4. 프로토콜 모델(요약)

프로토콜 파라미터를 \(\theta_t\), 안전 가능 집합을 \(\Omega\)라 하면:

$$
\theta_{t+1}=\Pi_{\Omega}\left(\theta_t-\alpha_t\nabla\mathcal{L}_t\right)
$$

투영 \(\Pi_{\Omega}\)는 cap/floor/simplex/수수료/CB 상태 같은 하드 제약을 강제한다. 즉, 최적화는 지급여력(solvency)과 라이브니스(liveness)를 침해할 수 없다.

## 5. 온체인 Instruction 표면(13)

v0.3 온체인 instruction 표면(13개 엔트리포인트):

1. `initialize`
2. `migrate_legacy_state`
3. `update_oracle`
4. `update_oracle_pyth`
5. `set_pyth_feed`
6. `mint`
7. `redeem`
8. `commit_rebalance`
9. `rebalance`
10. `activate_circuit_breaker`
11. `recover_circuit_breaker`
12. `emergency_shutdown` / `resume`
13. `rotate_keeper_set`

## 6. 키퍼 데몬 설계(Rust)

키퍼 데몬은 4개 핵심 모듈로 구성된다.

1. **oracle 모듈**: 다중 소스 데이터 수집/정규화, freshness 검사, 신뢰도 기반 점수화.
2. **rebalance 모듈**: 정책 제약 하에서 bounded proposal 계산 및 commit/reveal 호환 트랜잭션 준비.
3. **monitor 모듈**: peg, CR, 유동성, 오라클 품질, breaker 트리거를 관측하고 결정론적 경보 발행.
4. **watchdog 모듈**: 키퍼 프로세스 상태, 작업 마감시간, 안전 fallback 전이를 감시.

### 6.1 Dual-RPC 검증과 허용오차 비교

키퍼는 primary/secondary RPC를 교차 조회하고, 허용오차(tolerance) 규칙으로 상태 응답을 비교해 분기, 지연, 일시적 포크 불일치를 탐지한다.

### 6.2 `SecondaryRpcMode`

secondary RPC 상태는 3단계 모드로 명시된다.

- **normal**: primary + secondary 모두 정상/수렴,
- **degraded**: secondary 불안정 또는 불일치(보수 정책·낮은 cadence 적용),
- **no-secondary**: secondary 부재(최보수 모드 + 에스컬레이션 로그).

### 6.3 적응형 확인 윈도우(Adaptive confirm windows)

제출 후 확인 대기 구간은 네트워크 상태(슬롯 지연, 커밋 지연, 최근 finalization 패턴)에 맞춰 적응적으로 조정된다. 이를 통해 과도한 재시도와 위험한 낙관 가정을 동시에 줄인다.

### 6.4 빌드 증명과 공급망 통제

- `Cargo.lock` 고정 기반 컴파일 시점 증명(attestation),
- 재현 가능한 빌드 점검,
- 의존성 pinning + 리뷰 게이트,
- 키퍼 배포 산출물의 출처(provenance) 통제.

## 7. 보안 사이클 결과(v0.3)

microstable은 Purple/Red/Blue/Crimson 방식의 반복 보안 사이클을 완료했다. 최신 사이클 기준 결과는 **6개 라운드 ZERO FINDINGS**이다.

추가 검증 상태:

- **통합 테스트 38개 전부 통과**,
- 현재 추적 주기 기준 미해결 치명 이슈 없음,
- 적대적 테스트는 상시 운영 요구사항으로 유지.

## 8. Devnet 배포

- **Program ID**: `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`
- **MSTB mint**: `EZUwC88f1s3k9prgv5DGY6wML8giBqdpRxoA2rLtGA6R`

위 식별자는 devnet 실험 및 통합 테스트의 기준 엔드포인트다.

## 9. 정형검증 힌트(Formal Verification Hints)

v0.3에서 우선 검증할 정형 속성 후보:

1. **지급여력 불변식**: 승인된 상태 전이 하에서 부채가 위험조정 담보를 초과하지 않음.
2. **가중치 simplex 불변식**: 모든 업데이트 경로에서 담보 가중치는 음수가 아니고 합이 1.
3. **파라미터 경계 불변식**: 수락된 모든 파라미터 변화는 epoch별/전역 경계를 만족.
4. **CB 라이브니스**: 복구 조건 충족 시 breaker 상태에서 데드락 없이 정상 진행이 가능.

이 속성들은 상태머신 핵심 경로에 대해 실행 가능한 불변식/모델체킹 대상으로 정식화될 수 있다.

## 10. Open Agent Economy(OAE) + AIG

microstable은 허가 없는 참여(permissionless participation)를 유지하되, 책임성(accountability) 장치를 결합한다.

- permissionless 등록과 역할 분화,
- stake/reputation 기반 참여,
- 토너먼트형 제안 경쟁,
- 품질/안전 저하 시 단계적 제한을 거는 **Agent Intelligence Gate(AIG)**.

목표는 개방성과 안전성을 동시에 만족하는 에이전트 경제 구조다.

## 11. Agent Integration 및 MCP

프로토콜 연동 계층에는 MCP 도구가 배포되어 있다.

- npm 패키지: **`microstable-mcp-server@0.1.0`**,
- 목적: 외부 에이전트 시스템을 위한 machine-friendly 프로토콜 연산 제공,
- 정책 목표: 자동화 친화적 인터페이스 + 감사 가능한 제어 경계.

## 12. 결론(Conclusion)

microstable v0.3는 기존 명제를 더 명확한 구현 형태로 정리한다.

- 온체인 정산/불변식은 Solana Anchor/Rust,
- 오프체인 적응 로직은 강화된 Rust 키퍼,
- Python 시뮬레이션은 교육·검증용 아카이브,
- 보안 사이클 증거와 통합 테스트 상태를 명시적으로 공개.

프로젝트는 계속해서 점진적·검증 가능·안전 우선 원칙을 유지한다.

## 13. 참고문헌(선별)

1. S. Nakamoto, *Bitcoin: A Peer-to-Peer Electronic Cash System*, 2008.  
2. Solana 및 Anchor 공식 문서.  
3. Pyth 네트워크 문서.  
4. 스테이블코인 실패 사례 및 리스크 제어 관련 공개 문헌.  
5. microstable 내부 사양(OAE, AIG, keeper, security cycle reports).
