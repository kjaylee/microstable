# Microstable Week2 Devnet E2E Verification (Latest)

- 작성 시각(KST): 2026-02-25 21:58
- 대상 스펙:
  - `docs/week2-submission.md`
  - `docs/test-cases-week2.md`
- 실행 로그 루트: `docs/evidence/week2-e2e-20260225/`

---

## 1) Spec

Week2 마감 게이트의 필수 검증 범위:
1. Mint / Redeem / Register Agent UI + tx builder 동작 검증
2. Devnet 상태 검증 (Program executable, RPC health, 대시보드 접근)
3. 가능한 범위의 실 Devnet E2E 트랜잭션 검증
4. FAIL 항목 발생 시 `docs/index.html`, `docs/app.js` 즉시 수정 후 재검증

---

## 2) Plan

1. 문서 TC를 실행 가능한 명령/증거 경로 중심으로 구체화
2. 정적 검사 + Devnet 상태 검사 선실행
3. 대시보드 실동작(browser) 확인 및 스크린샷 확보
4. Node 스크립트로 Mint/Redeem/Register 실제 tx 시도 + 실패 원인 증거화
5. 실패 원인에 해당하는 프론트 수정 후 재검증
6. 최종 PASS/FAIL 판정표 + blocker + 다음 액션 작성

---

## 3) Test Cases (문서 TC + 사용자 시나리오 구체화)

`docs/test-cases-week2.md` 업데이트 완료:
- TC-W2-TX-003에 **agent escrow compatibility fallback**(legacy seed) 반영
- 실행 가능한 runbook/증거 경로(명령 + 파일) 추가

추가 사용자 시나리오(실행):
- US-01: Dashboard가 OFFLINE에서 LIVE로 정상 복구되는지
- US-02: Agent Arena에 UserPosition 계정이 섞여 표시되지 않는지(계정 discriminator 필터)

---

## 4) Task Breakdown

- T1. 정적/환경 체크
- T2. Devnet 상태 체크
- T3. Dashboard UI/접근성 체크 + 스크린샷
- T4. E2E 스크립트 실행(Mint/Redeem/Register)
- T5. 실패 항목 코드 수정(`docs/app.js`, TC 문서)
- T6. 재검증 및 판정표 작성

---

## 5) Implementation (실제 수정 사항)

### 5.1 `docs/app.js` 수정
1. **RPC 부트스트랩/쿼럼 안정화**
   - `rpc.ankr.com/solana_devnet` 제거(Unauthorized로 boot 실패 유발)
   - 동적 devnet 데이터 불일치로 인한 과도한 quorum fail 제거(대시보드 OFFLINE 방지)
2. **Agent registration 호환 fallback 추가**
   - 기본 `['v2:agent_escrow', wallet]` 실패 시 `['agent_escrow']` legacy PDA 재시도
3. **Agent Arena 데이터 정제**
   - `getProgramAccounts(dataSize=168)` 결과에서 `AgentRecord` discriminator 필터 적용
   - UserPosition 계정이 Arena에 섞이는 문제 제거

### 5.2 `docs/test-cases-week2.md` 수정
- TC-W2-TX-003 기대 동작을 현재 devnet 호환 로직 기준으로 업데이트
- 실행 runbook + evidence 파일 경로 명시

### 5.3 검증 스크립트 추가
- `scripts/week2-e2e-devnet-check.js`
  - Program 상태 체크
  - Mint/Redeem/Register tx 시도
  - 실패 시 on-chain 에러 로그를 구조화 JSON으로 저장

---

## 6) Verification

## 6.1 Devnet 상태

| 항목 | 결과 | 근거 |
|---|---|---|
| Program account executable | PASS | `program-account.json` (`executable: true`, owner `BPFLoaderUpgradeab1e...`) |
| RPC health | PASS | `rpc-health.json` (`result: ok`) |
| Dashboard 접근 | PASS | `dashboard-head.txt` (HTTP/2 200) |

## 6.2 TC 판정표

| TC ID | 판정 | 근거 |
|---|---|---|
| TC-W2-UI-001 Redeem Console 존재 | PASS | `screenshot-dashboard-live-after-rpc-fix.jpg` |
| TC-W2-UI-002 Agent Registration 존재 | PASS | `screenshot-dashboard-live-after-rpc-fix.jpg` |
| TC-W2-UI-003 Register link 기능 | PASS (정적/DOM) | `docs/index.html` href `#agentRegistrationPanel`, `verification.log` ID 체크 |
| TC-W2-WALLET-001 Connect/Disconnect | BLOCKED | 자동 브라우저에 Phantom 미연결(지갑 사용자 승인 필요) |
| TC-W2-WALLET-002 Balance refresh | BLOCKED | 동일 사유(지갑 미연결 상태) |
| TC-W2-TX-001 Mint tx | FAIL | `e2e-run-v2.log`, `e2e-result-v2.json`: `ConstraintMintMintAuthority (2016)` |
| TC-W2-TX-002 Redeem tx | BLOCKED | Mint 실패로 MSTB 미보유 (`No MSTB balance available for redeem test`) |
| TC-W2-TX-003 Register agent tx | FAIL | `e2e-run-v2.log`, `register-seed-probe.log`: `ConstraintSeeds agent_escrow (2006)` |
| TC-W2-FAUCET-001 SOL fallback | PARTIAL/BLOCKED | UI fallback 표시는 PASS, 실제 airdrop은 devnet faucet 429 (`verification.log`) |

## 6.3 사용자 시나리오 판정

| 시나리오 | 판정 | 근거 |
|---|---|---|
| US-01 Dashboard LIVE 복구 | PASS | before: `screenshot-dashboard-offline-before-fix.jpg`, after: `screenshot-dashboard-live-after-rpc-fix.jpg` |
| US-02 Agent Arena 계정 오염 제거 | PASS | after fix: `screenshot-dashboard-live-after-agent-filter-fix.jpg` (AgentRecord만 노출) |

## 6.4 E2E 시도 결과 상세

### Mint (실 tx 시도)
- 결과: FAIL
- 에러: `ConstraintMintMintAuthority` (Anchor 2016)
- 증거:
  - `e2e-result-v2.json` > `mint.error`
  - `verification.log`: `MSTB_MINT_AUTHORITY=3fimeX...` vs `EXPECTED_PROTOCOL_STATE=9Nbe...`
- 해석: 온체인 MSTB mint authority가 프로그램 기대값(protocol_state PDA)과 불일치

### Redeem (실 tx 시도)
- 결과: BLOCKED
- 직접 원인: Mint 선행 실패로 MSTB 미보유
- 증거: `e2e-result-v2.json` > `redeem.error`

### Register Agent (실 tx 시도)
- 결과: FAIL
- 에러: `ConstraintSeeds` on `agent_escrow` (Anchor 2006)
- 증거:
  - `e2e-result-v2.json` > `register.error`
  - `register-seed-probe.log` (v2/legacy seed 모두 Right가 고정 PDA `Cuc7P1...`로 수렴)
- 조치: 프론트는 fallback 로직을 넣었지만, 프로그램 계정 제약과 완전 정합되려면 on-chain seed 규격 재확인/재배포 필요

---

## 7) Blockers (명확화)

1. **Mint authority mismatch (P0)**
   - 증상: Mint instruction 즉시 실패
   - 재현: `node scripts/week2-e2e-devnet-check.js ...`
   - 영향: Mint/Redeem E2E 연쇄 차단

2. **register_agent escrow seed constraint mismatch (P0)**
   - 증상: register_agent가 `agent_escrow` seeds 제약에서 실패
   - 재현: `register-seed-probe.log`
   - 영향: 신규 agent 온체인 등록 불가

3. **Devnet faucet 429 (외부 의존성)**
   - 증상: airdrop rate limit/고갈
   - 재현: `verification.log`의 429 메시지
   - 영향: 신규 테스트 지갑 SOL 충전 불안정

---

## 8) 산출물 경로

- 최종 로그: `docs/evidence/week2-e2e-20260225/verification.log`
- Devnet 상태 증거:
  - `program-account.json`
  - `rpc-health.json`
  - `dashboard-head.txt`
- E2E 실행 증거:
  - `e2e-run-v2.log`
  - `e2e-result-v2.json`
  - `register-seed-probe.log`
- 스크린샷:
  - `screenshot-dashboard-offline-before-fix.jpg`
  - `screenshot-dashboard-live-after-rpc-fix.jpg`
  - `screenshot-dashboard-live-after-agent-filter-fix.jpg`

---

## 9) Git 상태

- Week2 게이트 작업 커밋/푸시 완료 (`origin/main` 반영)
- 커밋 이력 확인: `git log --oneline -n 5`
- 포함 파일: `docs/app.js`, `docs/test-cases-week2.md`, `scripts/week2-e2e-devnet-check.js`, `docs/week2-e2e-verification-latest.md`, `docs/evidence/week2-e2e-20260225/*`

---

## 10) 3줄 요약 (요구 포맷)

무엇을 고쳤는지: RPC 실패로 대시보드가 OFFLINE 되는 문제, Agent Arena 계정 오염, register_agent escrow seed 호환성(fallback)을 `docs/app.js`에서 수정했다.
어디를 검증했는지: Devnet program executable/RPC health/대시보드 HTTP 200, UI 패널 렌더링, Mint/Redeem/Register 실 tx 시도 및 실패 로그를 `docs/evidence/week2-e2e-20260225/`에 증거화했다.
다음 한 단계: 온체인에서 MSTB mint authority와 register_agent escrow seed 제약을 실제 배포 코드와 정합되게 수정/재배포 후 동일 스크립트로 Mint→Redeem→Register full PASS 재실행한다.
