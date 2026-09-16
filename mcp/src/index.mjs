#!/usr/bin/env node
import { timingSafeEqual } from "node:crypto";
import { createServer } from "node:http";
import { McpServer, createMcpHandler } from "@modelcontextprotocol/server";
import { hostHeaderValidation, originValidation, toNodeHandler } from "@modelcontextprotocol/node";
import * as z from "zod/v4";
import { PkosClient } from "./pkos-client.mjs";

const VERSION = "0.2.0";
const PORT = Number(process.env.PKOS_MCP_PORT ?? 8787);
// Local execution is loopback-only by default. Docker explicitly binds inside the container
// and publishes the host port on 127.0.0.1 so Secure MCP Tunnel is the external ingress.
const BIND = process.env.PKOS_MCP_BIND ?? "127.0.0.1";
const BACKEND_URL = process.env.PKOS_MCP_API_BASE_URL ?? "http://api:8080";
const BACKEND_API_KEY = process.env.PKOS_API_KEY ?? "";
const MCP_BEARER_TOKEN = process.env.PKOS_MCP_BEARER_TOKEN ?? "";
const PUBLIC_HOSTNAME = process.env.PKOS_MCP_PUBLIC_HOSTNAME ?? "";

if (MCP_BEARER_TOKEN.length < 32) {
  throw new Error("PKOS_MCP_BEARER_TOKEN must be at least 32 characters");
}
if (!BACKEND_API_KEY) {
  throw new Error("PKOS_API_KEY is required");
}

const splitList = (value) =>
  value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

const allowedHosts = [
  "localhost",
  "127.0.0.1",
  `localhost:${PORT}`,
  `127.0.0.1:${PORT}`,
  ...(PUBLIC_HOSTNAME ? [PUBLIC_HOSTNAME, `${PUBLIC_HOSTNAME}:443`] : []),
  ...splitList(process.env.PKOS_MCP_ALLOWED_HOSTS ?? ""),
];
const allowedOrigins = [
  "localhost",
  "127.0.0.1",
  ...(PUBLIC_HOSTNAME ? [PUBLIC_HOSTNAME] : []),
  ...splitList(process.env.PKOS_MCP_ALLOWED_ORIGINS ?? ""),
];

const client = new PkosClient({ baseUrl: BACKEND_URL, apiKey: BACKEND_API_KEY });

const INSTRUCTIONS = `Personal Knowledge OS は、ユーザーの長期的な知識・意思決定・好み・目標・制約・プロジェクト履歴を、根拠となるSourceと結び付けて保存する外部記憶です。

次のときは推測だけで答えず、このサーバーを使ってください。
- 過去の決定、好み、目標、制約、プロジェクト情報を思い出す必要があるとき: search_knowledge
- ユーザーが新しい長期的な事実・決定・好み・目標・制約・習慣・スキル・プロジェクト更新を明示したとき: remember
- 会話全体を根拠付きで保存したいとき: capture_conversation
- 接続状態を確認するとき: pkos_status

ユーザーに「覚えて」と言われるまで待つ必要はありません。将来の会話で役立つと判断でき、ユーザー本人の発言に根拠がある情報は remember を使って保存してください。
remember の user_text には、ユーザー本人が実際に書いた根拠テキストをできるだけ原文のまま渡してください。パスワード、APIキー、アクセストークン、秘密鍵などの秘密情報は保存しないでください。
書き込みはActive Memoryを直接変更しません。必ずSource/Captureとして保存され、PKOS側の抽出・根拠検証・自動昇格ポリシーを通ります。`;

const textResult = (value) => ({
  content: [{ type: "text", text: JSON.stringify(value, null, 2) }],
  structuredContent: value,
});

const toolError = (error) => ({
  isError: true,
  content: [{ type: "text", text: error instanceof Error ? error.message : String(error) }],
});

const readOnly = { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false };
const additiveWrite = { readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: false };

function registerTools(server) {
  server.registerTool(
    "pkos_status",
    {
      title: "Personal Knowledge OS status",
      description: "Check that the MCP gateway can reach the Personal Knowledge OS backend. / ChatGPTからPKOSへの接続状態を確認します。",
      annotations: readOnly,
      inputSchema: z.object({}),
    },
    async () => {
      try {
        return textResult({ gateway: "ok", backend: await client.status(), version: VERSION });
      } catch (error) {
        return toolError(error);
      }
    },
  );

  server.registerTool(
    "remember",
    {
      title: "Remember durable user knowledge",
      description: "Store a durable user-authored fact, preference, decision, goal, constraint, routine, skill, or project update in Personal Knowledge OS. / ユーザーが明示した長期的に覚える価値のある情報を保存します。user_textはできるだけユーザー原文を渡してください。",
      annotations: additiveWrite,
      inputSchema: z.object({
        user_text: z.string().min(1).max(200_000).describe("Verbatim or near-verbatim user-authored evidence to preserve."),
        assistant_context: z.string().max(200_000).optional().describe("Optional assistant context; never use this as a substitute for user evidence."),
        title: z.string().max(512).optional(),
        kind: z.string().min(1).max(64).optional(),
        occurred_at: z.string().datetime({ offset: true }).optional(),
        session_id: z.string().max(512).optional(),
        source_uri: z.string().max(2048).optional(),
        idempotency_key: z.string().max(480).optional().describe("Stable key when retrying the same write."),
        metadata: z.record(z.string(), z.unknown()).optional(),
      }),
    },
    async (args) => {
      try {
        return textResult(await client.remember(args));
      } catch (error) {
        return toolError(error);
      }
    },
  );

  server.registerTool(
    "capture_conversation",
    {
      title: "Capture conversation evidence",
      description: "Persist a role-preserving conversation transcript as Source evidence and schedule automatic memory extraction. / user・assistant等の役割を保った会話を根拠として保存します。",
      annotations: additiveWrite,
      inputSchema: z.object({
        messages: z.array(z.object({
          role: z.enum(["user", "assistant", "system", "tool", "agent"]),
          content: z.string().min(1).max(500_000),
          name: z.string().max(128).optional(),
        })).min(1).max(200),
        title: z.string().max(512).optional(),
        kind: z.string().min(1).max(64).optional(),
        provider: z.string().max(128).optional(),
        session_id: z.string().max(512).optional(),
        source_uri: z.string().max(2048).optional(),
        occurred_at: z.string().datetime({ offset: true }).optional(),
        idempotency_key: z.string().max(480).optional(),
        metadata: z.record(z.string(), z.unknown()).optional(),
      }),
    },
    async (args) => {
      try {
        return textResult(await client.captureConversation(args));
      } catch (error) {
        return toolError(error);
      }
    },
  );

  server.registerTool(
    "search_knowledge",
    {
      title: "Search personal knowledge",
      description: "Search Source evidence and active memories with PKOS hybrid retrieval. Use this before guessing what the user previously decided, preferred, planned, or learned. / 過去の意思決定・好み・計画・知識を検索します。",
      annotations: readOnly,
      inputSchema: z.object({
        query: z.string().min(1).max(8_000),
        limit: z.number().int().min(1).max(50).optional(),
        include_memories: z.boolean().optional(),
      }),
    },
    async (args) => {
      try {
        return textResult(await client.searchKnowledge(args));
      } catch (error) {
        return toolError(error);
      }
    },
  );

  server.registerTool(
    "get_source",
    {
      title: "Get source evidence",
      description: "Retrieve one immutable Source record by UUID, including exact content and provenance metadata. / UUIDで保存済みSource原文を取得します。",
      annotations: readOnly,
      inputSchema: z.object({ id: z.string().uuid() }),
    },
    async ({ id }) => {
      try {
        return textResult(await client.getSource(id));
      } catch (error) {
        return toolError(error);
      }
    },
  );

  server.registerTool(
    "list_memories",
    {
      title: "List memories",
      description: "List Personal Knowledge OS memories and their evidence links. / Active・Candidate・Superseded等のMemoryを一覧します。",
      annotations: readOnly,
      inputSchema: z.object({
        status: z.enum(["candidate", "active", "rejected", "superseded", "disputed", "expired"]).optional(),
        limit: z.number().int().min(1).max(200).optional(),
      }),
    },
    async (args) => {
      try {
        return textResult(await client.listMemories(args));
      } catch (error) {
        return toolError(error);
      }
    },
  );

  server.registerTool(
    "get_memory",
    {
      title: "Get memory",
      description: "Retrieve one memory by UUID with lifecycle state and evidence Source IDs. / Memoryとその根拠Source IDを取得します。",
      annotations: readOnly,
      inputSchema: z.object({ id: z.string().uuid() }),
    },
    async ({ id }) => {
      try {
        return textResult(await client.getMemory(id));
      } catch (error) {
        return toolError(error);
      }
    },
  );

  server.registerTool(
    "recent_timeline",
    {
      title: "Recent knowledge timeline",
      description: "Read recent Source events in reverse chronological order. / 最近PKOSに入った出来事・会話・ノートを時系列で確認します。",
      annotations: readOnly,
      inputSchema: z.object({
        limit: z.number().int().min(1).max(200).optional(),
        before: z.string().datetime({ offset: true }).optional(),
      }),
    },
    async (args) => {
      try {
        return textResult(await client.recentTimeline(args));
      } catch (error) {
        return toolError(error);
      }
    },
  );
}

const handler = createMcpHandler(() => {
  const server = new McpServer(
    { name: "personal-knowledge-os", version: VERSION },
    { instructions: INSTRUCTIONS },
  );
  registerTools(server);
  return server;
}, { responseMode: "json" });

const nodeHandler = toNodeHandler(handler);
const validateHost = hostHeaderValidation([...new Set(allowedHosts)]);
const validateOrigin = originValidation([...new Set(allowedOrigins)]);

function authorized(header) {
  if (typeof header !== "string" || !header.startsWith("Bearer ")) return false;
  const provided = Buffer.from(header.slice(7), "utf8");
  const expected = Buffer.from(MCP_BEARER_TOKEN, "utf8");
  return provided.length === expected.length && timingSafeEqual(provided, expected);
}

createServer((req, res) => {
  if (req.url === "/healthz") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ status: "ok" }));
    return;
  }

  if (!validateHost(req, res) || !validateOrigin(req, res)) return;

  if (req.url?.startsWith("/.well-known/")) {
    res.writeHead(404, { "content-type": "application/json" });
    res.end(JSON.stringify({
      error: "not_found",
      error_description: "This server does not use OAuth. For ChatGPT, use Secure MCP Tunnel and inject the local Bearer credential from the tunnel profile.",
    }));
    return;
  }

  if (req.url !== "/mcp") {
    res.writeHead(404, { "content-type": "application/json" });
    res.end(JSON.stringify({ error: "not_found" }));
    return;
  }

  if (!authorized(req.headers.authorization)) {
    res.writeHead(401, { "content-type": "application/json", "www-authenticate": "Bearer" });
    res.end(JSON.stringify({ error: "unauthorized" }));
    return;
  }

  void nodeHandler(req, res);
}).listen(PORT, BIND, () => {
  console.log(`Personal Knowledge OS MCP listening on http://${BIND}:${PORT}/mcp -> ${BACKEND_URL}`);
});
