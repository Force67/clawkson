const BASE = ''

// ── Primitive fetch helper ─────────────────────────────────────────

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    credentials: 'include',
    headers: { 'Content-Type': 'application/json' },
    ...init,
  })
  if (!res.ok) {
    let detail = res.statusText
    try {
      const body = await res.json()
      if (body?.error) detail = body.error
    } catch { /* no JSON body */ }
    throw new Error(`${res.status} ${detail}`)
  }
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
  owner_id?: string
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

export interface KnowledgeBase {
  id: string
  owner_id: string
  name: string
  description: string
  embedding_model: string
  entry_count: number
  created_at: string
  updated_at: string
}

export interface KnowledgeEntry {
  id: string
  knowledge_base_id: string
  title: string
  content: string
  token_count: number | null
  has_embedding: boolean
  created_at: string
  updated_at: string
}

export interface KnowledgeSearchResult {
  entry: KnowledgeEntry
  score: number
}

export interface UploadResult {
  files_processed: number
  entries_created: number
  errors: string[]
}

export interface KbShareInfo {
  id: string
  user_id: string
  email: string
  display_name: string
  permission: SharePermission
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

// ── Auth types ────────────────────────────────────────────────────

export type UserRole = 'admin' | 'user'

export interface User {
  id: string
  email: string
  display_name: string
  role: UserRole
  created_at: string
  updated_at: string
}

export interface AuthResponse {
  user: User
}

export type SharePermission = 'read' | 'write'

export interface ConversationShare {
  id: string
  conversation_id: string
  shared_by: string
  shared_with: string
  permission: SharePermission
  created_at: string
}

export interface ShareUserInfo {
  id: string
  email: string
  display_name: string
}

export interface ShareResponse {
  share: ConversationShare
  shared_with_user: ShareUserInfo
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
  auth: {
    register: (body: { email: string; password: string; display_name?: string }) =>
      request<AuthResponse>('/api/auth/register', { method: 'POST', body: JSON.stringify(body) }),
    login: (body: { email: string; password: string }) =>
      request<AuthResponse>('/api/auth/login', { method: 'POST', body: JSON.stringify(body) }),
    logout: () =>
      request<void>('/api/auth/logout', { method: 'POST' }),
    me: () =>
      request<AuthResponse>('/api/auth/me'),
  },

  admin: {
    listUsers: () => request<User[]>('/api/admin/users'),
    updateRole: (id: string, role: UserRole) =>
      request<User>(`/api/admin/users/${id}/role`, { method: 'PATCH', body: JSON.stringify({ role }) }),
    deleteUser: (id: string) =>
      request<void>(`/api/admin/users/${id}`, { method: 'DELETE' }),
  },

  shares: {
    list: (conversationId: string) =>
      request<ShareResponse[]>(`/api/conversations/${conversationId}/shares`),
    create: (conversationId: string, email: string, permission: SharePermission) =>
      request<ShareResponse>(`/api/conversations/${conversationId}/shares`, {
        method: 'POST',
        body: JSON.stringify({ email, permission }),
      }),
    remove: (conversationId: string, userId: string) =>
      request<void>(`/api/conversations/${conversationId}/shares/${userId}`, { method: 'DELETE' }),
  },

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
    // Bases
    listBases: () => request<KnowledgeBase[]>('/api/knowledge'),
    getBase: (id: string) => request<KnowledgeBase>(`/api/knowledge/${id}`),
    createBase: (body: { name: string; description?: string; embedding_model?: string }) =>
      request<KnowledgeBase>('/api/knowledge', { method: 'POST', body: JSON.stringify(body) }),
    patchBase: (id: string, body: { name?: string; description?: string }) =>
      request<KnowledgeBase>(`/api/knowledge/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
    deleteBase: (id: string) =>
      request<void>(`/api/knowledge/${id}`, { method: 'DELETE' }),
    // Entries
    listEntries: (kbId: string) => request<KnowledgeEntry[]>(`/api/knowledge/${kbId}/entries`),
    createEntry: (kbId: string, body: { title: string; content: string }) =>
      request<KnowledgeEntry>(`/api/knowledge/${kbId}/entries`, { method: 'POST', body: JSON.stringify(body) }),
    patchEntry: (kbId: string, entryId: string, body: { title?: string; content?: string }) =>
      request<KnowledgeEntry>(`/api/knowledge/${kbId}/entries/${entryId}`, { method: 'PATCH', body: JSON.stringify(body) }),
    deleteEntry: (kbId: string, entryId: string) =>
      request<void>(`/api/knowledge/${kbId}/entries/${entryId}`, { method: 'DELETE' }),
    // File upload (multipart)
    uploadFiles: async (kbId: string, files: File[]): Promise<UploadResult> => {
      const form = new FormData()
      for (const file of files) form.append('files', file)
      const res = await fetch(`${BASE}/api/knowledge/${kbId}/upload`, {
        method: 'POST',
        credentials: 'include',
        body: form,
      })
      if (!res.ok) {
        let detail = res.statusText
        try { const body = await res.json(); if (body?.error) detail = body.error } catch {}
        throw new Error(`${res.status} ${detail}`)
      }
      return res.json()
    },
    // Embeddings
    embed: (kbId: string) =>
      request<{ embedded: number; failed: number }>(`/api/knowledge/${kbId}/embed`, { method: 'POST' }),
    // Search
    search: (kbId: string, query: string, limit?: number) =>
      request<KnowledgeSearchResult[]>(`/api/knowledge/${kbId}/search`, {
        method: 'POST',
        body: JSON.stringify({ query, limit }),
      }),
    // Sharing
    listShares: (kbId: string) => request<KbShareInfo[]>(`/api/knowledge/${kbId}/shares`),
    createShare: (kbId: string, email: string, permission: SharePermission) =>
      request<KbShareInfo>(`/api/knowledge/${kbId}/shares`, {
        method: 'POST',
        body: JSON.stringify({ email, permission }),
      }),
    removeShare: (kbId: string, userId: string) =>
      request<void>(`/api/knowledge/${kbId}/shares/${userId}`, { method: 'DELETE' }),
    // Agent access
    listAgents: (kbId: string) => request<string[]>(`/api/knowledge/${kbId}/agents`),
    linkAgent: (kbId: string, agentId: string) =>
      request<void>(`/api/knowledge/${kbId}/agents`, { method: 'POST', body: JSON.stringify({ agent_id: agentId }) }),
    unlinkAgent: (kbId: string, agentId: string) =>
      request<void>(`/api/knowledge/${kbId}/agents/${agentId}`, { method: 'DELETE' }),
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
    credentials: 'include',
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
