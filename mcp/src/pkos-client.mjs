const DEFAULT_TIMEOUT_MS = 20_000;

export class PkosClient {
  constructor({ baseUrl, apiKey, timeoutMs = DEFAULT_TIMEOUT_MS }) {
    if (!baseUrl) throw new Error("PKOS_MCP_API_BASE_URL is required");
    if (!apiKey) throw new Error("PKOS_API_KEY is required for the MCP gateway");
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.apiKey = apiKey;
    this.timeoutMs = timeoutMs;
  }

  async request(path, { method = "GET", body } = {}) {
    const response = await fetch(`${this.baseUrl}${path}`, {
      method,
      headers: {
        accept: "application/json",
        authorization: `Bearer ${this.apiKey}`,
        ...(body === undefined ? {} : { "content-type": "application/json" }),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(this.timeoutMs),
    });

    const text = await response.text();
    let payload = null;
    if (text) {
      try {
        payload = JSON.parse(text);
      } catch {
        payload = text;
      }
    }

    if (!response.ok) {
      const detail = typeof payload === "string" ? payload : JSON.stringify(payload);
      throw new Error(`PKOS API ${method} ${path} returned ${response.status}: ${detail}`);
    }
    return payload;
  }

  remember(args) {
    const messages = [{ role: "user", content: args.user_text, name: null }];
    if (args.assistant_context) {
      messages.push({ role: "assistant", content: args.assistant_context, name: null });
    }
    const metadata = {
      ...(args.metadata ?? {}),
      pkos_mcp: { tool: "remember", automatic: true, provenance: "chatgpt-mcp" },
    };
    return this.request("/v1/captures", {
      method: "POST",
      body: {
        kind: args.kind ?? "conversation",
        title: args.title ?? null,
        provider: "chatgpt-mcp",
        session_id: args.session_id ?? null,
        external_id: args.idempotency_key ? `mcp:${args.idempotency_key}` : null,
        source_uri: args.source_uri ?? null,
        occurred_at: args.occurred_at ?? null,
        messages,
        metadata,
      },
    });
  }

  captureConversation(args) {
    return this.request("/v1/captures", {
      method: "POST",
      body: {
        kind: args.kind ?? "conversation",
        title: args.title ?? null,
        provider: args.provider ?? "chatgpt-mcp",
        session_id: args.session_id ?? null,
        external_id: args.idempotency_key ? `mcp:${args.idempotency_key}` : null,
        source_uri: args.source_uri ?? null,
        occurred_at: args.occurred_at ?? null,
        messages: args.messages.map((message) => ({
          role: message.role,
          content: message.content,
          name: message.name ?? null,
        })),
        metadata: {
          ...(args.metadata ?? {}),
          pkos_mcp: { tool: "capture_conversation", automatic: true, provenance: "mcp" },
        },
      },
    });
  }

  searchKnowledge(args) {
    return this.request("/v1/search", {
      method: "POST",
      body: {
        query: args.query,
        limit: args.limit ?? 10,
        include_memories: args.include_memories ?? true,
      },
    });
  }

  getSource(id) {
    return this.request(`/v1/sources/${encodeURIComponent(id)}`);
  }

  listMemories(args) {
    const params = new URLSearchParams();
    if (args.status) params.set("status", args.status);
    params.set("limit", String(args.limit ?? 50));
    return this.request(`/v1/memories?${params}`);
  }

  getMemory(id) {
    return this.request(`/v1/memories/${encodeURIComponent(id)}`);
  }

  recentTimeline(args) {
    const params = new URLSearchParams({ limit: String(args.limit ?? 20) });
    if (args.before) params.set("before", args.before);
    return this.request(`/v1/timeline?${params}`);
  }
}
