import { useState, useEffect } from 'react'
import {
  Activity,
  Bot,
  MessageCircle,
  Plug,
  Plus,
  Settings2,
  Trash2,
  Check,
  ChevronDown,
  ChevronRight,
  Loader2,
  Cpu,
  Thermometer,
  Hash,
  Zap,
  Container,
  BookOpen,
  Search,
  Star,
  Clock,
} from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { useAuth } from '../lib/auth'
import { StatusBadge } from '../components/StatusBadge'
import { Button } from '../components/Button'
import { api, type Agent, type Conversation, type LlmConnector, type KnowledgeBase, type Skill, type AgentStatus } from '../lib/api'
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
                  type="number" min="0" max="2" step="0.1"
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
                  type="number" min="1"
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
                    <label className={styles.label}><Cpu size={11} /> CPU Limit (cores)</label>
                    <input
                      className={styles.input} value={cpuLimit}
                      onChange={e => setCpuLimit(e.target.value)}
                      placeholder="1.0" type="number" min="0.1" max="4" step="0.1"
                    />
                  </div>
                  <div className={styles.formGroup}>
                    <label className={styles.label}><Hash size={11} /> Memory (MB)</label>
                    <input
                      className={styles.input} value={memoryLimit}
                      onChange={e => setMemoryLimit(e.target.value)}
                      placeholder="512" type="number" min="64" max="4096" step="64"
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
              <p className={styles.fieldHint}>No knowledge bases available.</p>
            ) : kbLoading ? (
              <div className={styles.kbLoading}><Loader2 size={13} className="spinning" /><span>Loading...</span></div>
            ) : (
              <div className={styles.kbList}>
                {knowledgeBases.map(kb => (
                  <label key={kb.id} className={styles.kbItem}>
                    <input type="checkbox" className={styles.checkbox} checked={linkedKbIds.has(kb.id)} onChange={() => handleKbToggle(kb.id)} />
                    <div className={styles.kbItemInfo}>
                      <span className={styles.kbItemName}><BookOpen size={11} /> {kb.name}</span>
                      <span className={styles.kbItemMeta}>{kb.entry_count} {kb.entry_count === 1 ? 'entry' : 'entries'}</span>
                    </div>
                  </label>
                ))}
              </div>
            )}
            <p className={styles.fieldHint}>Linked knowledge bases are searched during conversations.</p>
          </div>

          <div className={styles.fieldSection}>
            <h4 className={styles.fieldSectionTitle}>Skills</h4>
            {skills.length === 0 ? (
              <p className={styles.fieldHint}>No skills available.</p>
            ) : skillLoading ? (
              <div className={styles.kbLoading}><Loader2 size={13} className="spinning" /><span>Loading...</span></div>
            ) : (
              <div className={styles.kbList}>
                {skills.map(skill => (
                  <label key={skill.id} className={styles.kbItem}>
                    <input type="checkbox" className={styles.checkbox} checked={linkedSkillIds.has(skill.id)} onChange={() => handleSkillToggle(skill.id)} />
                    <div className={styles.kbItemInfo}>
                      <span className={styles.kbItemName}><Zap size={11} /> /{skill.name}</span>
                      <span className={styles.kbItemMeta}>{skill.description}</span>
                    </div>
                  </label>
                ))}
              </div>
            )}
            <p className={styles.fieldHint}>Linked skills can be activated with /skill-name in chat.</p>
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
          <input className={styles.input} value={name} onChange={e => setName(e.target.value)} placeholder="Agent name" autoFocus />
          <input className={styles.input} value={description} onChange={e => setDescription(e.target.value)} placeholder="Description" />
          <Button variant="primary" size="sm" type="submit" disabled={submitting || !name.trim()}>
            {submitting ? <Loader2 size={13} className="spinning" /> : <Plus size={13} />} Create
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
  onConfigure: () => void
  onDelete: () => void
  onStatusChange: (status: AgentStatus) => void
}

function AgentCard({ agent, connector, onConfigure, onDelete, onStatusChange }: AgentCardProps) {
  return (
    <div className={styles.agentCard}>
      <div className={styles.agentCardTop}>
        <div className={styles.agentCardLeft}>
          <div className={styles.agentAvatarWrap}>
            <div className={styles.agentAvatar}><Bot size={18} strokeWidth={1.5} /></div>
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
          <span className={styles.configTag}><Cpu size={10} /> {connector.name}</span>
        ) : (
          <span className={`${styles.configTag} ${styles.configTagMuted}`}><Cpu size={10} /> Default</span>
        )}
        {agent.temperature != null && <span className={styles.configTag}><Thermometer size={10} /> {agent.temperature}</span>}
        {agent.max_tokens != null && <span className={styles.configTag}><Hash size={10} /> {agent.max_tokens}</span>}
        {agent.container_enabled && <span className={styles.configTag}><Container size={10} /> Sandbox</span>}
      </div>

      <div className={styles.agentCardActions}>
        <div className={styles.statusToggle}>
          {(['online', 'offline'] as AgentStatus[]).map(s => (
            <button key={s} className={`${styles.statusBtn} ${agent.status === s ? styles.statusBtnActive : ''}`} onClick={() => onStatusChange(s)}>
              {s}
            </button>
          ))}
        </div>
        <div className={styles.agentCardBtns}>
          <button className={styles.configureBtn} onClick={onConfigure}><Settings2 size={13} /> Config</button>
          <button className={styles.deleteAgentBtn} onClick={onDelete}><Trash2 size={13} /></button>
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
  const navigate = useNavigate()
  const { user } = useAuth()

  useEffect(() => {
    Promise.all([api.agents.list(), api.conversations.list(), api.llmConnectors.list(), api.knowledge.listBases(), api.skills.list()])
      .then(([agts, convos, conns, kbs, sks]) => {
        setAgents(agts)
        setConversations(convos)
        setConnectors(conns)
        setKnowledgeBases(kbs)
        setSkills(sks)
      })
      .finally(() => setLoading(false))
  }, [])

  const onlineCount = agents.filter(a => a.status === 'online').length
  const busyCount = agents.filter(a => a.status === 'busy').length
  const activePercent = agents.length > 0 ? Math.round((onlineCount + busyCount) / agents.length * 100) : 0
  const totalKbEntries = knowledgeBases.reduce((sum, kb) => sum + kb.entry_count, 0)

  const handleCreate = (agent: Agent) => { setAgents(prev => [agent, ...prev]); setShowCreate(false) }
  const handleUpdate = (updated: Agent) => { setAgents(prev => prev.map(a => a.id === updated.id ? updated : a)); setConfiguring(null) }
  const handleDelete = async (id: string) => { await api.agents.delete(id); setAgents(prev => prev.filter(a => a.id !== id)) }
  const handleStatusChange = async (id: string, status: AgentStatus) => {
    const updated = await api.agents.patch(id, { status })
    setAgents(prev => prev.map(a => a.id === updated.id ? updated : a))
  }

  // Top agents sorted by conversation count
  const agentActivity = agents.map(a => ({
    ...a,
    convos: conversations.filter(c => c.agent_id === a.id).length,
  })).sort((a, b) => b.convos - a.convos)

  const agentScore = (a: typeof agentActivity[0]) => {
    const bonus = a.status === 'online' ? 1 : a.status === 'busy' ? 0.5 : 0
    return Math.min(5, 3 + a.convos * 0.3 + bonus).toFixed(1)
  }

  // Decorative trend bars
  const trendBars = Array.from({ length: 28 }, (_, i) => ({
    height: Math.max(12, Math.min(95, 30 + ((i * 7 + 5) % 13) * 5 + Math.sin(i * 0.5) * 18)),
    accent: i % 6 === 0,
  }))

  const statusColor = (s: string) =>
    s === 'online' ? 'var(--success)' : s === 'busy' ? 'var(--warning)' : s === 'error' ? 'var(--error)' : 'var(--text-tertiary)'

  return (
    <div className="fade-in">
      {/* Header */}
      <div className={styles.header}>
        <h1 className={styles.title}>// Overview</h1>
        <div className={styles.headerRight}>
          <div className={styles.searchBox}>
            <Search size={14} />
            <input placeholder="Search..." className={styles.searchInput} />
          </div>
          {user && (
            <div className={styles.headerAvatar}>
              {user.display_name.charAt(0).toUpperCase()}
            </div>
          )}
        </div>
      </div>

      {/* ── Bento Grid ── */}
      <div className={styles.bento}>

        {/* Hero: 2×2 — Conversations overview */}
        <div className={`${styles.cell} ${styles.hero}`}>
          <span className={styles.cellLabel}>Conversations</span>
          <div className={styles.heroTop}>
            <div>
              <div className={styles.heroStat}>
                {loading ? '\u2014' : String(conversations.length).padStart(2, '0')}
              </div>
              <div className={styles.heroStatSub}>Total conversations across all agents</div>
            </div>
            <div className={styles.activeBadge}>
              <span className={styles.activeDot} />
              {activePercent}% Active
            </div>
          </div>
          <div className={styles.activityWrap}>
            <div className={styles.activityMeta}>
              <span className={styles.activityMetaLabel}>Agent Availability</span>
              <span className={styles.activityMetaValue}>
                {onlineCount + busyCount}/{agents.length}
              </span>
            </div>
            <div className={styles.activityBar}>
              <div
                className={styles.activityBarFill}
                style={{ width: `${Math.max(activePercent, 8)}%` }}
              >
                <span className={styles.activityBarText}>Active</span>
              </div>
            </div>
          </div>
        </div>

        {/* Top Agents: 1×2 */}
        <div className={`${styles.cell} ${styles.topAgents}`}>
          <span className={styles.cellLabel}>Top Agents</span>
          {loading ? (
            <div className={styles.topAgentsEmpty}><Loader2 size={16} className="spinning" /></div>
          ) : agents.length === 0 ? (
            <div className={styles.topAgentsEmpty}>
              <Bot size={20} strokeWidth={1} />
              <span>No agents yet</span>
            </div>
          ) : (
            <div className={styles.topAgentsList}>
              {agentActivity.slice(0, 5).map(agent => (
                <div key={agent.id} className={styles.topAgentRow} onClick={() => setConfiguring(agent)}>
                  <div className={styles.topAgentAvatar}>
                    <Bot size={14} strokeWidth={1.5} />
                    <div className={styles.topAgentDot} style={{ background: statusColor(agent.status) }} />
                  </div>
                  <div className={styles.topAgentInfo}>
                    <span className={styles.topAgentName}>{agent.name}</span>
                    <span className={styles.topAgentDesc}>{agent.description || 'No description'}</span>
                  </div>
                  <div className={styles.topAgentScore}>
                    <Star size={11} fill="currentColor" /> {agentScore(agent)}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Metric: Total Agents */}
        <div className={`${styles.cell} ${styles.metric}`}>
          <span className={styles.cellLabel}>Agents</span>
          <div className={styles.metricValue}>{loading ? '\u2014' : String(agents.length).padStart(3, '0')}</div>
          <div className={styles.metricSub}>{onlineCount} online, {busyCount} busy</div>
          <div className={styles.metricAccent} style={{ background: 'var(--accent)' }} />
        </div>

        {/* Metric: Active Now */}
        <div className={`${styles.cell} ${styles.metric}`}>
          <span className={styles.cellLabel}>Active Now</span>
          <div className={styles.metricValue}>{loading ? '\u2014' : String(onlineCount + busyCount).padStart(3, '0')}</div>
          <div className={styles.metricSub}>{activePercent}% of fleet</div>
          <div className={styles.metricAccent} style={{ background: 'var(--success)' }} />
        </div>

        {/* Trends: 2×1 */}
        <div className={`${styles.cell} ${styles.trends}`}>
          <div className={styles.trendsHeader}>
            <span className={styles.cellLabel} style={{ marginBottom: 0 }}>Trends Over Time</span>
            <div className={styles.trendsLegend}>
              <span><span className={styles.legendDot} style={{ background: '#34d399' }} />msgs</span>
              <span><span className={styles.legendDot} style={{ background: '#fbbf24' }} />tools</span>
            </div>
          </div>
          <div className={styles.barsChart}>
            {trendBars.map((bar, i) => (
              <div
                key={i}
                className={styles.bar}
                style={{
                  height: `${bar.height}%`,
                  background: bar.accent ? '#fbbf24' : '#34d399',
                  animationDelay: `${i * 25}ms`,
                }}
              />
            ))}
          </div>
        </div>

        {/* Metric: Knowledge */}
        <div className={`${styles.cell} ${styles.metric}`}>
          <span className={styles.cellLabel}>Knowledge</span>
          <div className={styles.metricValue}>{loading ? '\u2014' : String(totalKbEntries).padStart(3, '0')}</div>
          <div className={styles.metricSub}>{knowledgeBases.length} bases</div>
          <div className={styles.metricAccent} style={{ background: 'var(--info)' }} />
        </div>

        {/* Metric: Connectors */}
        <div className={`${styles.cell} ${styles.metric}`}>
          <span className={styles.cellLabel}>LLM Connectors</span>
          <div className={styles.metricValue}>{loading ? '\u2014' : String(connectors.length).padStart(3, '0')}</div>
          <div className={styles.metricSub}>Configured</div>
          <div className={styles.metricAccent} style={{ background: 'var(--warning)' }} />
        </div>

        {/* CTA: New Agent */}
        <div className={`${styles.cell} ${styles.cta}`} onClick={() => setShowCreate(true)}>
          <div className={styles.ctaIcon}><Plus size={22} strokeWidth={2} /></div>
          <span className={styles.ctaText}>New Agent</span>
        </div>

        {/* Recent Conversations: 3×1 */}
        <div className={`${styles.cell} ${styles.convos}`}>
          <span className={styles.cellLabel}>Recent</span>
          {conversations.length === 0 && !loading ? (
            <div className={styles.convoEmpty}>No conversations yet</div>
          ) : (
            <div className={styles.convoList}>
              {conversations.slice(0, 4).map(convo => (
                <div key={convo.id} className={styles.convoRow} onClick={() => navigate(`/conversations/${convo.id}`)}>
                  <div className={styles.convoIcon}><MessageCircle size={13} /></div>
                  <span className={styles.convoText}>{convo.title}</span>
                  <span className={styles.convoTime}>{relativeTime(convo.updated_at)}</span>
                </div>
              ))}
            </div>
          )}
          <button className={styles.viewAll} onClick={() => navigate('/conversations')}>
            View all <ChevronRight size={12} />
          </button>
        </div>
      </div>

      {/* ── Agent Management ── */}
      <div className={styles.manageHeader}>
        <div className={styles.manageTitle}>
          <Bot size={15} strokeWidth={1.5} />
          Manage Agents
          <span className={styles.manageTitleCount}>{agents.length}</span>
        </div>
        {!showCreate && (
          <button className={styles.ghostBtn} onClick={() => setShowCreate(true)}>
            <Plus size={12} /> Add
          </button>
        )}
      </div>

      {showCreate && <CreateForm onSave={handleCreate} onCancel={() => setShowCreate(false)} />}

      {loading ? (
        <div className={styles.loadingState}>
          <Loader2 size={16} className="spinning" /> Loading agents...
        </div>
      ) : agents.length === 0 && !showCreate ? (
        <div className={styles.emptyAgents}>
          <div className={styles.emptyAgentsIcon}><Bot size={32} strokeWidth={1} /></div>
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
              onConfigure={() => setConfiguring(agent)}
              onDelete={() => handleDelete(agent.id)}
              onStatusChange={status => handleStatusChange(agent.id, status)}
            />
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
