# Microstable Security Audit — Purple+Red v8 (MAX Resolution)

- Date: 2026-02-23 (KST)
- Auditor: Purple+Red v8 (MAX)
- Scope:
  - On-chain: `solana/programs/microstable/src/lib.rs`
  - Keeper daemon: `solana/keeper/src/*.rs` (요청된 전 파일)
  - Deployment: MiniPC `/home/spritz/microstable-keeper/*`, PM2 runtime
  - Agent-targeted attack surface (AIG/Tournament/OAE paths)
- Prior rounds reviewed: `docs/purple-keeper-v6-report.md`, `docs/purple-team-v2-report.md`, `docs/unimplemented-audit.md` 외 `docs/` 내 보안 리포트

---

## Finding: MSV8-001
- Severity: CRITICAL
- Component: `solana/programs/microstable/src/lib.rs:1642-1715, 2221-2230`
- Attack Vector: 
  `devnet_force_reinit`가 "DEVNET ONLY" 주석과 달리 mainnet/devnet 구분 없이 컴파일되어 있으며, 호출 시 protocol/circuit 상태를 강제로 초기화하고 keeper_set을 재설정한다. `TRUSTED_INITIALIZER` 키 탈취/오용 시 단일 트랜잭션으로 거버넌스 장악 가능.
- Impact:
  프로토콜 전체 파라미터/권한(keeper_set) 탈취, 공급량/회로차단기 상태 리셋, 이후 privileged instruction(oracle/rebalance/emergency) 임의 실행 가능.
- PoC:
  - `devnet_force_reinit(...)`는 클러스터 체크(`Clock::cluster_type` 등)나 feature gate 없음 (`1642-1715`).
  - 계정 제약도 PDA seeds 강제가 아니라 `UncheckedAccount` (`2221-2230`).
  - 함수 내부에서 상태를 강제 재작성:
    - `protocol.keeper_set = new_keeper_set` (`1675`)
    - `total_supply = 0` (`1673`)
    - `write_anchor_account(...)`로 직접 overwrite (`1684`, `1712`)
- Remediation:
  1) 프로덕션 빌드에서 해당 instruction 제거 (`#[cfg(feature="devnet-admin")]`),
  2) 최소한 cluster hard-gate + timelock + 2-of-3(or 3-of-3) multisig,
  3) `DevnetForceReinit` 계정에 canonical PDA seed 제약 강제,
  4) initializer 키 롤오버 및 HSM/air-gapped 서명 정책 적용.

## Finding: MSV8-002
- Severity: HIGH
- Component: `solana/keeper/src/wire.rs:203-217`, `solana/keeper/src/rebalance.rs:279-295`, `solana/programs/microstable/src/lib.rs:1212-1239, 2145-2158`, `solana/keeper/src/main.rs:166-208, 350-353`
- Attack Vector:
  keeper가 `commit_rebalance` 트랜잭션을 잘못 구성(필수 계정 누락)하여 commit 단계가 구조적으로 실패한다. 공격자는 편차(deviation)를 임계치 이상으로 유도하면(예: 오라클/TVL 변화) keeper가 해당 실패를 반복해 연속 실패 한도 초과로 프로세스 종료.
- Impact:
  리밸런스 경로 가용성 상실(DoS), max consecutive failure 도달 시 데몬 종료로 운영 중단.
- PoC:
  - 온체인 `CommitRebalance`는 필수 계정 5개 요구:
    `protocol_state`, `agent_record`, `submitting_agent`, `keeper_one`, `keeper_two` (`2145-2158`).
  - 실제 tx builder는 3개만 넣음:
    `protocol_state`, `keeper_one`, `keeper_two` (`wire.rs:203-217`).
  - 이 builder를 그대로 호출해 전송 (`rebalance.rs:279-295`).
  - step 실패는 `failed_steps.push("rebalance")`로 누적 (`main.rs:350-353`), 누적 실패시 프로세스 종료 (`166-208`).
- Remediation:
  1) wire ABI를 on-chain Accounts와 동기화(필수 계정/signer 모두 포함),
  2) commit/reveal e2e 테스트 추가(실제 program invoke까지),
  3) step-level failure가 전체 프로세스 종료로 직결되지 않도록 circuit-breaker형 backoff 도입.

## Finding: MSV8-003
- Severity: HIGH
- Component: `solana/keeper/src/agent_loop.rs:211-240, 254-273, 299-301`, `solana/programs/microstable/src/lib.rs:72, 400-406, 414-420`
- Attack Vector:
  AIG/Tournament 참가자 선정이 외부 agent가 아니라 keeper key 기반으로 고정되고(최대 2명), 점수/티어 갱신은 단일 keeper 서명만으로 가능하다. 결과적으로 keeper 운영자가 self-dealing/시빌 형태로 점수와 티어를 독점 가능.
- Impact:
  Agent 신뢰도 체계 붕괴, tier 기반 권한(commit 제출 eligibility) 왜곡, OAE/AIG 점수 조작.
- PoC:
  - 참가자 소스: `keepers.iter().map(pubkey)` 후 `truncate(2)` (`211-229`).
  - 정해진 proposal만 제출 (`231-240`) 후 점수 tx 발행 (`254-273`).
  - AIG도 `select_candidate_agent = keepers.first()` (`299-301`), 이후 score tx 전송 (`122-150`).
  - 온체인 score/promote는 quorum이 아닌 단일 keeper membership만 검사 (`400-406`, `414-420`).
  - stake 최소값이 `1 lamport` (`72`)라 시빌 비용이 사실상 0.
  - 운영 로그 실증(MiniPC): tournament winner가 반복적으로 동일 keeper key, 동일 key들에 `update_agent_score` 연속 전송 확인.
- Remediation:
  1) AIG/Tournament 참가자 풀을 keeper 키와 분리(등록 agent only),
  2) score/promote/demote를 keeper quorum(2-of-3)으로 상향,
  3) 경제적 시빌 방지 stake 상향 + slash 조건 강화,
  4) deterministic 내부 proposal 대신 외부 제출/commit-reveal 검증 도입.

## Finding: MSV8-004
- Severity: MEDIUM
- Component: `solana/keeper/keeper2.json:1`, `solana/keeper/keeper3.json:1`, `solana/keeper/config.devnet.json:5-10`
- Attack Vector:
  비밀키(64-byte secret array) 파일이 repo에 추적 상태로 존재하며, 기본 config에서 해당 파일 경로를 keypair 소스로 참조한다.
- Impact:
  키 재사용/오배치 시 즉시 signer 탈취. 운영자가 실수로 동일 키를 keeper_set에 반영하면 quorum 손실로 이어질 수 있음.
- PoC:
  - `keeper2.json`, `keeper3.json` 파일이 실제 secret key 배열 포함(파일 1라인).
  - `git ls-files`로 추적 확인.
  - `config.devnet.json`이 `keeper/keeper2.json`, `keeper/keeper3.json`을 로딩 목록에 포함 (`5-10`).
- Remediation:
  1) 해당 키 즉시 폐기/회전,
  2) git history에서 완전 제거(filter-repo/BFG),
  3) `.gitignore` + secret scanning(pre-commit/CI) 강제,
  4) 키는 KMS/HSM 또는 로컬 secure vault만 사용.

## Finding: MSV8-005
- Severity: MEDIUM
- Component: `solana/keeper/src/utils.rs:368-370, 394-409`
- Attack Vector:
  키파일 소유자 검증에 외부 명령 `id -u`를 PATH 검색으로 실행한다. 서비스 PATH가 오염되면(fake `id`), owner validation 우회 가능.
- Impact:
  의도한 키 소유권 검증 무력화 → 공격자 제공 keypair 로딩 위험 증가.
- PoC:
  - 검증 경로: `effective_uid()` 결과와 `metadata.uid()` 비교 (`368-370`).
  - `effective_uid()` 구현이 `Command::new("id").arg("-u")` 호출 (`395-398`).
  - 즉, `/tmp/id` 같은 PATH 하이재킹 바이너리로 uid 응답 위조 가능.
- Remediation:
  1) 외부 명령 호출 제거, `libc::geteuid()`/`nix::unistd::Uid::effective()` 사용,
  2) PATH 의존 제거 및 서비스 환경 고정,
  3) key loader 단위테스트에 PATH hijack 회귀 케이스 추가.

## Finding: MSV8-006
- Severity: HIGH
- Component: `MiniPC deployment runtime (PM2 jlist output)`
- Attack Vector:
  PM2 런타임에서 프로세스 환경변수가 평문으로 조회되며(`pm2 jlist`), 동일 호스트의 타 앱 민감값(API keys 등)이 노출된다. keeper와 동일 사용자/PM2 domain 공유 시 lateral movement 가능.
- Impact:
  호스트 단위 credential compromise, keeper 운영환경까지 연쇄 침해 위험.
- PoC:
  - MiniPC에서 `pm2 jlist` 실행 시 다수 민감 env 변수가 평문 출력됨(실측 확인; 보고서에는 값 비공개 처리).
- Remediation:
  1) keeper를 전용 OS user + 전용 PM2 home으로 격리,
  2) PM2 RPC/socket 접근권한 최소화,
  3) 민감값은 env 직접 주입 대신 secret manager/파일 권한 격리,
  4) 멀티테넌트 PM2 운영 금지.

---

## Notes (No new exploitable finding observed in this round)
- On-chain arithmetic overflow/underflow: 주요 연산은 `checked_*` / saturation 기반으로 방어됨.
- Reentrancy/CPI callback class: Solana execution model상 EVM형 재진입 벡터는 확인되지 않음.
- Oracle discriminator parsing: on-chain/keeper 모두 owner + 구조 검증 경로 존재.
- Keypair file permission checks: `O_NOFOLLOW`/mode checks 존재(단, MSV8-005의 PATH 의존 취약점은 별개).

