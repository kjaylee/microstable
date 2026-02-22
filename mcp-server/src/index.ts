#!/usr/bin/env node

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { readFileSync, existsSync } from "node:fs";

import { Connection, PublicKey } from "@solana/web3.js";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  ListResourcesRequestSchema,
  ReadResourceRequestSchema,
  ErrorCode,
  McpError,
} from "@modelcontextprotocol/sdk/types.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const projectRoot = resolve(__dirname, "..");
const bridgePath = resolve(projectRoot, "scripts", "microstable-bridge.py");
const pythonBin = process.env.MICROSTABLE_PYTHON || "python3";
const microstableRoot = resolve(projectRoot, "..");

if (!existsSync(bridgePath)) {
  throw new Error(`bridge script not found: ${bridgePath}`);
}

const PROTOCOL_SPEC_TEXT = `# Microstable Protocol (요약)\n\nMicrostable은 다중 담보 스테이블코인 시뮬레이터/프로토콜입니다. 핵심 구성요소:\n\n- ProtocolState: weights, mint_fee, CR, supply, reserve 등 상태 보관\n- MarketEnv: 시나리오별 가격/오라클 품질 변동\n- Optimizer(Keeper): 가중치/수수료 제안 생성\n- Watchdog + CircuitBreaker: depeg/oracle 이상에 대한 보호 로직\n- OAE(Open Agent Economy): 에이전트 등록, 토너먼트, 평판/슬래싱\n\nMCP Tool은 이 코어를 래핑하여 다음 작업을 제공합니다:\n1) protocol state 조회\n2) 시뮬레이션 실행\n3) agent 등록\n4) proposal 제출\n5) tournament 평가/조회\n6) anomaly 보고\n7) Solana devnet 상태 조회`;

function loadAcpReference(): string {
  const acpPath = resolve(
    microstableRoot,
    "..",
    "misskim-skills",
    "skills",
    "microstable",
    "references",
    "acp-v1.md",
  );
  try {
    return readFileSync(acpPath, "utf-8");
  } catch {
    return "ACP v1 reference file not found.";
  }
}

const ACP_V1_TEXT = loadAcpReference();

async function callBridge(action: string, params: Record<string, unknown>): Promise<unknown> {
  return await new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(pythonBin, [bridgePath], {
      cwd: projectRoot,
      env: {
        ...process.env,
        MICROSTABLE_ROOT: microstableRoot,
      },
      stdio: ["pipe", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";

    child.stdout.on("data", (chunk) => {
      stdout += String(chunk);
    });
    child.stderr.on("data", (chunk) => {
      stderr += String(chunk);
    });

    child.on("error", (err) => {
      rejectPromise(new Error(`failed to spawn bridge: ${err.message}`));
    });

    child.on("close", (code) => {
      const trimmed = stdout.trim();
      if (!trimmed) {
        rejectPromise(new Error(`bridge returned empty output (code=${code}, stderr=${stderr.trim()})`));
        return;
      }

      let parsed: any;
      try {
        parsed = JSON.parse(trimmed);
      } catch (err: any) {
        rejectPromise(
          new Error(`bridge output parse failed: ${err?.message ?? String(err)}; raw=${trimmed}; stderr=${stderr.trim()}`),
        );
        return;
      }

      if (!parsed?.ok) {
        const msg = parsed?.error?.message ?? `bridge call failed (code=${code})`;
        rejectPromise(new Error(msg));
        return;
      }

      resolvePromise(parsed.result);
    });

    child.stdin.write(JSON.stringify({ action, params }));
    child.stdin.end();
  });
}

function textResult(data: unknown) {
  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(data, null, 2),
      },
    ],
  };
}

async function microstableDevnetInfo(args: Record<string, unknown>): Promise<unknown> {
  const rpcUrl = String(args.rpcUrl ?? args.rpc_url ?? "https://api.devnet.solana.com");
  const programIdRaw = args.programId ?? args.program_id;

  const conn = new Connection(rpcUrl, "confirmed");
  const [version, epochInfo, slot, latestBlockhash] = await Promise.all([
    conn.getVersion(),
    conn.getEpochInfo(),
    conn.getSlot("confirmed"),
    conn.getLatestBlockhash("confirmed"),
  ]);

  let programInfo: Record<string, unknown> | null = null;
  if (programIdRaw) {
    const pubkey = new PublicKey(String(programIdRaw));
    const account = await conn.getAccountInfo(pubkey, "confirmed");
    programInfo = {
      programId: pubkey.toBase58(),
      exists: !!account,
      executable: account?.executable ?? false,
      owner: account?.owner?.toBase58() ?? null,
      lamports: account?.lamports ?? 0,
      dataLength: account?.data?.length ?? 0,
    };
  }

  return {
    cluster: "devnet",
    rpcUrl,
    slot,
    epoch: epochInfo.epoch,
    blockHeight: epochInfo.blockHeight,
    absoluteSlot: epochInfo.absoluteSlot,
    latestBlockhash: latestBlockhash.blockhash,
    lastValidBlockHeight: latestBlockhash.lastValidBlockHeight,
    nodeVersion: version["solana-core"],
    program: programInfo,
  };
}

function requireObjectArgs(raw: unknown): Record<string, unknown> {
  if (!raw) return {};
  if (typeof raw !== "object" || Array.isArray(raw)) {
    throw new McpError(ErrorCode.InvalidParams, "arguments must be an object");
  }
  return raw as Record<string, unknown>;
}

const server = new Server(
  {
    name: "microstable-mcp-server",
    version: "0.1.0",
  },
  {
    capabilities: {
      tools: {},
      resources: {},
    },
  },
);

server.setRequestHandler(ListToolsRequestSchema, async () => {
  return {
    tools: [
      {
        name: "microstable_state",
        description: "프로토콜 상태 조회 (peg, CR, weights, totalSupply, circuit breaker)",
        inputSchema: {
          type: "object",
          properties: {
            epoch: { type: "integer", description: "epoch/tick hint" },
            scenario: { type: "string", description: "normal|single_depeg|multi_depeg|volatile|gradient_attack|oracle_failure" },
            seed: { type: "integer", description: "deterministic seed" },
          },
          additionalProperties: false,
        },
      },
      {
        name: "microstable_simulate",
        description: "시나리오 시뮬레이션 실행 (ticks, shocks)",
        inputSchema: {
          type: "object",
          properties: {
            scenario: { type: "string" },
            ticks: { type: "integer", minimum: 1 },
            seed: { type: "integer" },
            shocks: {
              type: "array",
              items: {
                type: "object",
                properties: {
                  tick: { type: "integer" },
                  asset: { anyOf: [{ type: "string" }, { type: "integer" }] },
                  delta: { type: "number" },
                },
                required: ["tick", "asset", "delta"],
                additionalProperties: false,
              },
            },
          },
          required: ["ticks"],
          additionalProperties: false,
        },
      },
      {
        name: "microstable_agent_register",
        description: "에이전트 등록 (type, stake)",
        inputSchema: {
          type: "object",
          properties: {
            agent_id: { type: "string" },
            type: { type: "string", description: "Optimizer|Monitor|Auditor|Liquidator" },
            stake: { type: "number", minimum: 0 },
            epoch: { type: "integer" },
          },
          required: ["agent_id", "type", "stake"],
          additionalProperties: false,
        },
      },
      {
        name: "microstable_propose",
        description: "최적화 제안 제출 (epoch, weights, fees)",
        inputSchema: {
          type: "object",
          properties: {
            agent_id: { type: "string" },
            epoch: { type: "integer" },
            weights: {
              type: "array",
              minItems: 4,
              maxItems: 4,
              items: { type: "number" },
            },
            fees: {
              type: "object",
              properties: {
                mint_fee: { type: "number" },
              },
              additionalProperties: false,
            },
            seed: { type: "integer" },
            scenario: { type: "string" },
          },
          required: ["epoch", "weights"],
          additionalProperties: false,
        },
      },
      {
        name: "microstable_tournament",
        description: "토너먼트 결과 조회",
        inputSchema: {
          type: "object",
          properties: {
            epoch: { type: "integer" },
            epoch_fees: { type: "number" },
            force: { type: "boolean" },
          },
          required: ["epoch"],
          additionalProperties: false,
        },
      },
      {
        name: "microstable_report_anomaly",
        description: "이상 감지 보고",
        inputSchema: {
          type: "object",
          properties: {
            agent_id: { type: "string" },
            anomaly_type: { type: "string" },
            epoch: { type: "integer" },
            method: { type: "string" },
            evidence: { type: "object" },
            resolve: { type: "boolean" },
            is_true: { type: "boolean" },
          },
          additionalProperties: false,
        },
      },
      {
        name: "microstable_devnet_info",
        description: "Solana 데브넷 프로그램 정보 조회",
        inputSchema: {
          type: "object",
          properties: {
            rpcUrl: { type: "string" },
            programId: { type: "string" },
          },
          additionalProperties: false,
        },
      },
    ],
  };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const name = request.params.name;
  const args = requireObjectArgs(request.params.arguments);

  try {
    switch (name) {
      case "microstable_state": {
        const result = await callBridge("state", args);
        return textResult(result);
      }
      case "microstable_simulate": {
        const result = await callBridge("simulate", args);
        return textResult(result);
      }
      case "microstable_agent_register": {
        const result = await callBridge("agent_register", args);
        return textResult(result);
      }
      case "microstable_propose": {
        const result = await callBridge("propose", args);
        return textResult(result);
      }
      case "microstable_tournament": {
        const result = await callBridge("tournament", args);
        return textResult(result);
      }
      case "microstable_report_anomaly": {
        const result = await callBridge("report_anomaly", args);
        return textResult(result);
      }
      case "microstable_devnet_info": {
        const result = await microstableDevnetInfo(args);
        return textResult(result);
      }
      default:
        throw new McpError(ErrorCode.MethodNotFound, `unknown tool: ${name}`);
    }
  } catch (err: any) {
    throw new McpError(ErrorCode.InternalError, err?.message ?? String(err));
  }
});

server.setRequestHandler(ListResourcesRequestSchema, async () => {
  return {
    resources: [
      {
        uri: "microstable://protocol/spec",
        name: "Microstable protocol spec summary",
        description: "요약된 프로토콜 스펙",
        mimeType: "text/markdown",
      },
      {
        uri: "microstable://acp/v1",
        name: "ACP v1 methods",
        description: "Agent Communication Protocol v1 메서드 목록",
        mimeType: "text/markdown",
      },
    ],
  };
});

server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
  const uri = request.params.uri;

  if (uri === "microstable://protocol/spec") {
    return {
      contents: [
        {
          uri,
          mimeType: "text/markdown",
          text: PROTOCOL_SPEC_TEXT,
        },
      ],
    };
  }

  if (uri === "microstable://acp/v1") {
    return {
      contents: [
        {
          uri,
          mimeType: "text/markdown",
          text: ACP_V1_TEXT,
        },
      ],
    };
  }

  throw new McpError(ErrorCode.InvalidRequest, `resource not found: ${uri}`);
});

server.onerror = (error) => {
  console.error("[microstable-mcp-server]", error);
};

async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  console.error("Failed to start microstable MCP server", err);
  process.exit(1);
});
