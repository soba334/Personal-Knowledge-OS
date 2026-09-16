# ChatGPTからPersonal Knowledge OSを使う

Personal Knowledge OSのMCPサーバーは、ChatGPTから過去の知識を検索したり、会話中に新しい情報を自動保存したりするための入口です。

## 構成

```text
ChatGPT
  -> Secure MCP Tunnel (outbound HTTPS)
  -> tunnel-client on your machine
  -> http://127.0.0.1:8787/mcp
  -> PKOS MCP sidecar
  -> http://api:8080 inside Docker network
  -> Personal Knowledge OS
```

MCP、Rust API、PostgreSQLを公開インターネットへ直接出す必要はありません。`docker-compose.yml`はHost側の5432/8080/8787をすべて`127.0.0.1`に限定しています。

## 1. PKOSを起動する

`.env`には少なくとも別々の強い秘密値を設定します。

```env
POSTGRES_PASSWORD=...
PKOS_API_KEY=...
PKOS_MCP_BEARER_TOKEN=...
```

生成例:

```bash
openssl rand -hex 32
```

自動Memory化を使う場合はMemory providerも設定します。

```env
PKOS_MEMORY_BASE_URL=https://api.openai.com/v1
PKOS_MEMORY_API_KEY=...
PKOS_MEMORY_MODEL=gpt-5-mini
PKOS_MEMORY_EXTRACTION_ENABLED=true
PKOS_MEMORY_AUTO_PROMOTE_ENABLED=true
```

起動:

```bash
docker compose up -d --build
curl -sS http://127.0.0.1:8787/healthz
```

## 2. ChatGPT側でSecure MCP Tunnelを作る

ChatGPTのカスタムMCPアプリ作成画面からSecure MCP Tunnelを新規作成します。ローカルMCPにはChatGPTから直接到達できないため、ローカル・オンプレミス・プライベートネットワーク上のMCPにはSecure MCP Tunnelを使います。

ChatGPT側の認証方式は**認証なし**にします。MCPのBearer秘密値はChatGPTへ登録しません。

作成した` tunnel_id `と、そのTunnelを動かすOpenAI Platform API keyを控えます。Tunnelは作成したChatGPT workspace / Platform organizationと対応する資格情報で動かしてください。

> UIや利用可能プランはOpenAI側で変更されることがあります。ChatGPTのDeveloper mode / custom MCP app / Secure MCP Tunnelの現在の表示に従ってください。

## 3. tunnel-clientを設定する

OpenAIが案内している現在の`tunnel-client`を、PKOSを動かすマシンにインストールします。バージョンは更新されるため、このリポジトリでは固定しません。

Platform API keyは権限を絞ったファイルに保存します。

```bash
install -d -m 700 ~/.config/pkos
read -rs -p "OpenAI tunnel API key: " K \
  && printf '%s' "$K" > ~/.config/pkos/tunnel-api-key \
  && chmod 600 ~/.config/pkos/tunnel-api-key \
  && unset K
```

Tunnel設定例:

```yaml
control_plane:
  tunnel_id: tunnel_...
  api_key: file:/home/YOUR_USER/.config/pkos/tunnel-api-key

mcp:
  server_urls:
    - channel: main
      url: http://127.0.0.1:8787/
  extra_headers:
    Authorization: Bearer YOUR_PKOS_MCP_BEARER_TOKEN

health:
  listen_addr: 127.0.0.1:8788

log:
  level: info
  format: struct-text
```

`YOUR_PKOS_MCP_BEARER_TOKEN`は`.env`の`PKOS_MCP_BEARER_TOKEN`と同じ値です。この値はTunnel clientからローカルMCPへの最終ホップにだけ付与されます。

## 4. ChatGPTで接続確認する

接続後、ChatGPTに次のように頼みます。

```text
Personal Knowledge OSで pkos_status を実行して
```

`gateway: ok`とPKOS backendのreadinessが返れば接続できています。

次に保存テスト:

```text
今後の技術判断として「LLM GatewayはBifrostを第一候補にする」と覚えておいて
```

ChatGPTは`remember`を呼び、PKOSでは次の経路を通ります。

```text
ChatGPT
 -> remember
 -> /v1/captures
 -> Source
 -> extract_memories
 -> LLM proposal
 -> evidence grounding verifier
 -> policy/threshold checks
 -> Active Memory
```

検索テスト:

```text
前にLLM Gatewayについて何を決めた？Personal Knowledge OSを検索して
```

`search_knowledge`からSourceとActive Memoryを取得できます。

## 自動保存の考え方

MCPサーバーのinstructionsには、ユーザーが「覚えて」と言うまで待たず、将来役立つ長期情報がユーザー本人の発言に明確に根拠づけられる場合は`remember`を使うよう記述しています。

ただし、MCPはChatGPTの全会話をOSレベルで盗み見る仕組みではありません。ChatGPTがMCPツールを呼んだ内容だけがPKOSに届きます。そのため、最終的な自動保存率はChatGPT側のツール選択にも依存します。

保存対象の例:

- 明示的な意思決定
- 長期的な好み
- 目標・制約
- プロジェクトの重要な状態変化
- 継続的な習慣
- スキルや経験

保存しないもの:

- パスワード
- API key / access token
- private key
- 一時的な雑談だけの内容
- Assistantだけが推測したユーザー情報

## セキュリティ境界

- `PKOS_API_KEY`はMCPコンテナだけが保持し、ChatGPT/Tunnelへ渡しません。
- `PKOS_MCP_BEARER_TOKEN`はChatGPTへ登録せず、Tunnel clientがlocalhostへの最終ホップで注入します。
- PostgreSQL、Rust API、MCPのHost portはloopback限定です。
- MemoryはLLMから直接Activeにならず、Source evidenceとdeterministic verifierを通ります。
- `/.well-known/*`は404です。OAuth serverを公開する構成ではありません。

## 利用可能プランについて

ChatGPTのカスタムMCP機能、とくに書き込み/変更ツールの提供範囲はOpenAI側で段階的に変わります。MCPサーバー自体はread/write両方に対応していますが、ChatGPTアカウント側で書き込みMCPが有効になっていない場合、`remember`や`capture_conversation`はChatGPTからは実行できません。その場合でも、サーバー実装は他のMCPクライアントから利用できます。
