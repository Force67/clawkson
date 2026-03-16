import { useState, useEffect } from 'react'
import {
  Bot, Plus, Settings2, Trash2, Check, ChevronDown, ChevronRight,
  Loader2, Cpu, Thermometer, Hash, Container,
  BookOpen, Zap, Search, Filter, Share2,
  Shield, HardDrive, Terminal, Database,
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
  type KnowledgeBase,
  type Skill,
  type AgentStatus,
  type AgentPermissions,
  type FilesystemMode,
} from '../lib/api'
import { useAuth } from '../lib/auth'
import styles from './Agents.module.css'

const DEFAULT_PERMISSIONS: AgentPermissions = {
  network: { enabled: false, internet: false, local_network: false, allowed_domains: [] },
  filesystem: { mode: 'read_write', max_workspace_size_mb: null },
  execution: { shell: true, python: true, allowed_runtimes: [], max_execution_time_secs: 300 },
  resources: { max_processes: 256, max_tmp_size_mb: 64, readonly_rootfs: true },
  data_access: { knowledge_bases: true, conversation_history: true },
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
          permissions,
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
                        placeholder="64" type="number" min="1"
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
  canManage: boolean
  onConfigure: () => void
  onDelete: () => void
  onStatusChange: (status: AgentStatus) => void
}

function AgentCard({ agent, connector, canManage, onConfigure, onDelete, onStatusChange }: AgentCardProps) {
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
          <span className={styles.configTag}><Cpu size={10} /> {connector.name}</span>
        ) : (
          <span className={`${styles.configTag} ${styles.configTagMuted}`}><Cpu size={10} /> Default</span>
        )}
        {agent.temperature != null && <span className={styles.configTag}><Thermometer size={10} /> {agent.temperature}</span>}
        {agent.max_tokens != null && <span className={styles.configTag}><Hash size={10} /> {agent.max_tokens}</span>}
        {agent.container_enabled && <span className={styles.configTag}><Container size={10} /> Sandbox</span>}
        {agent.shared && <span className={styles.configTag}><Share2 size={10} /> Shared</span>}
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
        {canManage && (
          <div className={styles.agentCardBtns}>
            <button className={styles.configureBtn} onClick={onConfigure} title="Configure agent">
              <Settings2 size={13} /> Config
            </button>
            <button className={styles.deleteAgentBtn} onClick={onDelete} title="Delete agent">
              <Trash2 size={13} />
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
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBase[]>([])
  const [skills, setSkills] = useState<Skill[]>([])
  const [loading, setLoading] = useState(true)
  const [showCreate, setShowCreate] = useState(false)
  const [configuring, setConfiguring] = useState<Agent | null>(null)
  const [search, setSearch] = useState('')
  const [statusFilter, setStatusFilter] = useState<AgentStatus | 'all'>('all')

  useEffect(() => {
    Promise.all([
      api.agents.list(),
      api.llmConnectors.list(),
      api.knowledge.listBases(),
      api.skills.list(),
    ])
      .then(([agts, conns, kbs, sks]) => {
        setAgents(agts)
        setConnectors(conns)
        setKnowledgeBases(kbs)
        setSkills(sks)
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

  const handleDelete = async (id: string) => {
    await api.agents.delete(id)
    setAgents(prev => prev.filter(a => a.id !== id))
  }

  const handleStatusChange = async (id: string, status: AgentStatus) => {
    const updated = await api.agents.patch(id, { status })
    setAgents(prev => prev.map(a => a.id === updated.id ? updated : a))
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
              canManage={isAdmin || agent.owner_id === user?.id}
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
