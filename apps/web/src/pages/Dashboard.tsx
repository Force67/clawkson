import { useState, useEffect } from 'react'
import {
  Activity,
  Bot,
  MessageCircle,
  Plug,
  TrendingUp,
  Clock,
  Plus,
  Settings2,
  Trash2,
  Check,
  ChevronDown,
  Loader2,
  Cpu,
  Thermometer,
  Hash,
  Zap,
  Container,
  Play,
  Square,
  BookOpen,
} from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { StatusBadge } from '../components/StatusBadge'
import { Button } from '../components/Button'
import { api, type Agent, type Conversation, type LlmConnector, type KnowledgeBase, type Skill, type AgentStatus, type ContainerStatus } from '../lib/api'
import styles from './Dashboard.module.css'

// ── Agent Config Panel ────────────────────────────────────────────

interface ConfigPanelProps {
  agent: Agent
  connectors: LlmConnector[]
  knowledgeBases: KnowledgeBase[]
  skills: Skill[]
  onSave: (updated: Agent) => void
  onClose: () => void
}

function ConfigPanel({ agent, connectors, knowledgeBases, skills, onSave, onClose }: ConfigPanelProps) {
  const [name, setName] = useState(agent.name)
  const [description, setDescription] = useState(agent.description)
  const [systemPrompt, setSystemPrompt] = useState(agent.system_prompt ?? '')
  const [temperature, setTemperature] = useState(
    agent.temperature != null ? String(agent.temperature) : ''
  )
  const [maxTokens, setMaxTokens] = useState(
    agent.max_tokens != null ? String(agent.max_tokens) : ''
  )
  const [connectorId, setConnectorId] = useState(agent.llm_connector_id ?? '')
  const [containerEnabled, setContainerEnabled] = useState(agent.container_enabled)
  const [cpuLimit, setCpuLimit] = useState(
    agent.container_config?.cpu_limit != null ? String(agent.container_config.cpu_limit) : ''
  )
  const [memoryLimit, setMemoryLimit] = useState(
    agent.container_config?.memory_limit_mb != null ? String(agent.container_config.memory_limit_mb) : ''
  )
  const [networkEnabled, setNetworkEnabled] = useState(agent.container_config?.network_enabled ?? false)
  const [linkedKbIds, setLinkedKbIds] = useState<Set<string>>(new Set())
  const [kbLoading, setKbLoading] = useState(true)
  const [linkedSkillIds, setLinkedSkillIds] = useState<Set<string>>(new Set())
  const [skillLoading, setSkillLoading] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')

  // Fetch which KBs and skills are currently linked to this agent
  useEffect(() => {
    let cancelled = false
    const fetchLinked = async () => {
      setKbLoading(true)
      setSkillLoading(true)
      try {
        const [kbLinked, skillLinked] = await Promise.all([
          (async () => {
            const linked = new Set<string>()
            await Promise.all(
              knowledgeBases.map(async (kb) => {
                const agentIds = await api.knowledge.listAgents(kb.id)
                if (agentIds.includes(agent.id)) linked.add(kb.id)
              })
            )
            return linked
          })(),
          api.agentSkills.list(agent.id).then(ids => new Set(ids)),
        ])
        if (!cancelled) {
          setLinkedKbIds(kbLinked)
          setLinkedSkillIds(skillLinked)
        }
      } finally {
        if (!cancelled) {
          setKbLoading(false)
          setSkillLoading(false)
        }
      }
    }
    fetchLinked()
    return () => { cancelled = true }
  }, [agent.id, knowledgeBases, skills])

  const handleKbToggle = async (kbId: string) => {
    const isLinked = linkedKbIds.has(kbId)
    try {
      if (isLinked) {
        await api.knowledge.unlinkAgent(kbId, agent.id)
        setLinkedKbIds(prev => { const next = new Set(prev); next.delete(kbId); return next })
      } else {
        await api.knowledge.linkAgent(kbId, agent.id)
        setLinkedKbIds(prev => new Set(prev).add(kbId))
      }
    } catch (err) {
      console.error('Failed to toggle KB access:', err)
    }
  }

  const handleSkillToggle = async (skillId: string) => {
    const isLinked = linkedSkillIds.has(skillId)
    try {
      if (isLinked) {
        await api.agentSkills.unlink(agent.id, skillId)
        setLinkedSkillIds(prev => { const next = new Set(prev); next.delete(skillId); return next })
      } else {
        await api.agentSkills.link(agent.id, skillId)
        setLinkedSkillIds(prev => new Set(prev).add(skillId))
      }
    } catch (err) {
      console.error('Failed to toggle skill:', err)
    }
  }

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) { setError('Name is required.'); return }
    setError('')
    setSubmitting(true)
    try {
      const updated = await api.agents.patch(agent.id, {
        name: name.trim(),
        description: description.trim(),
        system_prompt: systemPrompt || undefined,
        temperature: temperature ? parseFloat(temperature) : undefined,
        max_tokens: maxTokens ? parseInt(maxTokens) : undefined,
        llm_connector_id: connectorId || undefined,
        container_enabled: containerEnabled,
        container_config: containerEnabled ? {
          cpu_limit: cpuLimit ? parseFloat(cpuLimit) : null,
          memory_limit_mb: memoryLimit ? parseInt(memoryLimit) : null,
          network_enabled: networkEnabled,
        } : undefined,
      })
      onSave(updated)
    } catch (err) {
      setError(String(err))
    } finally {
      setSubmitting(false)
    }
  }

  const tempNum = parseFloat(temperature)
  const tempValid = !temperature || (!isNaN(tempNum) && tempNum >= 0 && tempNum <= 2)

  return (
    <div className={styles.configPanelOverlay} onClick={onClose}>
      <div className={styles.configPanel} onClick={e => e.stopPropagation()}>
        <div className={styles.panelHeader}>
          <div className={styles.panelHeaderLeft}>
            <div className={styles.panelAvatar}><Bot size={18} /></div>
            <div>
              <h3 className={styles.panelTitle}>Configure Agent</h3>
              <p className={styles.panelSub}>{agent.name}</p>
            </div>
          </div>
          <button className={styles.panelClose} onClick={onClose}>&#x2715;</button>
        </div>

        <form onSubmit={handleSave} className={styles.panelBody}>
          <div className={styles.fieldSection}>
            <h4 className={styles.fieldSectionTitle}>Identity</h4>
            <div className={styles.formGroup}>
              <label className={styles.label}>Name</label>
              <input className={styles.input} value={name} onChange={e => setName(e.target.value)} />
            </div>
            <div className={styles.formGroup}>
              <label className={styles.label}>Description</label>
              <input
                className={styles.input}
                value={description}
                onChange={e => setDescription(e.target.value)}
                placeholder="What does this agent do?"
              />
            </div>
          </div>

          <div className={styles.fieldSection}>
            <h4 className={styles.fieldSectionTitle}>System Prompt</h4>
            <div className={styles.formGroup}>
              <textarea
                className={`${styles.input} ${styles.textarea}`}
                value={systemPrompt}
                onChange={e => setSystemPrompt(e.target.value)}
                placeholder="You are a helpful assistant..."
                rows={5}
              />
              <p className={styles.fieldHint}>
                Prepended to every conversation. Leave blank for no system context.
              </p>
            </div>
          </div>

          <div className={styles.fieldSection}>
            <h4 className={styles.fieldSectionTitle}>Inference</h4>

            <div className={styles.formGroup}>
              <label className={styles.label}>
                <Cpu size={11} /> LLM Connector
              </label>
              <div className={styles.selectWrap}>
                <select
                  className={styles.select}
                  value={connectorId}
                  onChange={e => setConnectorId(e.target.value)}
                >
                  <option value="">Use default connector</option>
                  {connectors.map(c => (
                    <option key={c.id} value={c.id}>{c.name} ({c.model})</option>
                  ))}
                </select>
                <ChevronDown size={13} className={styles.selectChevron} />
              </div>
            </div>

            <div className={styles.formRow}>
              <div className={styles.formGroup}>
                <label className={styles.label}>
                  <Thermometer size={11} /> Temperature
                </label>
                <input
                  className={`${styles.input} ${!tempValid ? styles.inputError : ''}`}
                  value={temperature}
                  onChange={e => setTemperature(e.target.value)}
                  placeholder="0.7"
                  type="number"
                  min="0"
                  max="2"
                  step="0.1"
                />
              </div>
              <div className={styles.formGroup}>
                <label className={styles.label}>
                  <Hash size={11} /> Max Tokens
                </label>
                <input
                  className={styles.input}
                  value={maxTokens}
                  onChange={e => setMaxTokens(e.target.value)}
                  placeholder="2048"
                  type="number"
                  min="1"
                />
              </div>
            </div>
          </div>

          <div className={styles.fieldSection}>
            <h4 className={styles.fieldSectionTitle}>Sandbox</h4>
            <div className={styles.formGroup}>
              <label className={styles.toggleLabel}>
                <input
                  type="checkbox"
                  checked={containerEnabled}
                  onChange={e => setContainerEnabled(e.target.checked)}
                  className={styles.checkbox}
                />
                <Container size={11} /> Enable code execution sandbox
              </label>
              <p className={styles.fieldHint}>
                Gives the agent a Docker container for running Python and Bash code.
              </p>
            </div>

            {containerEnabled && (
              <>
                <div className={styles.formRow}>
                  <div className={styles.formGroup}>
                    <label className={styles.label}>
                      <Cpu size={11} /> CPU Limit (cores)
                    </label>
                    <input
                      className={styles.input}
                      value={cpuLimit}
                      onChange={e => setCpuLimit(e.target.value)}
                      placeholder="1.0"
                      type="number"
                      min="0.1"
                      max="4"
                      step="0.1"
                    />
                  </div>
                  <div className={styles.formGroup}>
                    <label className={styles.label}>
                      <Hash size={11} /> Memory (MB)
                    </label>
                    <input
                      className={styles.input}
                      value={memoryLimit}
                      onChange={e => setMemoryLimit(e.target.value)}
                      placeholder="512"
                      type="number"
                      min="64"
                      max="4096"
                      step="64"
                    />
                  </div>
                </div>
                <div className={styles.formGroup}>
                  <label className={styles.toggleLabel}>
                    <input
                      type="checkbox"
                      checked={networkEnabled}
                      onChange={e => setNetworkEnabled(e.target.checked)}
                      className={styles.checkbox}
                    />
                    Enable network access
                  </label>
                  <p className={styles.fieldHint}>
                    Disabled by default for security. Enable if the agent needs internet access.
                  </p>
                </div>
              </>
            )}
          </div>

          <div className={styles.fieldSection}>
            <h4 className={styles.fieldSectionTitle}>Knowledge Bases</h4>
            {knowledgeBases.length === 0 ? (
              <p className={styles.fieldHint}>No knowledge bases available. Create one in the Knowledge Base page.</p>
            ) : kbLoading ? (
              <div className={styles.kbLoading}>
                <Loader2 size={13} className="spinning" />
                <span>Loading...</span>
              </div>
            ) : (
              <div className={styles.kbList}>
                {knowledgeBases.map(kb => (
                  <label key={kb.id} className={styles.kbItem}>
                    <input
                      type="checkbox"
                      className={styles.checkbox}
                      checked={linkedKbIds.has(kb.id)}
                      onChange={() => handleKbToggle(kb.id)}
                    />
                    <div className={styles.kbItemInfo}>
                      <span className={styles.kbItemName}>
                        <BookOpen size={11} /> {kb.name}
                      </span>
                      <span className={styles.kbItemMeta}>
                        {kb.entry_count} {kb.entry_count === 1 ? 'entry' : 'entries'}
                      </span>
                    </div>
                  </label>
                ))}
              </div>
            )}
            <p className={styles.fieldHint}>
              Linked knowledge bases are searched during conversations for relevant context.
            </p>
          </div>

          <div className={styles.fieldSection}>
            <h4 className={styles.fieldSectionTitle}>Skills</h4>
            {skills.length === 0 ? (
              <p className={styles.fieldHint}>No skills available. Create skills to give agents reusable prompt modules invokable with /skill-name syntax.</p>
            ) : skillLoading ? (
              <div className={styles.kbLoading}>
                <Loader2 size={13} className="spinning" />
                <span>Loading...</span>
              </div>
            ) : (
              <div className={styles.kbList}>
                {skills.map(skill => (
                  <label key={skill.id} className={styles.kbItem}>
                    <input
                      type="checkbox"
                      className={styles.checkbox}
                      checked={linkedSkillIds.has(skill.id)}
                      onChange={() => handleSkillToggle(skill.id)}
                    />
                    <div className={styles.kbItemInfo}>
                      <span className={styles.kbItemName}>
                        <Zap size={11} /> /{skill.name}
                      </span>
                      <span className={styles.kbItemMeta}>
                        {skill.description}
                      </span>
                    </div>
                  </label>
                ))}
              </div>
            )}
            <p className={styles.fieldHint}>
              Linked skills can be activated by users with /skill-name in chat messages.
            </p>
          </div>

          {error && <p className={styles.errorMsg}>{error}</p>}

          <div className={styles.panelActions}>
            <Button variant="secondary" size="sm" type="button" onClick={onClose}>Cancel</Button>
            <Button variant="primary" size="sm" type="submit" disabled={submitting}>
              {submitting && <Loader2 size={13} className="spinning" />}
              <Check size={13} /> Save Changes
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── Create Agent Form ─────────────────────────────────────────────

interface CreateFormProps {
  onSave: (agent: Agent) => void
  onCancel: () => void
}

function CreateForm({ onSave, onCancel }: CreateFormProps) {
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!name.trim()) return
    setSubmitting(true)
    try {
      const agent = await api.agents.create({ name: name.trim(), description: description.trim() })
      onSave(agent)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className={styles.createForm}>
      <form onSubmit={handleSubmit}>
        <div className={styles.createFormRow}>
          <input
            className={styles.input}
            value={name}
            onChange={e => setName(e.target.value)}
            placeholder="Agent name (e.g. Research Assistant)"
            autoFocus
          />
          <input
            className={styles.input}
            value={description}
            onChange={e => setDescription(e.target.value)}
            placeholder="What does this agent do?"
          />
          <Button variant="primary" size="sm" type="submit" disabled={submitting || !name.trim()}>
            {submitting ? <Loader2 size={13} className="spinning" /> : <Plus size={13} />}
            Create
          </Button>
          <Button variant="secondary" size="sm" type="button" onClick={onCancel}>Cancel</Button>
        </div>
      </form>
    </div>
  )
}

// ── Agent Card ────────────────────────────────────────────────────

interface AgentCardProps {
  agent: Agent
  connector?: LlmConnector
  containerStatus?: ContainerStatus
  onConfigure: () => void
  onDelete: () => void
  onStatusChange: (status: AgentStatus) => void
  onContainerToggle: () => void
}

function AgentCard({ agent, connector, containerStatus, onConfigure, onDelete, onStatusChange, onContainerToggle }: AgentCardProps) {
  const containerRunning = containerStatus?.state === 'running'

  return (
    <div className={styles.agentCard}>
      <div className={styles.agentCardTop}>
        <div className={styles.agentCardLeft}>
          <div className={styles.agentAvatarWrap}>
            <div className={styles.agentAvatar}><Bot size={20} strokeWidth={1.5} /></div>
            <div className={`${styles.statusDot} ${styles[`status_${agent.status}`]}`} />
          </div>
          <div>
            <h3 className={styles.agentCardName}>{agent.name}</h3>
            <p className={styles.agentCardDesc}>{agent.description || 'No description'}</p>
          </div>
        </div>
        <StatusBadge status={agent.status} />
      </div>

      <div className={styles.agentConfig}>
        {connector ? (
          <span className={styles.configTag}>
            <Cpu size={11} /> {connector.name}
          </span>
        ) : (
          <span className={`${styles.configTag} ${styles.configTagMuted}`}>
            <Cpu size={11} /> Default connector
          </span>
        )}
        {agent.temperature != null && (
          <span className={styles.configTag}>
            <Thermometer size={11} /> {agent.temperature}
          </span>
        )}
        {agent.max_tokens != null && (
          <span className={styles.configTag}>
            <Hash size={11} /> {agent.max_tokens}
          </span>
        )}
        {agent.container_enabled && (
          <span className={`${styles.configTag} ${containerRunning ? styles.configTagActive : ''}`}>
            <Container size={11} /> {containerRunning ? 'Container running' : 'Sandbox'}
          </span>
        )}
      </div>

      <div className={styles.agentCardActions}>
        <div className={styles.statusToggle}>
          {(['online', 'offline'] as AgentStatus[]).map(s => (
            <button
              key={s}
              className={`${styles.statusBtn} ${agent.status === s ? styles.statusBtnActive : ''}`}
              onClick={() => onStatusChange(s)}
            >
              {s}
            </button>
          ))}
        </div>
        <div className={styles.agentCardBtns}>
          {agent.container_enabled && (
            <button
              className={`${styles.containerBtn} ${containerRunning ? styles.containerBtnActive : ''}`}
              onClick={onContainerToggle}
              title={containerRunning ? 'Stop container' : 'Start container'}
            >
              {containerRunning ? <Square size={12} /> : <Play size={12} />}
            </button>
          )}
          <button className={styles.configureBtn} onClick={onConfigure} title="Configure">
            <Settings2 size={14} /> Configure
          </button>
          <button className={styles.deleteAgentBtn} onClick={onDelete} title="Delete">
            <Trash2 size={14} />
          </button>
        </div>
      </div>
    </div>
  )
}

// ── Page ──────────────────────────────────────────────────────────

export function DashboardPage() {
  const [agents, setAgents] = useState<Agent[]>([])
  const [conversations, setConversations] = useState<Conversation[]>([])
  const [connectors, setConnectors] = useState<LlmConnector[]>([])
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBase[]>([])
  const [skills, setSkills] = useState<Skill[]>([])
  const [loading, setLoading] = useState(true)
  const [showCreate, setShowCreate] = useState(false)
  const [configuring, setConfiguring] = useState<Agent | null>(null)
  const [containerStatuses, setContainerStatuses] = useState<Record<string, ContainerStatus>>({})
  const navigate = useNavigate()

  const fetchContainerStatuses = async (agentList: Agent[]) => {
    const enabled = agentList.filter(a => a.container_enabled)
    const statuses: Record<string, ContainerStatus> = {}
    await Promise.allSettled(
      enabled.map(async (a) => {
        try {
          const status = await api.containers.status(a.id)
          statuses[a.id] = status
        } catch { /* no container running */ }
      })
    )
    setContainerStatuses(statuses)
  }

  useEffect(() => {
    Promise.all([api.agents.list(), api.conversations.list(), api.llmConnectors.list(), api.knowledge.listBases(), api.skills.list()])
      .then(([agts, convos, conns, kbs, sks]) => {
        setAgents(agts)
        setConversations(convos)
        setConnectors(conns)
        setKnowledgeBases(kbs)
        setSkills(sks)
        fetchContainerStatuses(agts)
      })
      .finally(() => setLoading(false))
  }, [])

  const onlineCount = agents.filter(a => a.status === 'online').length
  const busyCount = agents.filter(a => a.status === 'busy').length

  const handleCreate = (agent: Agent) => {
    setAgents(prev => [agent, ...prev])
    setShowCreate(false)
  }

  const handleUpdate = (updated: Agent) => {
    setAgents(prev => prev.map(a => a.id === updated.id ? updated : a))
    setConfiguring(null)
  }

  const handleDelete = async (id: string) => {
    await api.agents.delete(id)
    setAgents(prev => prev.filter(a => a.id !== id))
  }

  const handleStatusChange = async (id: string, status: AgentStatus) => {
    const updated = await api.agents.patch(id, { status })
    setAgents(prev => prev.map(a => a.id === updated.id ? updated : a))
  }

  const handleContainerToggle = async (agentId: string) => {
    const current = containerStatuses[agentId]
    try {
      if (current?.state === 'running') {
        await api.containers.stop(agentId)
        setContainerStatuses(prev => {
          const next = { ...prev }
          delete next[agentId]
          return next
        })
      } else {
        const status = await api.containers.start(agentId)
        setContainerStatuses(prev => ({ ...prev, [agentId]: status }))
      }
    } catch (err) {
      console.error('Container toggle failed:', err)
    }
  }

  const stats = [
    { label: 'Total Agents', value: String(agents.length), icon: Bot, sub: `${onlineCount} online`, color: 'var(--accent)' },
    { label: 'Conversations', value: String(conversations.length), icon: MessageCircle, sub: 'All time', color: 'var(--info)' },
    { label: 'Active Now', value: String(onlineCount + busyCount), icon: TrendingUp, sub: `${onlineCount} online, ${busyCount} busy`, color: 'var(--success)' },
    { label: 'Connectors', value: String(connectors.length), icon: Plug, sub: 'LLM providers', color: 'var(--warning)' },
  ]

  return (
    <div className="fade-in">
      <PageHeader
        title="Dashboard"
        description="Agent command center. Manage your agents, monitor status, and track activity."
        actions={
          !showCreate ? (
            <Button onClick={() => setShowCreate(true)}>
              <Plus size={15} /> New Agent
            </Button>
          ) : undefined
        }
      />

      {/* Stats */}
      <div className={`${styles.statsGrid} stagger`}>
        {stats.map(({ label, value, icon: Icon, sub, color }) => (
          <div key={label} className={styles.statCard}>
            <div className={styles.statIcon} style={{ color, background: `color-mix(in srgb, ${color} 12%, transparent)` }}>
              <Icon size={18} strokeWidth={1.5} />
            </div>
            <div>
              <div className={styles.statValue}>{loading ? '\u2014' : value}</div>
              <div className={styles.statLabel}>{label}</div>
              <div className={styles.statSub}>{sub}</div>
            </div>
          </div>
        ))}
      </div>

      {/* Create agent form */}
      {showCreate && (
        <CreateForm onSave={handleCreate} onCancel={() => setShowCreate(false)} />
      )}

      {/* Agents section */}
      <div className={styles.sectionHeader}>
        <h2 className={styles.sectionTitle}>
          <Bot size={16} strokeWidth={1.5} /> Agents
          <span className={styles.sectionCount}>{agents.length}</span>
        </h2>
        {!showCreate && (
          <Button size="sm" variant="ghost" onClick={() => setShowCreate(true)}>
            <Plus size={13} /> Add
          </Button>
        )}
      </div>

      {loading ? (
        <Card>
          <div className={styles.loadingState}>
            <Loader2 size={18} className="spinning" />
            <span>Loading agents...</span>
          </div>
        </Card>
      ) : agents.length === 0 && !showCreate ? (
        <div className={styles.emptyAgents}>
          <div className={styles.emptyAgentsIcon}>
            <Bot size={36} strokeWidth={1} />
          </div>
          <p className={styles.emptyAgentsTitle}>No agents yet</p>
          <p className={styles.emptyAgentsDesc}>Create your first agent to get started.</p>
          <Button variant="primary" size="sm" onClick={() => setShowCreate(true)}>
            <Plus size={13} /> Create Agent
          </Button>
        </div>
      ) : (
        <div className={styles.agentGrid}>
          {agents.map(agent => (
            <AgentCard
              key={agent.id}
              agent={agent}
              connector={connectors.find(c => c.id === agent.llm_connector_id)}
              containerStatus={containerStatuses[agent.id]}
              onConfigure={() => setConfiguring(agent)}
              onDelete={() => handleDelete(agent.id)}
              onStatusChange={status => handleStatusChange(agent.id, status)}
              onContainerToggle={() => handleContainerToggle(agent.id)}
            />
          ))}
        </div>
      )}

      {/* Recent conversations */}
      <div className={styles.sectionHeader} style={{ marginTop: 32 }}>
        <h2 className={styles.sectionTitle}>
          <Activity size={16} strokeWidth={1.5} /> Recent Conversations
        </h2>
        <Button size="sm" variant="ghost" onClick={() => navigate('/conversations')}>
          View all
        </Button>
      </div>

      {conversations.length === 0 && !loading ? (
        <p className={styles.emptyMsg}>
          No conversations yet.{' '}
          <button className={styles.emptyLink} onClick={() => navigate('/conversations')}>Start one &#x2192;</button>
        </p>
      ) : (
        <div className={styles.activityList}>
          {conversations.slice(0, 5).map(convo => (
            <div
              key={convo.id}
              className={styles.activityRow}
              onClick={() => navigate('/conversations')}
            >
              <div className={styles.activityIcon}>
                <MessageCircle size={14} strokeWidth={1.5} />
              </div>
              <div className={styles.activityContent}>
                <span className={styles.activityText}>{convo.title}</span>
                <span className={styles.activityTime}>
                  <Clock size={10} />
                  {relativeTime(convo.updated_at)}
                </span>
              </div>
              <Zap size={12} className={styles.activityArrow} />
            </div>
          ))}
        </div>
      )}

      {configuring && (
        <ConfigPanel
          agent={configuring}
          connectors={connectors}
          knowledgeBases={knowledgeBases}
          skills={skills}
          onSave={handleUpdate}
          onClose={() => setConfiguring(null)}
        />
      )}
    </div>
  )
}

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const m = Math.floor(diff / 60000)
  if (m < 1) return 'just now'
  if (m < 60) return `${m}m ago`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h ago`
  return `${Math.floor(h / 24)}d ago`
}
