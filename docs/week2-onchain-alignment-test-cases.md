# Week2 On-Chain Alignment Test Cases

- 기준 스펙: `docs/week2-onchain-alignment-spec.md`
- 실행 환경: Solana Devnet / Program `BSdLEP...`

## TC-W2-ALIGN-001 — Mint authority 정렬 성공
- 목적: MSTB mint authority를 `protocol_state`로 맞춘다.
- 사전조건:
  - 현재 authority signer 키 보유
- 절차:
  1. authority 조회
  2. mismatch면 `setAuthority(MintTokens)` 실행
  3. 재조회로 일치 확인
- 기대결과:
  - `before != protocol_state`, `after == protocol_state`
  - tx signature 저장
- 증거:
  - `mint-authority-align.json`
  - `mint-authority-align.log`

## TC-W2-ALIGN-002 — Mint 성공
- 목적: `ConstraintMintMintAuthority` 없이 mint 성공
- 절차:
  1. E2E 스크립트 실행
  2. mint step 결과 확인
- 기대결과:
  - `mint.ok = true`
  - `mint.mintedDeltaRaw > 0`
- 증거:
  - `e2e-result.json`
  - `e2e-run.log`

## TC-W2-ALIGN-003 — Redeem 성공
- 목적: mint 결과를 사용해 redeem 성공
- 절차:
  1. mint 이후 redeem step 실행
- 기대결과:
  - `redeem.ok = true`
  - user/supply burn delta > 0
- 증거:
  - `e2e-result.json`

## TC-W2-ALIGN-004 — register_agent seed fallback 성공
- 목적: agent_escrow seed mismatch 없이 register 성공
- 절차:
  1. 후보 seed 순차 시도
     - v2(wallet)
     - legacy(wallet)
     - legacy(global)
  2. 성공 후보 기록
- 기대결과:
  - `register.ok = true`
  - `register.selectedSeedLabel` 존재
  - `ConstraintSeeds(agent_escrow)` 최종 실패로 남지 않음
- 증거:
  - `e2e-result.json`
  - `register-seed-probe.json`

## TC-W2-ALIGN-005 — 회귀: seed mismatch 미재현
- 목적: 재실행 시 동일 blocker 미재현 확인
- 절차:
  1. 동일 스크립트 1회 추가 실행(또는 probe 재실행)
- 기대결과:
  - blocker 목록에 `register_agent ConstraintSeeds` 없음
- 증거:
  - `e2e-rerun.log` 또는 probe 로그

## 실패 시 즉시 기록 항목
- 실패 instruction / Anchor error code
- Left/Right PDA 로그
- 즉시 후속 액션(권한자/재배포/IDL 동기화 중 무엇이 필요한지)
