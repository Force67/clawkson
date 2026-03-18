import { useState, useEffect, useRef } from 'react'
import {
  Bot, Plus, Settings2, Trash2, Check, ChevronDown, ChevronRight,
  Loader2, Cpu, Thermometer, Hash, Container, RotateCw,
  BookOpen, Zap, Search, Filter, Share2,
  Shield, HardDrive, Terminal, Database, Globe, X, GitBranch,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import { StatusBadge } from '../components/StatusBadge'
import { EmptyState } from '../components/EmptyState'
import {
  api,
  type Agent,
  type LlmConnector,
  type LlmProviderType,
  type KnowledgeBase,
  type Skill,
  type Connector,
  type User,
  type AgentStatus,
  type AgentPermissions,
  type FilesystemMode,
  type ConnectorPolicy,
  type PolicyPreset,
  type ProxyRule,
  type HttpMethod,
} from '../lib/api'
import { useAuth } from '../lib/auth'
import styles from './Agents.module.css'

// ── LLM Provider labels for grouped selects ─────────────────────

const LLM_PROVIDER_LABELS: Record<LlmProviderType, string> = {
  open_router: 'OpenRouter',
  azure: 'Azure OpenAI',
  open_ai: 'OpenAI',
  custom: 'Custom / Ollama',
}

function groupConnectorsForSelect(connectors: LlmConnector[]): { label: string; items: LlmConnector[] }[] {
  const map = new Map<string, { label: string; items: LlmConnector[] }>()
  for (const c of connectors) {
    const key = c.provider_type === 'open_router' || c.provider_type === 'open_ai'
      ? c.provider_type
      : `${c.provider_type}:${c.api_base_url}`
    let group = map.get(key)
    if (!group) {
      group = { label: LLM_PROVIDER_LABELS[c.provider_type] ?? c.provider_type, items: [] }
      map.set(key, group)
    }
    group.items.push(c)
  }
  return Array.from(map.values())
}

const DEFAULT_PERMISSIONS: AgentPermissions = {
  network: { enabled: false, internet: false, local_network: false, allowed_domains: [] },
  filesystem: { mode: 'read_write', max_workspace_size_mb: null },
  execution: { shell: true, python: true, allowed_runtimes: [], max_execution_time_secs: 300 },
  resources: { max_processes: 256, max_tmp_size_mb: 256, max_storage_size_mb: 512, readonly_rootfs: true },
  data_access: { knowledge_bases: true, conversation_history: true },
}

// ── Rule Editor (inline) ──────────────────────────────────────────

const ALL_METHODS: HttpMethod[] = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD']

function RuleEditor({ rule, onChange, onRemove }: {
  rule: ProxyRule
  onChange: (updates: Partial<ProxyRule>) => void
  onRemove: () => void
}) {
  const toggleMethod = (m: HttpMethod) => {
    const has = rule.methods.includes(m)
    const next = has ? rule.methods.filter(x => x !== m) : [...rule.methods, m]
    onChange({ methods: next })
  }

  return (
    <div className={styles.ruleRow}>
      <div className={styles.ruleMethodRow}>
        {ALL_METHODS.map(m => (
          <button
            key={m}
            type="button"
            className={`${styles.methodChip} ${rule.methods.includes(m) ? styles.methodChipActive : ''}`}
            onClick={() => toggleMethod(m)}
          >
            {m}
          </button>
        ))}
        <button type="button" className={styles.ruleRemoveBtn} onClick={onRemove} title="Remove rule">
          <X size={11} />
        </button>
      </div>
      <input
        className={styles.input}
        value={rule.path_pattern}
        onChange={e => onChange({ path_pattern: e.target.value })}
        placeholder="Path pattern (e.g. /api/v1/**)"
        style={{ fontSize: 12, fontFamily: 'var(--font-mono)' }}
      />
      <input
        className={styles.input}
        value={rule.description}
        onChange={e => onChange({ description: e.target.value })}
        placeholder="Description (optional)"
        style={{ fontSize: 12 }}
      />
    </div>
  )
}

// ── Connector Policies Modal ──────────────────────────────────────

const CONN_TYPE_LABEL: Record<string, string> = {
  azure_devops: 'Azure DevOps',
  telegram: 'Telegram',
  gmail: 'Gmail',
  slack: 'Slack',
  custom: 'Custom',
}

const CONN_TYPE_COLOR: Record<string, string> = {
  azure_devops: '#0078d4',
  telegram: '#229ed9',
  gmail: '#ea4335',
  slack: '#4a154b',
  custom: '#6b7280',
}

interface ConnectorPoliciesModalProps {
  agent: Agent
  platformConnectors: Connector[]
  policyPresets: PolicyPreset[]
  onSave: (updated: Agent) => void
  onClose: () => void
}

function ConnectorPoliciesModal({ agent, platformConnectors, policyPresets, onSave, onClose }: ConnectorPoliciesModalProps) {
  const enabledConnectors = platformConnectors.filter(c => c.enabled)
  const [policies, setPolicies] = useState<ConnectorPolicy[]>(agent.connector_policies ?? [])
  const [activeConnId, setActiveConnId] = useState<string | null>(
    enabledConnectors.length > 0 ? enabledConnectors[0].id : null
  )
  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState(false)

  const activeConn = enabledConnectors.find(c => c.id === activeConnId) ?? null
  const activePolicy = activeConn ? policies.find(p => p.connector_id === activeConn.id) ?? null : null
  const activePresets = activeConn ? policyPresets.filter(p => p.connector_type === activeConn.connector_type) : []

  const applyPreset = (preset: PolicyPreset) => {
    if (!activeConn) return
    const newPolicy: ConnectorPolicy = {
      connector_id: activeConn.id,
      allow: preset.policy.allow,
      deny: preset.policy.deny,
      rate_limit_rpm: preset.policy.rate_limit_rpm,
    }
    setPolicies(prev => [...prev.filter(p => p.connector_id !== activeConn.id), newPolicy])
  }

  const removePolicy = () => {
    if (!activeConn) return
    setPolicies(prev => prev.filter(p => p.connector_id !== activeConn.id))
  }

  const addRule = (type: 'allow' | 'deny') => {
    if (!activeConn || !activePolicy) return
    const newRule: ProxyRule = { methods: ['GET'], path_pattern: '/**', description: '' }
    setPolicies(prev => prev.map(p =>
      p.connector_id === activeConn.id ? { ...p, [type]: [...p[type], newRule] } : p
    ))
  }

  const updateRule = (type: 'allow' | 'deny', idx: number, updates: Partial<ProxyRule>) => {
    if (!activeConn) return
    setPolicies(prev => prev.map(p => {
      if (p.connector_id !== activeConn.id) return p
      const rules = [...p[type]]
      rules[idx] = { ...rules[idx], ...updates }
      return { ...p, [type]: rules }
    }))
  }

  const removeRule = (type: 'allow' | 'deny', idx: number) => {
    if (!activeConn) return
    setPolicies(prev => prev.map(p => {
      if (p.connector_id !== activeConn.id) return p
      return { ...p, [type]: p[type].filter((_, i) => i !== idx) }
    }))
  }

  const updateRateLimit = (val: string) => {
    if (!activeConn) return
    setPolicies(prev => prev.map(p =>
      p.connector_id === activeConn.id ? { ...p, rate_limit_rpm: val ? parseInt(val) : null } : p
    ))
  }

  const handleSave = async () => {
    setSaving(true)
    try {
      const updated = await api.agents.patch(agent.id, { connector_policies: policies })
      setSaved(true)
      setTimeout(() => { onSave(updated) }, 400)
    } catch (err) {
      console.error('Failed to save policies:', err)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className={styles.policyOverlay} onClick={onClose}>
      <div className={styles.policyModal} onClick={e => e.stopPropagation()}>
        {/* Header */}
        <div className={styles.policyHeader}>
          <div className={styles.policyHeaderLeft}>
            <div className={styles.policyHeaderIcon}><Shield size={16} /></div>
            <div>
              <h2 className={styles.policyTitle}>Connector Policies</h2>
              <p className={styles.policySub}>{agent.name}</p>
            </div>
          </div>
          <button className={styles.panelClose} onClick={onClose}>&#x2715;</button>
        </div>

        <div className={styles.policyLayout}>
          {/* Left sidebar — connector list */}
          <div className={styles.policySidebar}>
            <div className={styles.policySidebarLabel}>Connectors</div>
            {enabledConnectors.length === 0 ? (
              <p className={styles.fieldHint} style={{ padding: '0 12px' }}>No enabled connectors.</p>
            ) : (
              <div className={styles.policySidebarList}>
                {enabledConnectors.map(conn => {
                  const hasPolicy = policies.some(p => p.connector_id === conn.id)
                  const isActive = conn.id === activeConnId
                  return (
                    <button
                      key={conn.id}
                      className={`${styles.policySidebarItem} ${isActive ? styles.policySidebarItemActive : ''}`}
                      onClick={() => setActiveConnId(conn.id)}
                    >
                      <span
                        className={styles.connDot}
                        style={{ background: CONN_TYPE_COLOR[conn.connector_type] ?? '#6b7280' }}
                      />
                      <div className={styles.policySidebarItemInfo}>
                        <span className={styles.policySidebarItemName}>{conn.name}</span>
                        <span className={styles.policySidebarItemType}>
                          {CONN_TYPE_LABEL[conn.connector_type] ?? conn.connector_type}
                        </span>
                      </div>
                      {hasPolicy ? (
                        <span className={styles.policyBadgeSm}>Restricted</span>
                      ) : (
                        <span className={styles.policyBadgeOpen}>Open</span>
                      )}
                    </button>
                  )
                })}
              </div>
            )}
          </div>

          {/* Right panel — policy editor */}
          <div className={styles.policyEditor}>
            {!activeConn ? (
              <div className={styles.policyEmptyState}>
                <Globe size={32} strokeWidth={1} />
                <p>Select a connector to manage its access policy.</p>
              </div>
            ) : !activePolicy ? (
              <div className={styles.policyEditorInner}>
                <div className={styles.policyStatusBar}>
                  <div className={styles.policyStatusDot} />
                  <span>Unrestricted access — no policy active</span>
                </div>
                <p className={styles.policyDesc}>
                  This agent can make any HTTP request through <strong>{activeConn.name}</strong>.
                  Apply a preset or create a custom policy to restrict access.
                </p>
                {activePresets.length > 0 && (
                  <div className={styles.presetGrid}>
                    {activePresets.map(preset => (
                      <button
                        key={preset.name}
                        type="button"
                        className={styles.presetCard}
                        onClick={() => applyPreset(preset)}
                      >
                        <Shield size={14} className={styles.presetCardIcon} />
                        <span className={styles.presetCardLabel}>{preset.label}</span>
                        <span className={styles.presetCardMeta}>
                          {preset.policy.allow.length} allow
                          {preset.policy.deny.length > 0 ? `, ${preset.policy.deny.length} deny` : ''}
                          {preset.policy.rate_limit_rpm ? ` · ${preset.policy.rate_limit_rpm} rpm` : ''}
                        </span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            ) : (
              <div className={styles.policyEditorInner}>
                {/* Active policy status + presets */}
                <div className={styles.policyStatusBarActive}>
                  <div className={styles.policyStatusDotActive} />
                  <span>Policy active</span>
                </div>

                {activePresets.length > 0 && (
                  <div className={styles.presetSwitcher}>
                    <span className={styles.presetSwitcherLabel}>Presets</span>
                    <div className={styles.presetChips}>
                      {activePresets.map(preset => (
                        <button
                          key={preset.name}
                          type="button"
                          className={styles.presetChip}
                          onClick={() => applyPreset(preset)}
                        >
                          {preset.label}
                        </button>
                      ))}
                    </div>
                  </div>
                )}

                {/* Allow rules */}
                <div className={styles.ruleSection}>
                  <div className={styles.ruleSectionHeader}>
                    <span className={styles.ruleLabel}>
                      <Check size={10} className={styles.ruleIconAllow} /> Allow rules
                    </span>
                    <button type="button" className={styles.addRuleBtn} onClick={() => addRule('allow')}>
                      <Plus size={10} /> Add
                    </button>
                  </div>
                  {activePolicy.allow.map((rule, idx) => (
                    <RuleEditor
                      key={idx}
                      rule={rule}
                      onChange={updates => updateRule('allow', idx, updates)}
                      onRemove={() => removeRule('allow', idx)}
                    />
                  ))}
                  {activePolicy.allow.length === 0 && (
                    <p className={styles.fieldHint}>No allow rules — all requests blocked.</p>
                  )}
                </div>

                {/* Deny rules */}
                <div className={styles.ruleSection}>
                  <div className={styles.ruleSectionHeader}>
                    <span className={styles.ruleLabel}>
                      <X size={10} className={styles.ruleIconDeny} /> Deny rules
                    </span>
                    <button type="button" className={styles.addRuleBtn} onClick={() => addRule('deny')}>
                      <Plus size={10} /> Add
                    </button>
                  </div>
                  {activePolicy.deny.map((rule, idx) => (
                    <RuleEditor
                      key={idx}
                      rule={rule}
                      onChange={updates => updateRule('deny', idx, updates)}
                      onRemove={() => removeRule('deny', idx)}
                    />
                  ))}
                  {activePolicy.deny.length === 0 && (
                    <p className={styles.fieldHint}>No deny overrides.</p>
                  )}
                </div>

                {/* Rate limit */}
                <div className={styles.formGroup}>
                  <label className={styles.label}>Rate limit (requests/min)</label>
                  <input
                    className={styles.input}
                    value={activePolicy.rate_limit_rpm ?? ''}
                    onChange={e => updateRateLimit(e.target.value)}
                    placeholder="Unlimited"
                    type="number" min="1"
                  />
                </div>

                <button type="button" className={styles.removePolicyBtn} onClick={removePolicy}>
                  <Trash2 size={11} /> Remove policy (unrestrict)
                </button>
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className={styles.policyFooter}>
          <Button variant="secondary" size="sm" onClick={onClose}>Cancel</Button>
          <Button variant="primary" size="sm" onClick={handleSave} disabled={saving || saved}>
            {saving ? <Loader2 size={13} className={styles.spinning} />
              : saved ? <Check size={13} />
              : <Check size={13} />}
            {saved ? 'Saved' : 'Save Policies'}
          </Button>
        </div>
      </div>
    </div>
  )
}

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
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'
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
  const [subtaskConnectorId, setSubtaskConnectorId] = useState(agent.subtask_llm_connector_id ?? '')
  const [containerEnabled, setContainerEnabled] = useState(agent.container_enabled)
  const [cpuLimit, setCpuLimit] = useState(
    agent.container_config?.cpu_limit != null ? String(agent.container_config.cpu_limit) : ''
  )
  const [memoryLimit, setMemoryLimit] = useState(
    agent.container_config?.memory_limit_mb != null ? String(agent.container_config.memory_limit_mb) : ''
  )
  const [networkEnabled] = useState(agent.container_config?.network_enabled ?? false)
  const [permissions, setPermissions] = useState<AgentPermissions>(
    agent.container_config?.permissions ?? DEFAULT_PERMISSIONS
  )
  const [expandedPerms, setExpandedPerms] = useState<Set<string>>(new Set())
  const [linkedKbIds, setLinkedKbIds] = useState<Set<string>>(new Set())
  const [kbLoading, setKbLoading] = useState(true)
  const [linkedSkillIds, setLinkedSkillIds] = useState<Set<string>>(new Set())
  const [skillLoading, setSkillLoading] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')

  // Track running containers for this agent + whether container config changed
  const [runningContainerCount, setRunningContainerCount] = useState(0)
  const [containerConfigDirty, setContainerConfigDirty] = useState(false)
  const savedConfig = useRef(agent.container_config)

  useEffect(() => {
    if (!agent.container_enabled) return
    api.containers.list().then(containers => {
      const running = containers.filter(c => c.agent_id === agent.id && c.state === 'running')
      setRunningContainerCount(running.length)
    }).catch(() => {})
  }, [agent.id, agent.container_enabled])

  // Mark dirty when any container resource field diverges from saved value
  useEffect(() => {
    if (runningContainerCount === 0) { setContainerConfigDirty(false); return }
    const saved = savedConfig.current
    const dirty =
      (cpuLimit || null) !== (saved?.cpu_limit != null ? String(saved.cpu_limit) : null) ||
      (memoryLimit || null) !== (saved?.memory_limit_mb != null ? String(saved.memory_limit_mb) : null) ||
      permissions.resources.max_storage_size_mb !== (saved?.permissions?.resources.max_storage_size_mb ?? 512) ||
      permissions.resources.max_tmp_size_mb !== (saved?.permissions?.resources.max_tmp_size_mb ?? 256) ||
      permissions.resources.max_processes !== (saved?.permissions?.resources.max_processes ?? 256) ||
      permissions.resources.readonly_rootfs !== (saved?.permissions?.resources.readonly_rootfs ?? true)
    setContainerConfigDirty(dirty)
  }, [cpuLimit, memoryLimit, permissions.resources, runningContainerCount])

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
        subtask_llm_connector_id: subtaskConnectorId || undefined,
        container_enabled: containerEnabled,
        container_config: containerEnabled ? {
          cpu_limit: cpuLimit ? parseFloat(cpuLimit) : null,
          memory_limit_mb: memoryLimit ? parseInt(memoryLimit) : null,
          network_enabled: networkEnabled,
          permissions,
        } : undefined,
      })
      savedConfig.current = updated.container_config
      onSave(updated)
    } catch (err) {
      setError(String(err))
    } finally {
      setSubmitting(false)
    }
  }

  const tempNum = parseFloat(temperature)
  const tempValid = !temperature || (!isNaN(tempNum) && tempNum >= 0 && tempNum <= 2)

  const togglePermGroup = (group: string) => {
    setExpandedPerms(prev => {
      const next = new Set(prev)
      next.has(group) ? next.delete(group) : next.add(group)
      return next
    })
  }

  const updatePerm = <K extends keyof AgentPermissions>(
    group: K,
    updates: Partial<AgentPermissions[K]>
  ) => {
    setPermissions(prev => ({
      ...prev,
      [group]: { ...prev[group], ...updates },
    }))
  }

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
            {isAdmin && (
              <div className={styles.formGroup}>
                <label className={styles.toggleLabel}>
                  <input
                    type="checkbox"
                    checked={agent.shared}
                    onChange={async () => {
                      try {
                        const updated = await api.agents.patch(agent.id, { shared: !agent.shared })
                        onSave(updated)
                      } catch (err) {
                        console.error('Failed to toggle shared:', err)
                      }
                    }}
                    className={styles.checkbox}
                  />
                  <Share2 size={11} /> Shared with all users
                </label>
                <p className={styles.fieldHint}>
                  Shared agents are visible and usable by everyone. Unshared agents are only visible to the owner.
                </p>
              </div>
            )}
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
                  {groupConnectorsForSelect(connectors).map(g => (
                    <optgroup key={g.label} label={g.label}>
                      {g.items.map(c => (
                        <option key={c.id} value={c.id}>{c.model}{c.azure_deployment ? ` (${c.azure_deployment})` : ''}</option>
                      ))}
                    </optgroup>
                  ))}
                </select>
                <ChevronDown size={13} className={styles.selectChevron} />
              </div>
            </div>
            <div className={styles.formGroup}>
              <label className={styles.label}>
                <GitBranch size={11} /> Sub-task LLM
              </label>
              <div className={styles.selectWrap}>
                <select
                  className={styles.select}
                  value={subtaskConnectorId}
                  onChange={e => setSubtaskConnectorId(e.target.value)}
                >
                  <option value="">Same as primary</option>
                  {groupConnectorsForSelect(connectors).map(g => (
                    <optgroup key={g.label} label={g.label}>
                      {g.items.map(c => (
                        <option key={c.id} value={c.id}>{c.model}{c.azure_deployment ? ` (${c.azure_deployment})` : ''}</option>
                      ))}
                    </optgroup>
                  ))}
                </select>
                <ChevronDown size={13} className={styles.selectChevron} />
              </div>
              <p className={styles.fieldHint}>
                Optional cheaper/faster model for parallel sub-tasks via delegate_tasks.
              </p>
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
            )}
            {containerConfigDirty && runningContainerCount > 0 && (
              <div className={styles.restartBanner}>
                <RotateCw size={13} />
                <span>
                  {runningContainerCount} running container{runningContainerCount > 1 ? 's' : ''} will
                  use the old config until restarted.
                </span>
              </div>
            )}
          </div>

          {containerEnabled && (
          <div className={styles.fieldSection}>
            <h4 className={styles.fieldSectionTitle}>Permissions</h4>
            <p className={styles.fieldHint} style={{ marginBottom: 12, marginTop: -4 }}>
              Fine-grained control over what the agent's sandbox can access.
            </p>

            {/* ── Network ── */}
            <div className={styles.permGroup}>
              <div className={styles.permGroupHeader} onClick={() => togglePermGroup('network')}>
                <div className={styles.permGroupLeft}>
                  <div className={`${styles.permIcon} ${styles.permIconNetwork}`}><Shield size={13} /></div>
                  <div>
                    <span className={styles.permGroupName}>Network</span>
                    <span className={styles.permGroupSummary}>
                      {permissions.network.enabled ? 'Enabled' : 'Disabled'}
                    </span>
                  </div>
                </div>
                <div className={styles.permGroupRight}>
                  <input
                    type="checkbox"
                    className={styles.checkbox}
                    checked={permissions.network.enabled}
                    onChange={e => { e.stopPropagation(); updatePerm('network', { enabled: e.target.checked }) }}
                  />
                  {expandedPerms.has('network') ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                </div>
              </div>
              {expandedPerms.has('network') && permissions.network.enabled && (
                <div className={styles.permGroupBody}>
                  <label className={styles.permRow}>
                    <span>Internet access</span>
                    <input type="checkbox" className={styles.checkbox}
                      checked={permissions.network.internet}
                      onChange={e => updatePerm('network', { internet: e.target.checked })}
                    />
                  </label>
                  <label className={styles.permRow}>
                    <span>Local network (private IPs)</span>
                    <input type="checkbox" className={styles.checkbox}
                      checked={permissions.network.local_network}
                      onChange={e => updatePerm('network', { local_network: e.target.checked })}
                    />
                  </label>
                  <div className={styles.formGroup}>
                    <label className={styles.label}>Allowed domains (comma-separated)</label>
                    <input
                      className={styles.input}
                      value={permissions.network.allowed_domains.join(', ')}
                      onChange={e => updatePerm('network', {
                        allowed_domains: e.target.value.split(',').map(s => s.trim()).filter(Boolean)
                      })}
                      placeholder="api.example.com, cdn.example.com"
                    />
                    <p className={styles.fieldHint}>Leave empty to allow all domains when internet is enabled.</p>
                  </div>
                </div>
              )}
            </div>

            {/* ── Filesystem ── */}
            <div className={styles.permGroup}>
              <div className={styles.permGroupHeader} onClick={() => togglePermGroup('filesystem')}>
                <div className={styles.permGroupLeft}>
                  <div className={`${styles.permIcon} ${styles.permIconFilesystem}`}><HardDrive size={13} /></div>
                  <div>
                    <span className={styles.permGroupName}>Filesystem</span>
                    <span className={styles.permGroupSummary}>
                      {permissions.filesystem.mode === 'read_write' ? 'Read/Write' : permissions.filesystem.mode === 'read_only' ? 'Read Only' : 'None'}
                    </span>
                  </div>
                </div>
                <div className={styles.permGroupRight}>
                  {expandedPerms.has('filesystem') ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                </div>
              </div>
              {expandedPerms.has('filesystem') && (
                <div className={styles.permGroupBody}>
                  <div className={styles.formGroup}>
                    <label className={styles.label}>Workspace mode</label>
                    <div className={styles.selectWrap}>
                      <select className={styles.select}
                        value={permissions.filesystem.mode}
                        onChange={e => updatePerm('filesystem', { mode: e.target.value as FilesystemMode })}
                      >
                        <option value="read_write">Read / Write</option>
                        <option value="read_only">Read Only</option>
                        <option value="none">No Filesystem</option>
                      </select>
                      <ChevronDown size={13} className={styles.selectChevron} />
                    </div>
                  </div>
                  <div className={styles.formGroup}>
                    <label className={styles.label}>Max workspace size (MB)</label>
                    <input className={styles.input}
                      value={permissions.filesystem.max_workspace_size_mb ?? ''}
                      onChange={e => updatePerm('filesystem', {
                        max_workspace_size_mb: e.target.value ? parseInt(e.target.value) : null
                      })}
                      placeholder="No limit" type="number" min="1"
                    />
                  </div>
                </div>
              )}
            </div>

            {/* ── Execution ── */}
            <div className={styles.permGroup}>
              <div className={styles.permGroupHeader} onClick={() => togglePermGroup('execution')}>
                <div className={styles.permGroupLeft}>
                  <div className={`${styles.permIcon} ${styles.permIconExecution}`}><Terminal size={13} /></div>
                  <div>
                    <span className={styles.permGroupName}>Code Execution</span>
                    <span className={styles.permGroupSummary}>
                      {[permissions.execution.shell && 'Shell', permissions.execution.python && 'Python'].filter(Boolean).join(', ') || 'Restricted'}
                    </span>
                  </div>
                </div>
                <div className={styles.permGroupRight}>
                  {expandedPerms.has('execution') ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                </div>
              </div>
              {expandedPerms.has('execution') && (
                <div className={styles.permGroupBody}>
                  <label className={styles.permRow}>
                    <span>Shell (bash/sh)</span>
                    <input type="checkbox" className={styles.checkbox}
                      checked={permissions.execution.shell}
                      onChange={e => updatePerm('execution', { shell: e.target.checked })}
                    />
                  </label>
                  <label className={styles.permRow}>
                    <span>Python</span>
                    <input type="checkbox" className={styles.checkbox}
                      checked={permissions.execution.python}
                      onChange={e => updatePerm('execution', { python: e.target.checked })}
                    />
                  </label>
                  <div className={styles.formGroup}>
                    <label className={styles.label}>Additional runtimes (comma-separated)</label>
                    <input className={styles.input}
                      value={permissions.execution.allowed_runtimes.join(', ')}
                      onChange={e => updatePerm('execution', {
                        allowed_runtimes: e.target.value.split(',').map(s => s.trim()).filter(Boolean)
                      })}
                      placeholder="node, ruby, go"
                    />
                  </div>
                  <div className={styles.formGroup}>
                    <label className={styles.label}>Max execution time (seconds)</label>
                    <input className={styles.input}
                      value={permissions.execution.max_execution_time_secs ?? ''}
                      onChange={e => updatePerm('execution', {
                        max_execution_time_secs: e.target.value ? parseInt(e.target.value) : null
                      })}
                      placeholder="300" type="number" min="1" max="3600"
                    />
                  </div>
                </div>
              )}
            </div>

            {/* ── Resources ── */}
            <div className={styles.permGroup}>
              <div className={styles.permGroupHeader} onClick={() => togglePermGroup('resources')}>
                <div className={styles.permGroupLeft}>
                  <div className={`${styles.permIcon} ${styles.permIconResources}`}><Cpu size={13} /></div>
                  <div>
                    <span className={styles.permGroupName}>Resources</span>
                    <span className={styles.permGroupSummary}>
                      {permissions.resources.readonly_rootfs ? 'Read-only rootfs' : 'Writable rootfs'}
                      {permissions.resources.max_storage_size_mb ? ` · ${permissions.resources.max_storage_size_mb} MB storage` : ''}
                    </span>
                  </div>
                </div>
                <div className={styles.permGroupRight}>
                  {expandedPerms.has('resources') ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                </div>
              </div>
              {expandedPerms.has('resources') && (
                <div className={styles.permGroupBody}>
                  <label className={styles.permRow}>
                    <span>Read-only root filesystem</span>
                    <input type="checkbox" className={styles.checkbox}
                      checked={permissions.resources.readonly_rootfs}
                      onChange={e => updatePerm('resources', { readonly_rootfs: e.target.checked })}
                    />
                  </label>
                  <div className={styles.storageGauge} data-disabled={!permissions.resources.readonly_rootfs || undefined}>
                    <div className={styles.storageHeader}>
                      <label className={styles.label}>Package storage</label>
                      <span className={styles.storageValue}>
                        {(permissions.resources.max_storage_size_mb ?? 512) >= 1024
                          ? `${((permissions.resources.max_storage_size_mb ?? 512) / 1024).toFixed(1).replace(/\.0$/, '')} GB`
                          : `${permissions.resources.max_storage_size_mb ?? 512} MB`}
                      </span>
                    </div>
                    <div className={styles.storageTrack}>
                      <div
                        className={styles.storageFill}
                        style={{ width: `${(((permissions.resources.max_storage_size_mb ?? 512) - 256) / (2048 - 256)) * 100}%` }}
                      />
                      <input
                        type="range" min="256" max="2048" step="128"
                        className={styles.storageRange}
                        value={permissions.resources.max_storage_size_mb ?? 512}
                        onChange={e => updatePerm('resources', { max_storage_size_mb: parseInt(e.target.value) })}
                        disabled={!permissions.resources.readonly_rootfs}
                      />
                    </div>
                    <div className={styles.storageStops}>
                      <span>256 MB</span>
                      <span>1 GB</span>
                      <span>2 GB</span>
                    </div>
                    <p className={styles.fieldHint}>
                      Writable /usr/local for pip, npm, and other package managers.
                    </p>
                  </div>
                  <div className={styles.formRow}>
                    <div className={styles.formGroup}>
                      <label className={styles.label}>Max processes (PID limit)</label>
                      <input className={styles.input}
                        value={permissions.resources.max_processes ?? ''}
                        onChange={e => updatePerm('resources', {
                          max_processes: e.target.value ? parseInt(e.target.value) : null
                        })}
                        placeholder="256" type="number" min="1"
                      />
                    </div>
                    <div className={styles.formGroup}>
                      <label className={styles.label}>Tmp space (MB)</label>
                      <input className={styles.input}
                        value={permissions.resources.max_tmp_size_mb ?? ''}
                        onChange={e => updatePerm('resources', {
                          max_tmp_size_mb: e.target.value ? parseInt(e.target.value) : null
                        })}
                        placeholder="256" type="number" min="16"
                      />
                    </div>
                  </div>
                </div>
              )}
            </div>

            {/* ── Data Access ── */}
            <div className={styles.permGroup}>
              <div className={styles.permGroupHeader} onClick={() => togglePermGroup('data_access')}>
                <div className={styles.permGroupLeft}>
                  <div className={`${styles.permIcon} ${styles.permIconData}`}><Database size={13} /></div>
                  <div>
                    <span className={styles.permGroupName}>Data Access</span>
                    <span className={styles.permGroupSummary}>
                      {[permissions.data_access.knowledge_bases && 'KB', permissions.data_access.conversation_history && 'History'].filter(Boolean).join(', ') || 'Restricted'}
                    </span>
                  </div>
                </div>
                <div className={styles.permGroupRight}>
                  {expandedPerms.has('data_access') ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                </div>
              </div>
              {expandedPerms.has('data_access') && (
                <div className={styles.permGroupBody}>
                  <label className={styles.permRow}>
                    <span>Knowledge bases</span>
                    <input type="checkbox" className={styles.checkbox}
                      checked={permissions.data_access.knowledge_bases}
                      onChange={e => updatePerm('data_access', { knowledge_bases: e.target.checked })}
                    />
                  </label>
                  <label className={styles.permRow}>
                    <span>Conversation history</span>
                    <input type="checkbox" className={styles.checkbox}
                      checked={permissions.data_access.conversation_history}
                      onChange={e => updatePerm('data_access', { conversation_history: e.target.checked })}
                    />
                  </label>
                </div>
              )}
            </div>
          </div>
          )}

          <div className={styles.fieldSection}>
            <h4 className={styles.fieldSectionTitle}>Knowledge Bases</h4>
            {knowledgeBases.length === 0 ? (
              <p className={styles.fieldHint}>No knowledge bases available.</p>
            ) : kbLoading ? (
              <div className={styles.linkedLoading}><Loader2 size={13} className={styles.spinning} /><span>Loading...</span></div>
            ) : (
              <div className={styles.linkedList}>
                {knowledgeBases.map(kb => (
                  <label key={kb.id} className={styles.linkedItem}>
                    <input type="checkbox" className={styles.checkbox} checked={linkedKbIds.has(kb.id)} onChange={() => handleKbToggle(kb.id)} />
                    <div className={styles.linkedItemInfo}>
                      <span className={styles.linkedItemName}><BookOpen size={11} /> {kb.name}</span>
                      <span className={styles.linkedItemMeta}>{kb.entry_count} {kb.entry_count === 1 ? 'entry' : 'entries'}</span>
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
              <div className={styles.linkedLoading}><Loader2 size={13} className={styles.spinning} /><span>Loading...</span></div>
            ) : (
              <div className={styles.linkedList}>
                {skills.map(skill => (
                  <label key={skill.id} className={styles.linkedItem}>
                    <input type="checkbox" className={styles.checkbox} checked={linkedSkillIds.has(skill.id)} onChange={() => handleSkillToggle(skill.id)} />
                    <div className={styles.linkedItemInfo}>
                      <span className={styles.linkedItemName}><Zap size={11} /> /{skill.name}</span>
                      <span className={styles.linkedItemMeta}>{skill.description}</span>
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
  subtaskConnector?: LlmConnector
  canManage: boolean
  ownerLabel?: string
  onConfigure: () => void
  onPolicies: () => void
  onDelete: () => void
  onStatusChange: (status: AgentStatus) => void
}

function formatModel(c: LlmConnector) {
  return c.azure_deployment ? `${c.model} (${c.azure_deployment})` : c.model
}

function AgentCard({ agent, connector, subtaskConnector, canManage, ownerLabel, onConfigure, onPolicies, onDelete, onStatusChange }: AgentCardProps) {
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

      {/* Inference models */}
      <div className={styles.inferenceBlock}>
        <div className={styles.inferenceRow}>
          <Cpu size={12} className={styles.inferenceIcon} />
          <span className={`${styles.inferenceModel} ${!connector ? styles.inferenceDefault : ''}`}>
            {connector ? formatModel(connector) : 'Default'}
          </span>
          {connector && (
            <span className={styles.inferenceProvider}>
              {LLM_PROVIDER_LABELS[connector.provider_type]}
            </span>
          )}
        </div>
        {subtaskConnector && (
          <div className={`${styles.inferenceRow} ${styles.inferenceRowSub}`}>
            <GitBranch size={11} className={styles.inferenceIcon} />
            <span className={styles.inferenceModel}>{formatModel(subtaskConnector)}</span>
            <span className={styles.inferenceProvider}>
              {LLM_PROVIDER_LABELS[subtaskConnector.provider_type]}
            </span>
          </div>
        )}
      </div>

      <div className={styles.agentConfig}>
        {ownerLabel && <span className={styles.configTag}>{ownerLabel}</span>}
        {agent.temperature != null && <span className={styles.configTag}><Thermometer size={10} /> {agent.temperature}</span>}
        {agent.max_tokens != null && <span className={styles.configTag}><Hash size={10} /> {agent.max_tokens}</span>}
        {agent.container_enabled && <span className={styles.configTag}><Container size={10} /> Sandbox</span>}
        {agent.shared && <span className={styles.configTag}><Share2 size={10} /> Shared</span>}
      </div>

      <div className={styles.cardFooter}>
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
        {canManage && (
          <div className={styles.cardActions}>
            <button className={styles.actionBtn} onClick={onPolicies} title="Connector policies" aria-label="Connector policies">
              <Shield size={14} />
            </button>
            <button className={styles.actionBtn} onClick={onConfigure} title="Configure" aria-label="Configure agent">
              <Settings2 size={14} />
            </button>
            <div className={styles.actionDivider} />
            <button className={`${styles.actionBtn} ${styles.actionBtnDanger}`} onClick={onDelete} title="Delete agent" aria-label="Delete agent">
              <Trash2 size={14} />
            </button>
          </div>
        )}
      </div>
    </div>
  )
}

// ── Page ──────────────────────────────────────────────────────────

export function AgentsPage() {
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'
  const [agents, setAgents] = useState<Agent[]>([])
  const [connectors, setConnectors] = useState<LlmConnector[]>([])
  const [platformConnectors, setPlatformConnectors] = useState<Connector[]>([])
  const [policyPresets, setPolicyPresets] = useState<PolicyPreset[]>([])
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBase[]>([])
  const [skills, setSkills] = useState<Skill[]>([])
  const [allUsers, setAllUsers] = useState<User[]>([])
  const [loading, setLoading] = useState(true)
  const [showCreate, setShowCreate] = useState(false)
  const [configuring, setConfiguring] = useState<Agent | null>(null)
  const [policyAgent, setPolicyAgent] = useState<Agent | null>(null)
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<AgentStatus | 'all'>('all')

  useEffect(() => {
    const promises: Promise<unknown>[] = [
      api.agents.list(),
      api.llmConnectors.list(),
      api.knowledge.listBases(),
      api.skills.list(),
      api.connectors.list(),
      api.policyPresets.list(),
    ]
    // Admins load user list to show creator names
    if (isAdmin) promises.push(api.admin.listUsers())

    Promise.all(promises)
      .then(([agts, conns, kbs, sks, platConns, presets, users]) => {
        setAgents(agts as Agent[])
        setConnectors(conns as LlmConnector[])
        setKnowledgeBases(kbs as KnowledgeBase[])
        setSkills(sks as Skill[])
        setPlatformConnectors(platConns as Connector[])
        setPolicyPresets(presets as PolicyPreset[])
        if (users) setAllUsers(users as User[])
      })
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

  const handlePolicySave = (updated: Agent) => {
    setAgents(prev => prev.map(a => a.id === updated.id ? updated : a))
    setPolicyAgent(null)
  }

  const handleDelete = async (id: string) => {
    await api.agents.delete(id)
    setAgents(prev => prev.filter(a => a.id !== id))
  }

  const handleStatusChange = async (id: string, status: AgentStatus) => {
    const updated = await api.agents.patch(id, { status })
    setAgents(prev => prev.map(a => a.id === updated.id ? updated : a))
  }

  const ownerLabel = (agent: Agent): string | undefined => {
    if (!agent.owner_id) return undefined
    if (agent.owner_id === user?.id) return agent.shared ? 'You (shared)' : 'You'
    if (agent.shared) {
      const owner = allUsers.find(u => u.id === agent.owner_id)
      return owner ? `by ${owner.display_name}` : 'Shared'
    }
    // Admin seeing a non-shared agent from another user
    const owner = allUsers.find(u => u.id === agent.owner_id)
    return owner ? `by ${owner.display_name}` : undefined
  }

  const filtered = agents.filter(a => {
    if (statusFilter !== 'all' && a.status !== statusFilter) return false
    if (search) {
      const q = search.toLowerCase()
      return a.name.toLowerCase().includes(q) || a.description.toLowerCase().includes(q)
    }
    return true
  })

  const onlineCount = agents.filter(a => a.status === 'online').length
  const offlineCount = agents.filter(a => a.status === 'offline').length

  return (
    <div className="fade-in">
      <PageHeader
        title="Agents"
        description="Create, configure, and manage your AI agents."
      />

      <div className={styles.toolbar}>
        <div className={styles.toolbarLeft}>
          <div className={styles.searchBox}>
            <Search size={14} />
            <input
              className={styles.searchInput}
              placeholder="Search agents..."
              value={search}
              onChange={e => setSearch(e.target.value)}
            />
          </div>
          <div className={styles.filterGroup}>
            <Filter size={12} />
            {(['all', 'online', 'offline'] as const).map(f => (
              <button
                key={f}
                className={`${styles.filterBtn} ${statusFilter === f ? styles.filterBtnActive : ''}`}
                onClick={() => setStatusFilter(f)}
              >
                {f === 'all' ? `All (${agents.length})` : f === 'online' ? `Online (${onlineCount})` : `Offline (${offlineCount})`}
              </button>
            ))}
          </div>
        </div>
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
            <span>Loading agents...</span>
          </div>
        </Card>
      ) : filtered.length === 0 && !showCreate ? (
        agents.length === 0 ? (
          <EmptyState
            icon={Bot}
            title="No agents yet"
            description="Create your first agent to start chatting with AI assistants."
            action={<Button variant="primary" size="sm" onClick={() => setShowCreate(true)}>Create Agent</Button>}
          />
        ) : (
          <div className={styles.noResults}>
            No agents match your filters.
          </div>
        )
      ) : (
        <div className={styles.agentGrid}>
          {filtered.map(agent => (
            <AgentCard
              key={agent.id}
              agent={agent}
              connector={connectors.find(c => c.id === agent.llm_connector_id)}
              subtaskConnector={connectors.find(c => c.id === agent.subtask_llm_connector_id)}
              canManage={isAdmin || agent.owner_id === user?.id}
              ownerLabel={ownerLabel(agent)}
              onConfigure={() => setConfiguring(agent)}
              onPolicies={() => setPolicyAgent(agent)}
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

      {policyAgent && (
        <ConnectorPoliciesModal
          agent={policyAgent}
          platformConnectors={platformConnectors}
          policyPresets={policyPresets}
          onSave={handlePolicySave}
          onClose={() => setPolicyAgent(null)}
        />
      )}
    </div>
  )
}
