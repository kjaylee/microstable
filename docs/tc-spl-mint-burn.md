# TC — SPL Mint/Burn 실구현 검증

## 범위
- Program: `solana/programs/microstable/src/lib.rs`
- E2E: `solana/tests/devnet-e2e.ts`

## TC-01 Mint 시 MSTB 실제 발행
- Given: 사용자 USDC ATA 잔고 보유, MSTB mint authority = `protocol_state` PDA
- When: `mint(collateral_index=0, collateral_amount=1_000_000)` 호출
- Then:
  1. 사용자 USDC ATA 감소 / vault USDC ATA 증가
  2. 사용자 MSTB ATA 증가량 == `user_position.usd_balance` 증가량
  3. MSTB mint `supply` 증가량 == 발행량

## TC-02 Redeem 시 MSTB 실제 소각
- Given: TC-01 수행 후 사용자 MSTB 보유
- When: `redeem(musd_amount=mintedDelta)` 호출
- Then:
  1. 사용자 MSTB ATA 감소(소각) / MSTB mint `supply` 감소
  2. 사용자 collateral ATA 환급, vault ATA 감소
  3. `user_position.usd_balance`와 `protocol_state.total_supply`가 동일량 감소

## TC-03 잘못된 MSTB ATA 방어
- Given: 사용자 소유가 아닌 MSTB ATA 전달
- When: `mint` 또는 `redeem` 호출
- Then: `InvalidTokenAccount` 실패

## TC-04 잘못된 mint authority 방어
- Given: `protocol_state`가 authority가 아닌 임의 mint 전달
- When: `mint` 또는 `redeem` 호출
- Then: account constraint로 트랜잭션 실패
