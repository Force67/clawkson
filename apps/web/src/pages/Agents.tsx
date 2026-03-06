import { useState, useEffect } from 'react'
import {
  Bot, Plus, Settings2, Trash2, Check, ChevronDown,
  Loader2, Cpu, Thermometer, Hash,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import { StatusBadge } from '../components/StatusBadge'
import { EmptyState } from '../components/EmptyState'
import { api, type Agent, type LlmConnector, type AgentStatus } from '../lib/api'
import styles from './Agents.module.css'

// ── Agent Config Panel ────────────────────────────────────────────

interface ConfigPanelProps {
  agent: Agent
  connectors: LlmConnector[]
  onSave: (updated: Agent) => void
  onClose: () => void
}

function ConfigPanel({ agent, connectors, onSave, onClose }: ConfigPanelProps) {
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
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')

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
          <button className={styles.panelClose} onClick={onClose}>✕</button>
        </div>

        <form onSubmit={handleSave} className={styles.panelBody}>
          {/* Identity */}
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

          {/* System Prompt */}
          <div className={styles.fieldSection}>
            <h4 className={styles.fieldSectionTitle}>System Prompt</h4>
            <div className={styles.formGroup}>
              <textarea
                className={`${styles.input} ${styles.textarea}`}
                value={systemPrompt}
                onChange={e => setSystemPrompt(e.target.value)}
                placeholder="You are a helpful assistant. Be concise and precise."
                rows={5}
              />
              <p className={styles.fieldHint}>
                Prepended to every conversation. Leave blank for no system context.
              </p>
            </div>
          </div>

          {/* Inference Settings */}
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
                <p className={styles.fieldHint}>0.0 – 2.0. Leave blank for provider default.</p>
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
                <p className={styles.fieldHint}>Max response length. Leave blank for default.</p>
              </div>
            </div>
          </div>

          {error && <p className={styles.errorMsg}>{error}</p>}

          <div className={styles.panelActions}>
            <Button variant="secondary" size="sm" type="button" onClick={onClose}>Cancel</Button>
            <Button variant="primary" size="sm" type="submit" disabled={submitting}>
              {submitting && <Loader2 size={13} className={styles.spinning} />}
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
            {submitting ? <Loader2 size={13} className={styles.spinning} /> : <Plus size={13} />}
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
            <div className={styles.agentAvatar}><Bot size={20} strokeWidth={1.5} /></div>
            <div className={`${styles.statusDot} ${styles[`status_${agent.status}`]}`} />
          </div>
          <div>
            <h3 className={styles.agentCardName}>{agent.name}</h3>
            <p className={styles.agentCardDesc}>{agent.description || 'No description'}</p>
          </div>
        </div>
        <div className={styles.agentCardRight}>
          <StatusBadge status={agent.status} />
        </div>
      </div>

      {/* Config summary */}
      <div className={styles.agentConfig}>
        {connector ? (
          <span className={styles.configTag}>
            <Cpu size={11} /> {connector.name}
          </span>
        ) : (
          <span className={`${styles.configTag} ${styles.configTagMuted}`}>
            <Cpu size={11} /> Using default connector
          </span>
        )}
        {agent.temperature != null && (
          <span className={styles.configTag}>
            <Thermometer size={11} /> {agent.temperature}
          </span>
        )}
        {agent.max_tokens != null && (
          <span className={styles.configTag}>
            <Hash size={11} /> {agent.max_tokens} tokens
          </span>
        )}
        {agent.system_prompt && (
          <span className={styles.configTag} title={agent.system_prompt}>
            System prompt set
          </span>
        )}
      </div>

      {/* Actions */}
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
          <button className={styles.configureBtn} onClick={onConfigure} title="Configure agent">
            <Settings2 size={15} /> Configure
          </button>
          <button className={styles.deleteAgentBtn} onClick={onDelete} title="Delete agent">
            <Trash2 size={14} />
          </button>
        </div>
      </div>
    </div>
  )
}

// ── Page ──────────────────────────────────────────────────────────

export function AgentsPage() {
  const [agents, setAgents] = useState<Agent[]>([])
  const [connectors, setConnectors] = useState<LlmConnector[]>([])
  const [loading, setLoading] = useState(true)
  const [showCreate, setShowCreate] = useState(false)
  const [configuring, setConfiguring] = useState<Agent | null>(null)

  useEffect(() => {
    Promise.all([api.agents.list(), api.llmConnectors.list()])
      .then(([agts, conns]) => { setAgents(agts); setConnectors(conns) })
      .finally(() => setLoading(false))
  }, [])

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

  return (
    <div className="fade-in">
      <PageHeader
        title="Agents"
        description="Configure and manage your AI agents. Each agent can have its own LLM connector, system prompt, and inference parameters."
      />

      <div className={styles.toolbar}>
        {!showCreate && (
          <Button variant="primary" size="sm" onClick={() => setShowCreate(true)}>
            <Plus size={14} /> New Agent
          </Button>
        )}
      </div>

      {showCreate && (
        <CreateForm onSave={handleCreate} onCancel={() => setShowCreate(false)} />
      )}

      {loading ? (
        <Card>
          <div className={styles.loadingState}>
            <Loader2 size={20} className={styles.spinning} />
            <span>Loading agents…</span>
          </div>
        </Card>
      ) : agents.length === 0 && !showCreate ? (
        <EmptyState
          icon={Bot}
          title="No agents yet"
          description="Create your first agent to start chatting with AI assistants."
          action={<Button variant="primary" size="sm" onClick={() => setShowCreate(true)}>Create Agent</Button>}
        />
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
          onSave={handleUpdate}
          onClose={() => setConfiguring(null)}
        />
      )}
    </div>
  )
}
