# @microstable/mcp-server

Microstable 프로토콜을 MCP(Model Context Protocol) 서버로 래핑한 stdio 서버입니다.

- TypeScript + `@modelcontextprotocol/sdk`
- Python core bridge (`scripts/microstable-bridge.py`)를 통해 `microstable.py`, `open_agent_economy.py` 호출
- Solana Devnet 조회는 `@solana/web3.js` 직접 사용

## 설치

```bash
cd mcp-server
npm install
npm run build
```

## 실행

```bash
node dist/index.js
```

또는 npm bin:

```bash
npx @microstable/mcp-server
```

## Claude Desktop 설정 예시

```json
{
  "mcpServers": {
    "microstable": {
      "command": "npx",
      "args": ["@microstable/mcp-server"]
    }
  }
}
```

## 제공 MCP Tools

1. `microstable_state` — 프로토콜 상태 조회 (peg, CR, weights, totalSupply, circuit breaker)
2. `microstable_simulate` — 시나리오 시뮬레이션 실행 (ticks, shocks)
3. `microstable_agent_register` — 에이전트 등록 (type, stake)
4. `microstable_propose` — 최적화 제안 제출 (epoch, weights, fees)
5. `microstable_tournament` — 토너먼트 결과 조회
6. `microstable_report_anomaly` — 이상 감지 보고
7. `microstable_devnet_info` — Solana 데브넷 프로그램 정보 조회

## 제공 MCP Resources

- `microstable://protocol/spec` — 프로토콜 스펙 요약
- `microstable://acp/v1` — ACP v1 메서드 목록

## Bridge 상태 파일

Bridge는 로컬 상태를 아래 경로에 저장합니다.

- `mcp-server/.state/microstable-bridge-state.json`

## 테스트

```bash
cd mcp-server
npm install
npm run build

echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | node dist/index.js
```

추가 예시 (`microstable_state`):

```bash
echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"microstable_state","arguments":{}}}' | node dist/index.js
```

## 개발 메모

- Python 실행 경로를 바꾸려면 `MICROSTABLE_PYTHON` 환경변수 사용
- 기본 Python: `python3`
- 기본 Solana RPC: `https://api.devnet.solana.com`
