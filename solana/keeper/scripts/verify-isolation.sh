#!/bin/bash
# Verify PM2 isolation for microstable-keeper
set -euo pipefail

KEEPER_NAME="${KEEPER_NAME:-microstable-keeper}"
ENV_FILE="${KEEPER_ENV_PATH:-/home/spritz/microstable-keeper/.env}"

echo "=== PM2 Isolation Check ==="
if [[ -z "${PM2_HOME:-}" ]]; then
  echo "⚠️  WARNING: PM2_HOME is not set in this shell (pm2 defaults to ~/.pm2)."
else
  echo "PM2_HOME=${PM2_HOME}"
fi

JLIST_JSON="$(pm2 jlist 2>/dev/null || true)"
if [[ -z "${JLIST_JSON}" ]]; then
  echo "⚠️  WARNING: unable to read pm2 process list from the current PM2 domain."
else
  SUMMARY="$(KEEPER_NAME="${KEEPER_NAME}" python3 - <<'PY' <<<"${JLIST_JSON}"
import json
import os
import sys

target = os.environ.get("KEEPER_NAME", "microstable-keeper")
procs = json.load(sys.stdin)
names = [p.get("name") for p in procs]

keeper_pid = ""
for proc in procs:
    if proc.get("name") == target and proc.get("pid"):
        keeper_pid = str(proc.get("pid"))
        break

isolated = len(procs) == 1 and names and names[0] == target

print(f"NAMES={json.dumps(names)}")
print(f"ISOLATED={'1' if isolated else '0'}")
print(f"PID={keeper_pid}")
PY
)"

  PROCESS_NAMES="[]"
  IS_ISOLATED="0"
  KEEPER_PID=""
  while IFS='=' read -r key value; do
    case "${key}" in
      NAMES) PROCESS_NAMES="${value}" ;;
      ISOLATED) IS_ISOLATED="${value}" ;;
      PID) KEEPER_PID="${value}" ;;
    esac
  done <<<"${SUMMARY}"

  echo "Processes in PM2 domain: ${PROCESS_NAMES}"
  if [[ "${IS_ISOLATED}" == "1" ]]; then
    echo "✅ ISOLATED"
  else
    echo "⚠️  NOT ISOLATED — other processes detected"
  fi

  RUNNING_PM2_HOME=""
  if [[ -n "${KEEPER_PID}" && -r "/proc/${KEEPER_PID}/environ" ]]; then
    RUNNING_PM2_HOME="$(tr '\0' '\n' < "/proc/${KEEPER_PID}/environ" | awk -F= '$1=="PM2_HOME" {print substr($0,10); exit}')"
    if [[ -n "${RUNNING_PM2_HOME}" ]]; then
      echo "Running ${KEEPER_NAME} PM2_HOME (/proc/${KEEPER_PID}/environ): ${RUNNING_PM2_HOME}"
    else
      echo "⚠️  WARNING: PM2_HOME is not present in /proc/${KEEPER_PID}/environ"
    fi
  fi

  if [[ -z "${RUNNING_PM2_HOME}" ]]; then
    DESCRIBE_PM2_HOME="$(pm2 describe "${KEEPER_NAME}" 2>/dev/null | awk -F': ' '/PM2_HOME/ {print $2; exit}' || true)"
    if [[ -n "${DESCRIBE_PM2_HOME}" ]]; then
      RUNNING_PM2_HOME="${DESCRIBE_PM2_HOME}"
      echo "Running ${KEEPER_NAME} PM2_HOME (pm2 describe): ${RUNNING_PM2_HOME}"
    else
      echo "⚠️  WARNING: unable to determine running ${KEEPER_NAME} PM2_HOME from /proc or pm2 describe"
    fi
  fi

  if [[ -n "${PM2_HOME:-}" && -n "${RUNNING_PM2_HOME}" && "${PM2_HOME}" != "${RUNNING_PM2_HOME}" ]]; then
    echo "⚠️  WARNING: shell PM2_HOME (${PM2_HOME}) differs from running keeper PM2_HOME (${RUNNING_PM2_HOME})"
  fi
fi

echo "=== .env permissions ==="
ls -la "${ENV_FILE}" 2>/dev/null || echo "⚠️  .env not found at ${ENV_FILE}"
