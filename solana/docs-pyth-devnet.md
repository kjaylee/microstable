# Pyth devnet feed mapping (USDC/USDT/DAI)

- Source (program addresses): https://docs.pyth.network/price-feeds/core/contract-addresses/solana
  - Receiver program (devnet): `rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ`
  - Price feed/push oracle program: `pythWSnswVUd12oZpeFP8e9CVaEqJg25g1Vtc2biRsT`

- Source (feed IDs): Hermes metadata endpoint
  - `https://hermes.pyth.network/v2/price_feeds?query=USDC/USD`
  - `https://hermes.pyth.network/v2/price_feeds?query=USDT/USD`
  - `https://hermes.pyth.network/v2/price_feeds?query=DAI/USD`

## Feed IDs
- USDC/USD: `eaa020c61cc479712813461ce153894a96a6c00b21ed0cfc2798d1f9a9e9c94a`
- USDT/USD: `2b89b9dc8fdf9f34709a5b106b472f0f39bb6ca9ce04b0fd7f2e971688e2e53b`
- DAI/USD: `b0948a5e5313200c632b51bb5ca32f6de0d36e9950a942d19751e833f70dabfd`

## Solana devnet feed accounts (shard 0)
Derived via PDA formula used by Pyth Solana receiver SDK:
`PDA([shard_u16_le, feed_id_bytes], pythWSns...)`

- USDC/USD: `Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX`
- USDT/USD: `HT2PLQBcG5EiCcNSaMHAjSgd9F98ecpATbk4Sk5oYuM`
- DAI/USD: `FmfrxJ7YH8yVxoYpJ9ZDMeb8gUceYXYaSrQiBJ1uSZjN`

Verified on devnet: all three accounts exist and are owned by `rec5EKMG...`.
