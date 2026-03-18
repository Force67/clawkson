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
  if (res.status === 204 || res.status === 201) {
    // Try to parse JSON body if present, otherwise return undefined
    const text = await res.text()
    if (!text) return undefined as T
    try { return JSON.parse(text) as T } catch { return undefined as T }
  }
  return res.json() as Promise<T>
}

// ── Domain types ───────────────────────────────────────────────────

export type AgentStatus = 'online' | 'offline' | 'busy' | 'error'

export interface AgentContainerConfig {
  cpu_limit: number | null
  memory_limit_mb: number | null
  network_enabled: boolean
  permissions?: AgentPermissions
}

// ── Agent Permissions (Android-style) ─────────────────────────────

export type FilesystemMode = 'read_write' | 'read_only' | 'none'

export interface NetworkPermission {
  enabled: boolean
  internet: boolean
  local_network: boolean
  allowed_domains: string[]
}

export interface FilesystemPermission {
  mode: FilesystemMode
  max_workspace_size_mb: number | null
}

export interface ExecutionPermission {
  shell: boolean
  python: boolean
  allowed_runtimes: string[]
  max_execution_time_secs: number | null
}

export interface ResourcePermission {
  max_processes: number | null
  max_tmp_size_mb: number | null
  max_storage_size_mb: number | null
  readonly_rootfs: boolean
}

export interface DataAccessPermission {
  knowledge_bases: boolean
  conversation_history: boolean
}

export interface AgentPermissions {
  network: NetworkPermission
  filesystem: FilesystemPermission
  execution: ExecutionPermission
  resources: ResourcePermission
  data_access: DataAccessPermission
}

export interface ContainerStatus {
  agent_id: string
  conversation_id: string
  state: string
  image: string
  workspace_path: string
}

export interface ExecResult {
  stdout: string
  stderr: string
  exit_code: number
  timed_out: boolean
  output_files?: OutputFile[]
}

export interface OutputFile {
  path: string
  size: number
}

export interface WorkspaceEntry {
  name: string
  path: string
  is_dir: boolean
  size: number
  modified_at: string | null
}

export interface WorkspaceListing {
  path: string
  entries: WorkspaceEntry[]
}

export interface WorkspaceUploadResponse {
  uploaded: string[]
  errors: string[]
}

export interface Agent {
  id: string
  name: string
  description: string
  status: AgentStatus
  llm_connector_id: string | null
  system_prompt: string | null
  temperature: number | null
  max_tokens: number | null
  container_enabled: boolean
  container_config: AgentContainerConfig | null
  connector_policies: ConnectorPolicy[]
  subtask_llm_connector_id: string | null
  owner_id?: string | null
  shared: boolean
  created_at: string
  updated_at: string
}

export interface Conversation {
  id: string
  title: string
  agent_id: string
  owner_id?: string
  pinned: boolean
  archived: boolean
  created_at: string
  updated_at: string
}

export type MessageRole = 'user' | 'assistant' | 'system' | 'tool'

export interface OfficeMetadata {
  type: 'spreadsheet' | 'presentation' | 'document'
  sheet_count?: number
  sheet_names?: string[]
  slide_count?: number
  paragraph_count?: number
}

export interface MessageAttachment {
  id: string
  filename: string
  content_type: string
  size_bytes: number
  metadata?: OfficeMetadata
}

export interface Message {
  id: string
  conversation_id: string
  role: MessageRole
  content: string
  created_at: string
  attachments?: MessageAttachment[]
}

export interface ChatResponse {
  user_message: Message
  assistant_message: Message
}

export type ConnectorType = 'telegram' | 'gmail' | 'slack' | 'azure_devops' | 'custom' | 'tavily' | 'bing'

export interface Connector {
  id: string
  user_id: string
  name: string
  connector_type: ConnectorType
  enabled: boolean
  config: Record<string, unknown>
  /** Free-text operational context injected when this connector is invoked. */
  context: string
  created_at: string
  updated_at: string
}

// ── Connector Policies ──────────────────────────────────────────

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'HEAD' | 'OPTIONS'

export interface ProxyRule {
  methods: HttpMethod[]
  path_pattern: string
  description: string
}

export interface ConnectorPolicy {
  connector_id: string
  allow: ProxyRule[]
  deny: ProxyRule[]
  rate_limit_rpm: number | null
}

export interface PolicyPreset {
  name: string
  label: string
  connector_type: string
  policy: {
    allow: ProxyRule[]
    deny: ProxyRule[]
    rate_limit_rpm: number | null
  }
}

export interface KnowledgeBase {
  id: string
  owner_id: string
  name: string
  description: string
  kb_type: string
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
  source_document_id?: string
  created_at: string
  updated_at: string
}

export interface KnowledgeSearchResult {
  entry: KnowledgeEntry
  score: number
  document_url?: string
}

export interface UploadResult {
  files_processed: number
  entries_created: number
  embedded: number
  embed_failed: number
  errors: string[]
}

export interface KbShareInfo {
  id: string
  user_id: string
  email: string
  display_name: string
  permission: SharePermission
}

export type ToolType = 'builtin' | 'connector'

export interface Tool {
  id: string
  name: string
  description: string
  /** Present only for connector-derived tools */
  connector_id?: string
  tool_type: ToolType
  enabled: boolean
}

export interface Skill {
  id: string
  name: string
  description: string
  instructions: string
  created_at: string
  updated_at: string
}

export interface CreateSkillRequest {
  name: string
  description: string
  instructions?: string
}

export interface PatchSkillRequest {
  name?: string
  description?: string
  instructions?: string
}

export interface SkillTemplate {
  name: string
  description: string
  instructions: string
}

export interface AgentSkillInfo {
  id: string
  name: string
  description: string
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
  /** LLM connector used for ETL semantic chunking. Null = heuristic only. */
  etl_llm_connector_id: string | null
  theme: string
  agent_base_prompt: string
  /** Maximum seconds to wait for an LLM HTTP response. Range 10–600. Default 120. */
  llm_request_timeout_secs: number
  /** OpenAI-compatible base URL for the embedding provider. */
  embedding_api_base_url: string
  /** API key for the embedding provider (masked on retrieval). */
  embedding_api_key: string
  /** Model name for embedding generation. */
  embedding_model: string
}

// ── Auth types ────────────────────────────────────────────────────

export type UserRole = 'admin' | 'user'

export interface User {
  id: string
  email: string
  display_name: string
  role: UserRole
  /** Free-text context about the user that agents can read. */
  bio: string
  /** URL of the user's avatar image (data URL or remote URL). */
  avatar_url: string
  created_at: string
  updated_at: string
}

export interface AuthResponse {
  user: User
}

export type SharePermission = 'read' | 'write'
export type EventCategory = 'work' | 'personal' | 'meeting' | 'health' | 'travel' | 'creative'

export interface CalendarEvent {
  id: string
  owner_id: string
  title: string
  date: string
  start_time: string
  end_time: string
  category: EventCategory
  location?: string
  notes?: string
  completed: boolean
  created_at: string
  updated_at: string
}

export interface CalendarShareResponse {
  share: CalendarShareInfo
  user: ShareUserInfo
}

export interface CalendarShareInfo {
  id: string
  owner_id: string
  shared_with: string
  permission: SharePermission
  created_at: string
}

export interface SharedCalendar {
  owner_id: string
  email: string
  display_name: string
  permission: SharePermission
}

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

// ── Scheduled Tasks ─────────────────────────────────────────────────

export interface ScheduledTask {
  id: string
  owner_id: string
  agent_id: string
  name: string
  prompt: string
  cron_expression?: string | null
  enabled: boolean
  last_run_at?: string | null
  next_run_at?: string | null
  created_at: string
  updated_at: string
}

export interface TaskOutputFile {
  id: string
  filename: string
  content_type: string
  size_bytes: number
}

export interface TaskExecution {
  id: string
  task_id: string
  conversation_id?: string | null
  status: string
  result_summary?: string | null
  error_message?: string | null
  started_at: string
  completed_at?: string | null
  duration_ms?: number | null
  output_files?: TaskOutputFile[]
}

export interface CreateScheduledTaskRequest {
  name: string
  agent_id: string
  prompt: string
  cron_expression?: string | null
}

export interface PatchScheduledTaskRequest {
  name?: string
  prompt?: string
  cron_expression?: string | null
  enabled?: boolean
}

// ── Create / patch request types ───────────────────────────────────

export interface CreateAgentRequest {
  name: string
  description: string
  llm_connector_id?: string
  system_prompt?: string
  temperature?: number
  max_tokens?: number
  container_enabled?: boolean
  container_config?: AgentContainerConfig
  connector_policies?: ConnectorPolicy[]
  shared?: boolean
  subtask_llm_connector_id?: string
}

export interface PatchAgentRequest {
  name?: string
  description?: string
  llm_connector_id?: string
  system_prompt?: string
  temperature?: number
  max_tokens?: number
  status?: AgentStatus
  container_enabled?: boolean
  container_config?: AgentContainerConfig
  connector_policies?: ConnectorPolicy[]
  shared?: boolean
  subtask_llm_connector_id?: string
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

export interface PatchConnectorRequest {
  enabled?: boolean
  context?: string
  name?: string
  config?: Record<string, unknown>
}

export interface PatchProfileRequest {
  display_name?: string
  bio?: string
  avatar_url?: string
}

export interface PatchSettingsRequest {
  default_llm_connector_id?: string
  /** Set to a connector id to enable LLM semantic chunking, or omit to keep existing. */
  etl_llm_connector_id?: string | null
  theme?: string
  agent_base_prompt?: string
  /** Maximum seconds to wait for LLM responses. Range 10–600. */
  llm_request_timeout_secs?: number
  /** OpenAI-compatible base URL for the embedding provider. */
  embedding_api_base_url?: string
  /** API key for the embedding provider. */
  embedding_api_key?: string
  /** Model name for embedding generation. */
  embedding_model?: string
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
    patchProfile: (body: PatchProfileRequest) =>
      request<AuthResponse>('/api/auth/profile', { method: 'PATCH', body: JSON.stringify(body) }),
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
    delete: (id: string) =>
      request<void>(`/api/conversations/${id}`, { method: 'DELETE' }),
    patch: (id: string, body: { title?: string; pinned?: boolean }) =>
      request<Conversation>(`/api/conversations/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
    deleteAll: () =>
      request<void>('/api/conversations', { method: 'DELETE' }),
    messages: (id: string) => request<Message[]>(`/api/conversations/${id}/messages`),
    clearMessages: (id: string) =>
      request<void>(`/api/conversations/${id}/messages`, { method: 'DELETE' }),
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

  containers: {
    list: () => request<ContainerStatus[]>('/api/containers'),
    status: (agentId: string, conversationId: string) =>
      request<ContainerStatus>(`/api/agents/${agentId}/container?conversation_id=${conversationId}`),
    start: (agentId: string, conversationId: string) =>
      request<ContainerStatus>(`/api/agents/${agentId}/container/start?conversation_id=${conversationId}`, { method: 'POST' }),
    stop: (agentId: string, conversationId: string) =>
      request<void>(`/api/agents/${agentId}/container/stop?conversation_id=${conversationId}`, { method: 'POST' }),
    remove: (agentId: string, conversationId: string) =>
      request<void>(`/api/agents/${agentId}/container?conversation_id=${conversationId}`, { method: 'DELETE' }),
    logs: (agentId: string, conversationId: string, tail?: number) =>
      request<{ logs: string }>(`/api/agents/${agentId}/container/logs?conversation_id=${conversationId}${tail ? `&tail=${tail}` : ''}`),
    exec: (agentId: string, conversationId: string, command: string, timeout?: number, outputDir?: string) =>
      request<ExecResult>(`/api/agents/${agentId}/container/exec`, {
        method: 'POST',
        body: JSON.stringify({ conversation_id: conversationId, command, timeout, output_dir: outputDir }),
      }),
    workspace: {
      list: (agentId: string, conversationId: string, path?: string) =>
        request<WorkspaceListing>(
          `/api/agents/${agentId}/container/workspace?conversation_id=${conversationId}${path ? `&path=${encodeURIComponent(path)}` : ''}`
        ),
      upload: async (agentId: string, conversationId: string, files: File[], path?: string): Promise<WorkspaceUploadResponse> => {
        const form = new FormData()
        form.append('conversation_id', conversationId)
        if (path) form.append('path', path)
        for (const f of files) form.append('files', f)
        const res = await fetch(`${BASE}/api/agents/${agentId}/container/workspace/upload`, {
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
      downloadUrl: (agentId: string, conversationId: string, path: string) =>
        `${BASE}/api/agents/${agentId}/container/workspace/download?conversation_id=${conversationId}&path=${encodeURIComponent(path)}`,
      delete: (agentId: string, conversationId: string, path: string, recursive?: boolean) =>
        request<void>(`/api/agents/${agentId}/container/workspace`, {
          method: 'DELETE',
          body: JSON.stringify({ conversation_id: conversationId, path, recursive: recursive ?? false }),
        }),
      watchUrl: (agentId: string, conversationId: string) =>
        `${BASE}/api/agents/${agentId}/container/workspace/watch?conversation_id=${conversationId}`,
    },
  },

  skills: {
    list: () => request<Skill[]>('/api/skills'),
    get: (id: string) => request<Skill>(`/api/skills/${id}`),
    create: (body: CreateSkillRequest) =>
      request<Skill>('/api/skills', { method: 'POST', body: JSON.stringify(body) }),
    patch: (id: string, body: PatchSkillRequest) =>
      request<Skill>(`/api/skills/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
    delete: (id: string) =>
      request<void>(`/api/skills/${id}`, { method: 'DELETE' }),
    listAgents: (id: string) => request<string[]>(`/api/skills/${id}/agents`),
    templates: () => request<SkillTemplate[]>('/api/skills/templates'),
  },

  agentSkills: {
    list: (agentId: string) => request<string[]>(`/api/agents/${agentId}/skills`),
    link: (agentId: string, skillId: string) =>
      request<void>(`/api/agents/${agentId}/skills`, { method: 'POST', body: JSON.stringify({ skill_id: skillId }) }),
    unlink: (agentId: string, skillId: string) =>
      request<void>(`/api/agents/${agentId}/skills/${skillId}`, { method: 'DELETE' }),
    full: (agentId: string) => request<AgentSkillInfo[]>(`/api/agents/${agentId}/skills/full`),
  },

  tools: {
    list: () => request<Tool[]>('/api/tools'),
  },

  policyPresets: {
    list: () => request<PolicyPreset[]>('/api/policy-presets'),
  },

  connectors: {
    list: () => request<Connector[]>('/api/connectors'),
    create: (body: CreateConnectorRequest) =>
      request<Connector>('/api/connectors', { method: 'POST', body: JSON.stringify(body) }),
    patch: (id: string, body: PatchConnectorRequest) =>
      request<Connector>(`/api/connectors/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
    delete: (id: string) =>
      request<void>(`/api/connectors/${id}`, { method: 'DELETE' }),
  },

  knowledge: {
    // Bases
    listBases: () => request<KnowledgeBase[]>('/api/knowledge'),
    getMemory: () => request<KnowledgeBase>('/api/knowledge/memory'),
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

  calendar: {
    list: (params?: { from?: string; to?: string; owner_id?: string }) => {
      const qs = new URLSearchParams()
      if (params?.from) qs.set('from', params.from)
      if (params?.to) qs.set('to', params.to)
      if (params?.owner_id) qs.set('owner_id', params.owner_id)
      const query = qs.toString()
      return request<CalendarEvent[]>(`/api/calendar${query ? `?${query}` : ''}`)
    },
    get: (id: string) => request<CalendarEvent>(`/api/calendar/${id}`),
    create: (body: {
      title: string; date: string; start_time: string; end_time: string;
      category?: string; location?: string; notes?: string
    }) =>
      request<CalendarEvent>('/api/calendar', { method: 'POST', body: JSON.stringify(body) }),
    update: (id: string, body: {
      title: string; date: string; start_time: string; end_time: string;
      category: string; location?: string; notes?: string; completed?: boolean
    }) =>
      request<CalendarEvent>(`/api/calendar/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
    delete: (id: string) =>
      request<void>(`/api/calendar/${id}`, { method: 'DELETE' }),
    toggleComplete: (id: string, completed: boolean) =>
      request<void>(`/api/calendar/${id}/complete`, { method: 'PATCH', body: JSON.stringify({ completed }) }),
    // Sharing
    listShares: () => request<CalendarShareResponse[]>('/api/calendar/shares'),
    createShare: (email: string, permission: SharePermission) =>
      request<CalendarShareResponse>('/api/calendar/shares', {
        method: 'POST', body: JSON.stringify({ email, permission }),
      }),
    removeShare: (userId: string) =>
      request<void>(`/api/calendar/shares/${userId}`, { method: 'DELETE' }),
    listSharedCalendars: () => request<SharedCalendar[]>('/api/calendar/shared'),
    listSharedEvents: (ownerId: string, params?: { from?: string; to?: string }) => {
      const qs = new URLSearchParams()
      if (params?.from) qs.set('from', params.from)
      if (params?.to) qs.set('to', params.to)
      const query = qs.toString()
      return request<CalendarEvent[]>(`/api/calendar/shared/${ownerId}${query ? `?${query}` : ''}`)
    },
  },

  scheduledTasks: {
    list: () => request<ScheduledTask[]>('/api/scheduled-tasks'),
    get: (id: string) => request<ScheduledTask>(`/api/scheduled-tasks/${id}`),
    create: (body: CreateScheduledTaskRequest) =>
      request<ScheduledTask>('/api/scheduled-tasks', { method: 'POST', body: JSON.stringify(body) }),
    patch: (id: string, body: PatchScheduledTaskRequest) =>
      request<ScheduledTask>(`/api/scheduled-tasks/${id}`, { method: 'PATCH', body: JSON.stringify(body) }),
    delete: (id: string) =>
      request<void>(`/api/scheduled-tasks/${id}`, { method: 'DELETE' }),
    run: (id: string) =>
      request<TaskExecution>(`/api/scheduled-tasks/${id}/run`, { method: 'POST' }),
    history: (id: string) =>
      request<TaskExecution[]>(`/api/scheduled-tasks/${id}/history`),
  },

  uploads: {
    upload: async (files: File[], conversationId?: string, onProgress?: (loaded: number, total: number) => void): Promise<UploadFilesResponse> => {
      const form = new FormData()
      for (const f of files) form.append('files', f)
      if (conversationId) form.append('conversation_id', conversationId)

      // Use XHR when progress callback is provided for byte-level tracking
      if (onProgress) {
        return new Promise((resolve, reject) => {
          const xhr = new XMLHttpRequest()
          xhr.open('POST', `${BASE}/api/uploads`)
          xhr.withCredentials = true
          xhr.upload.addEventListener('progress', (e) => {
            if (e.lengthComputable) onProgress(e.loaded, e.total)
          })
          xhr.addEventListener('load', () => {
            if (xhr.status >= 200 && xhr.status < 300) {
              resolve(JSON.parse(xhr.responseText))
            } else {
              let detail = xhr.statusText
              try { const body = JSON.parse(xhr.responseText); if (body?.error) detail = body.error } catch {}
              reject(new Error(`${xhr.status} ${detail}`))
            }
          })
          xhr.addEventListener('error', () => reject(new Error('Upload failed')))
          xhr.addEventListener('abort', () => reject(new Error('Upload aborted')))
          xhr.send(form)
        })
      }

      const res = await fetch(`${BASE}/api/uploads`, {
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
    delete: (id: string) =>
      request<void>(`/api/uploads/${id}`, { method: 'DELETE' }),
    downloadUrl: (id: string) => `${BASE}/api/uploads/${id}`,
  },
}

// ── SSE streaming helper ───────────────────────────────────────────

export interface ToolEvent {
  type: 'tool_start' | 'tool_end' | 'subtask_start' | 'subtask_end' | 'image_output'
  name: string
  round: number
  description?: string
  ok?: boolean
  result?: string
  duration_ms?: number
  /** Sub-task specific fields */
  id?: string
  total?: number
  completed?: number
  /** image_output fields */
  url?: string
  filename?: string
}

export interface PreviewInfo {
  preview_url: string
  port: number
  title: string
}

export interface StreamChunk {
  delta?: string
  reasoning_delta?: string
  tool_event?: ToolEvent
  done?: boolean
  id?: string
  error?: string
}

export type ReasoningEffort = 'low' | 'medium' | 'high'

export interface ChatStreamOptions {
  reasoning_effort?: ReasoningEffort
  search_enabled?: boolean
  attachment_ids?: string[]
}

export interface AttachmentInfo {
  id: string
  filename: string
  content_type: string
  size_bytes: number
  created_at: string
}

export interface UploadFilesResponse {
  files: AttachmentInfo[]
}

/**
 * Send a chat message and stream the assistant response via SSE.
 * @param conversationId  The conversation to post to
 * @param content         The user message text
 * @param onChunk         Called for each streamed text delta
 * @param onDone          Called when the stream completes (with final message id)
 * @param onError         Called on network/API error
 * @param onReasoning     Called for each reasoning/thinking delta
 * @param options         Optional reasoning_effort and search_enabled flags
 */
export function streamChat(
  conversationId: string,
  content: string,
  onChunk: (text: string) => void,
  onDone: (msgId: string) => void,
  onError: (err: string) => void,
  onReasoning?: (text: string) => void,
  options?: ChatStreamOptions,
  onToolEvent?: (event: ToolEvent) => void,
): () => void {
  const controller = new AbortController()

  const body: Record<string, unknown> = { content }
  if (options?.reasoning_effort) body.reasoning_effort = options.reasoning_effort
  if (options?.search_enabled !== undefined) body.search_enabled = options.search_enabled
  if (options?.attachment_ids?.length) body.attachment_ids = options.attachment_ids

  fetch(`${BASE}/api/conversations/${conversationId}/chat/stream`, {
    method: 'POST',
    credentials: 'include',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
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
            if (chunk.tool_event && onToolEvent) onToolEvent(chunk.tool_event)
            if (chunk.reasoning_delta && onReasoning) onReasoning(chunk.reasoning_delta)
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
