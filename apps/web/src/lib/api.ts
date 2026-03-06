const BASE = ''

// ── Primitive fetch helper ─────────────────────────────────────────

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  })
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}`)
  if (res.status === 204) return undefined as T
  return res.json() as Promise<T>
}

// ── Domain types ───────────────────────────────────────────────────

export type AgentStatus = 'online' | 'offline' | 'busy' | 'error'

export interface Agent {
  id: string
  name: string
  description: string
  status: AgentStatus
  llm_connector_id: string | null
  system_prompt: string | null
  temperature: number | null
  max_tokens: number | null
  created_at: string
  updated_at: string
}

export interface Conversation {
  id: string
  title: string
  agent_id: string
  created_at: string
  updated_at: string
}

export type MessageRole = 'user' | 'assistant' | 'system' | 'tool'

export interface Message {
  id: string
  conversation_id: string
  role: MessageRole
  content: string
  created_at: string
}

export interface ChatResponse {
  user_message: Message
  assistant_message: Message
}

export type ConnectorType = 'telegram' | 'gmail' | 'slack' | 'custom'

export interface Connector {
  id: string
  name: string
  connector_type: ConnectorType
  enabled: boolean
  config: Record<string, unknown>
  created_at: string
}

export interface KnowledgeEntry {
  id: string
  title: string
  content: string
  tags: string[]
  created_at: string
  updated_at: string
}

export interface Tool {
  id: string
  name: string
  description: string
  connector_id: string
  schema: Record<string, unknown>
  enabled: boolean
}

export type LlmProviderType = 'azure' | 'open_router' | 'open_ai' | 'custom'

export interface LlmConnector {
  id: string
  name: string
  provider_type: LlmProviderType
  api_key: string       // masked on retrieval
  api_base_url: string
  model: string
  azure_deployment: string | null
  azure_api_version: string | null
  created_at: string
}

export interface Settings {
  default_llm_connector_id: string | null
  theme: string
}

// ── Create / patch request types ───────────────────────────────────

export interface CreateAgentRequest {
  name: string
  description: string
  llm_connector_id?: string
  system_prompt?: string
  temperature?: number
  max_tokens?: number
}

export interface PatchAgentRequest {
  name?: string
  description?: string
  llm_connector_id?: string
  system_prompt?: string
  temperature?: number
  max_tokens?: number
  status?: AgentStatus
}

export interface CreateConversationRequest {
  title: string
  agent_id: string
}

export interface CreateLlmConnectorRequest {
  name: string
  provider_type: LlmProviderType
  api_key: string
  api_base_url?: string
  model: string
  azure_deployment?: string
  azure_api_version?: string
}

export interface PatchLlmConnectorRequest {
  name?: string
  provider_type?: LlmProviderType
  api_key?: string
  api_base_url?: string
  model?: string
  azure_deployment?: string
  azure_api_version?: string
}

export interface CreateConnectorRequest {
  name: string
  connector_type: ConnectorType
  config: Record<string, unknown>
}

export interface PatchSettingsRequest {
  default_llm_connector_id?: string
  theme?: string
}

// ── API client ─────────────────────────────────────────────────────

export const api = {
  agents: {
    list: () => request<Agent[]>('/api/agents'),
    get: (id: string) => request<Agent>(`/api/agents/${id}`),
    create: (body: CreateAgentRequest) =>
      request<Agent>('/api/agents', { method: 'POST', body: JSON.stringify(body) }),
    patch: (id: string, body: PatchAgentRequest) =>
      request<Agent>(`/api/agents/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
    delete: (id: string) =>
      request<void>(`/api/agents/${id}`, { method: 'DELETE' }),
  },

  conversations: {
    list: () => request<Conversation[]>('/api/conversations'),
    get: (id: string) => request<Conversation>(`/api/conversations/${id}`),
    create: (body: CreateConversationRequest) =>
      request<Conversation>('/api/conversations', { method: 'POST', body: JSON.stringify(body) }),
    messages: (id: string) => request<Message[]>(`/api/conversations/${id}/messages`),
    chat: (id: string, content: string) =>
      request<ChatResponse>(`/api/conversations/${id}/chat`, {
        method: 'POST',
        body: JSON.stringify({ content }),
      }),
  },

  llmConnectors: {
    list: () => request<LlmConnector[]>('/api/llm-connectors'),
    get: (id: string) => request<LlmConnector>(`/api/llm-connectors/${id}`),
    create: (body: CreateLlmConnectorRequest) =>
      request<LlmConnector>('/api/llm-connectors', { method: 'POST', body: JSON.stringify(body) }),
    patch: (id: string, body: PatchLlmConnectorRequest) =>
      request<LlmConnector>(`/api/llm-connectors/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
    delete: (id: string) =>
      request<void>(`/api/llm-connectors/${id}`, { method: 'DELETE' }),
    test: (body: CreateLlmConnectorRequest) =>
      request<{ ok: boolean; latency_ms: number; error?: string }>(
        '/api/llm-connectors/test',
        { method: 'POST', body: JSON.stringify(body) },
      ),
  },

  settings: {
    get: () => request<Settings>('/api/settings'),
    patch: (body: PatchSettingsRequest) =>
      request<Settings>('/api/settings', { method: 'PATCH', body: JSON.stringify(body) }),
  },

  connectors: {
    list: () => request<Connector[]>('/api/connectors'),
    create: (body: CreateConnectorRequest) =>
      request<Connector>('/api/connectors', { method: 'POST', body: JSON.stringify(body) }),
    patch: (id: string, body: { enabled?: boolean }) =>
      request<Connector>(`/api/connectors/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
    delete: (id: string) =>
      request<void>(`/api/connectors/${id}`, { method: 'DELETE' }),
  },

  knowledge: {
    list: () => request<KnowledgeEntry[]>('/api/knowledge'),
    create: (body: { title: string; content: string; tags: string[] }) =>
      request<KnowledgeEntry>('/api/knowledge', { method: 'POST', body: JSON.stringify(body) }),
  },
}

// ── SSE streaming helper ───────────────────────────────────────────

export interface StreamChunk {
  delta?: string
  done?: boolean
  id?: string
  error?: string
}

/**
 * Send a chat message and stream the assistant response via SSE.
 * @param conversationId  The conversation to post to
 * @param content         The user message text
 * @param onChunk         Called for each streamed text delta
 * @param onDone          Called when the stream completes (with final message id)
 * @param onError         Called on network/API error
 */
export function streamChat(
  conversationId: string,
  content: string,
  onChunk: (text: string) => void,
  onDone: (msgId: string) => void,
  onError: (err: string) => void,
): () => void {
  const controller = new AbortController()

  fetch(`${BASE}/api/conversations/${conversationId}/chat/stream`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ content }),
    signal: controller.signal,
  })
    .then(async (res) => {
      if (!res.ok) throw new Error(`${res.status} ${res.statusText}`)
      const reader = res.body!.getReader()
      const decoder = new TextDecoder()
      let buf = ''

      while (true) {
        const { done, value } = await reader.read()
        if (done) break
        buf += decoder.decode(value, { stream: true })

        const lines = buf.split('\n')
        buf = lines.pop() ?? ''

        for (const line of lines) {
          const trimmed = line.trim()
          if (!trimmed.startsWith('data:')) continue
          const data = trimmed.slice(5).trim()
          try {
            const chunk: StreamChunk = JSON.parse(data)
            if (chunk.error) { onError(chunk.error); return }
            if (chunk.done) { onDone(chunk.id ?? ''); return }
            if (chunk.delta) onChunk(chunk.delta)
          } catch {
            // ignore malformed lines
          }
        }
      }
    })
    .catch((err) => {
      if (err.name !== 'AbortError') onError(String(err))
    })

  return () => controller.abort()
}
