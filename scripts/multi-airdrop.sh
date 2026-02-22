#!/bin/bash

MAIN_WALLET="3fimeXDHiEK9oeJX6XM1rXNoavTCWhzbxNXVmwFzh6Kk"
WALLET_DIR="/Users/kjaylee/.openclaw/workspace/microstable/wallets"
SOLANA="/Users/kjaylee/.local/share/solana/install/active_release/bin/solana"
KEYGEN=$(PATH="/Users/kjaylee/.local/share/solana/install/active_release/bin:$PATH" which solana-keygen)

mkdir -p "$WALLET_DIR"

if [ -z "$KEYGEN" ] || [ ! -x "$KEYGEN" ]; then
  echo "ERROR: solana-keygen not found"
  exit 1
fi

if [ ! -x "$SOLANA" ]; then
  echo "ERROR: solana not found at $SOLANA"
  exit 1
fi

echo "Using solana: $SOLANA"
echo "Using solana-keygen: $KEYGEN"

GENERATED=0
AIRDROP_SUCCESS=0
AIRDROP_FAILED=0
TRANSFER_SUCCESS=0
TRANSFER_FAILED=0
TOTAL_COLLECTED="0"

# 10개 지갑 생성 + 에어드롭
for i in $(seq 1 10); do
  KEYPAIR="$WALLET_DIR/wallet-$i.json"

  if "$KEYGEN" new --no-bip39-passphrase --outfile "$KEYPAIR" --force >/dev/null 2>&1; then
    GENERATED=$((GENERATED + 1))
  else
    echo "Wallet $i: keygen failed"
    AIRDROP_FAILED=$((AIRDROP_FAILED + 1))
    continue
  fi

  ADDR=$("$KEYGEN" pubkey "$KEYPAIR" 2>/dev/null)
  echo "Wallet $i: $ADDR"

  DROPPED=0
  for attempt in 1 2 3; do
    OUT=$("$SOLANA" airdrop 2 "$ADDR" --url devnet 2>&1)
    STATUS=$?
    echo "$OUT"

    if [ $STATUS -eq 0 ]; then
      echo "  Airdrop SUCCESS (attempt $attempt)"
      AIRDROP_SUCCESS=$((AIRDROP_SUCCESS + 1))
      DROPPED=1
      break
    else
      if echo "$OUT" | grep -q "429"; then
        echo "  Airdrop FAILED with 429 (attempt $attempt), waiting 30s..."
      else
        echo "  Airdrop FAILED (attempt $attempt), waiting 30s..."
      fi
      if [ "$attempt" -lt 3 ]; then
        sleep 30
      fi
    fi
  done

  if [ "$DROPPED" -eq 0 ]; then
    AIRDROP_FAILED=$((AIRDROP_FAILED + 1))
  fi
done

# 잔고 확인 후 메인 지갑으로 전송
echo "=== Transferring to main wallet ==="
for i in $(seq 1 10); do
  KEYPAIR="$WALLET_DIR/wallet-$i.json"
  [ -f "$KEYPAIR" ] || continue

  ADDR=$("$KEYGEN" pubkey "$KEYPAIR" 2>/dev/null)
  BAL_OUT=$("$SOLANA" balance "$ADDR" --url devnet 2>/dev/null)
  BAL=$(echo "$BAL_OUT" | awk '{print $1}')

  if [ -n "$BAL" ] && awk "BEGIN {exit !($BAL > 0)}"; then
    # 여유 수수료를 위해 0.001 SOL 차감
    SEND=$(awk "BEGIN {printf \"%.9f\", $BAL - 0.001}")

    if awk "BEGIN {exit !($SEND > 0)}"; then
      TX_OUT=$("$SOLANA" transfer "$MAIN_WALLET" "$SEND" --from "$KEYPAIR" --url devnet --allow-unfunded-recipient --fee-payer "$KEYPAIR" 2>&1)
      TX_STATUS=$?
      echo "$TX_OUT"

      if [ $TX_STATUS -eq 0 ]; then
        echo "  Wallet $i: sent $SEND SOL"
        TOTAL_COLLECTED=$(awk "BEGIN {printf \"%.9f\", $TOTAL_COLLECTED + $SEND}")
        TRANSFER_SUCCESS=$((TRANSFER_SUCCESS + 1))
      else
        echo "  Wallet $i: transfer failed"
        TRANSFER_FAILED=$((TRANSFER_FAILED + 1))
      fi
    fi
  fi
done

MAIN_BALANCE=$("$SOLANA" balance "$MAIN_WALLET" --url devnet 2>/dev/null)

echo "=== Summary ==="
echo "Generated wallets: $GENERATED"
echo "Airdrop success: $AIRDROP_SUCCESS"
echo "Airdrop failed: $AIRDROP_FAILED"
echo "Transfer success: $TRANSFER_SUCCESS"
echo "Transfer failed: $TRANSFER_FAILED"
echo "Total collected: $TOTAL_COLLECTED SOL"
echo "Main wallet final balance: $MAIN_BALANCE"
