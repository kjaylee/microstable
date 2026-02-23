# Ops Hardening — PM2 환경변수 비밀정보 노출 대응 (MSV8-006)

## 목적
MiniPC에서 `pm2 jlist` 등 런타임 조회 시 민감 환경변수 평문 노출 가능성을 줄이기 위해,
keeper 운영 구성을 **`.env` 파일 기반**으로 전환하고 파일 권한을 최소화한다.

## 적용 범위
- PM2 프로세스 정의(`ecosystem.config.js`)
- Keeper 운영 서버(예: MiniPC)

## 조치 항목

### 1) `ecosystem.config.js`에서 민감 env 인라인 제거
민감값을 `env` 오브젝트에 직접 하드코딩하지 말고, `.env` 파일에서 로드한다.

예시:

```js
require('dotenv').config({ path: '/home/spritz/microstable-keeper/.env' });

module.exports = {
  apps: [
    {
      name: 'microstable-keeper',
      script: 'target/release/microstable-keeper',
      env: {
        RUST_LOG: process.env.RUST_LOG || 'info',
        RPC_URL: process.env.RPC_URL,
        SECONDARY_RPC_URL: process.env.SECONDARY_RPC_URL,
        // 민감값은 process.env.*로만 참조
      },
    },
  ],
};
```

### 2) `.env` 파일 권한 600 강제

```bash
chmod 600 /home/spritz/microstable-keeper/.env
```

권장 소유권:

```bash
chown <keeper-user>:<keeper-user> /home/spritz/microstable-keeper/.env
```

### 3) 배포 후 확인

```bash
ls -l /home/spritz/microstable-keeper/.env
pm2 restart ecosystem.config.js --update-env
pm2 describe microstable-keeper
```

- `.env`가 `-rw-------`(600)인지 확인
- PM2가 최신 환경을 다시 로드했는지 확인

### 4) Rebalance 가용성 필수 요건(중요)

`rebalance commit` 트랜잭션은 **로컬 keeper가 실제로 보유한 서명키**로만 제출할 수 있다.
따라서 keeper에 로드된 키 중 최소 1개는 반드시 다음 조건을 만족해야 한다.

- AgentRecord가 온체인에 존재
- `status == Active`
- `tier >= 2`

운영 시 준비 명령:

```bash
# Register keeper as agent (from register-agents.ts):
ts-node solana/scripts/register-agents.ts

# Promote to tier 2 (requires quorum):
# Use update_agent_score + promote_agent instructions
```

권장: 배포 파이프라인에서 keeper 시작 전에 위 조건을 사전 점검하고,
운영 정책상 rebalance가 필수인 환경에서는 keeper를 `--require-rebalance` 플래그와 함께 실행한다.

## 추가 권고
- Keeper 전용 OS 사용자 및 전용 PM2 HOME(`PM2_HOME`) 분리
- PM2 RPC/socket 접근 권한 최소화
- 운영 비밀값은 가능하면 Secret Manager/KMS 사용
