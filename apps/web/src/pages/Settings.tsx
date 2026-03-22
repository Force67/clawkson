import { useState, useEffect, useRef, useMemo } from 'react'
import {
  ChevronDown, ChevronRight, Plus, Check, Loader2, X,
  Cloud, Zap, Globe, Star, Key, Trash2, Pencil, Cpu, Palette, Database,
  Users, BarChart3, Timer, Lock, DollarSign,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import { api, type Settings, type LlmConnector, type LlmProviderType, type User, type UserTokenUsage, type ModelPricing, type UsageSummaryWithCost } from '../lib/api'
import { useAuth } from '../lib/auth'
import styles from './Settings.module.css'

// ── LLM Provider Metadata ────────────────────────────────────────

const LLM_PROVIDERS: {
  id: LlmProviderType
  label: string
  icon: React.ReactNode
  description: string
  defaultModel: string
  color: string
}[] = [
  { id: 'open_router', label: 'OpenRouter',      icon: <Zap size={15} />,   description: 'Access 100+ models via a single API', defaultModel: 'openai/gpt-4o-mini', color: '#f97316' },
  { id: 'azure',       label: 'Azure OpenAI',    icon: <Cloud size={15} />, description: 'Microsoft Azure hosted OpenAI',       defaultModel: 'gpt-4o',            color: '#0ea5e9' },
  { id: 'open_ai',     label: 'OpenAI',          icon: <Star size={15} />,  description: 'Direct OpenAI API',                   defaultModel: 'gpt-4o',            color: '#22c55e' },
  { id: 'custom',      label: 'Custom / Ollama', icon: <Globe size={15} />, description: 'Any OpenAI-compatible endpoint',       defaultModel: 'llama3.2',          color: '#8b5cf6' },
]

// ── Grouping logic ───────────────────────────────────────────────

function connectorGroupKey(c: LlmConnector): string {
  if (c.provider_type === 'open_router' || c.provider_type === 'open_ai') return c.provider_type
  return `${c.provider_type}:${c.api_base_url}`
}

interface ConnectorGroup {
  key: string
  providerType: LlmProviderType
  baseUrl: string
  apiKey: string
  connectors: LlmConnector[]
}

function groupConnectors(connectors: LlmConnector[]): ConnectorGroup[] {
  const map = new Map<string, ConnectorGroup>()
  for (const c of connectors) {
    const key = connectorGroupKey(c)
    let group = map.get(key)
    if (!group) {
      group = { key, providerType: c.provider_type, baseUrl: c.api_base_url, apiKey: c.api_key, connectors: [] }
      map.set(key, group)
    }
    group.connectors.push(c)
  }
  return Array.from(map.values())
}

function maskString(s: string): string {
  if (!s) return ''
  if (s.length <= 8) return '\u2022'.repeat(s.length)
  return s.slice(0, 4) + '\u2022'.repeat(Math.min(s.length - 8, 12)) + s.slice(-4)
}

// ── Tab definition ──────────────────────────────────────────────

type SettingsTab = 'inference' | 'embeddings' | 'usage' | 'appearance' | 'advanced'

interface TabDef {
  id: SettingsTab
  label: string
  icon: React.ReactNode
  adminOnly?: boolean
}

const TABS: TabDef[] = [
  { id: 'inference',   label: 'Inference',   icon: <Cpu size={15} /> },
  { id: 'embeddings',  label: 'Embeddings',  icon: <Database size={15} /> },
  { id: 'usage',       label: 'Usage',       icon: <BarChart3 size={15} /> },
  { id: 'appearance',  label: 'Appearance',  icon: <Palette size={15} /> },
  { id: 'advanced',    label: 'Advanced',    icon: <Timer size={15} /> },
]

// ── Inference Form ──────────────────────────────────────────────

interface InferenceFormProps {
  editing?: LlmConnector
  onSave: (c: LlmConnector) => void
  onCancel: () => void
}

interface AzureDeploymentEntry {
  deployment: string
  model: string
}

function InferenceForm({ editing, onSave, onCancel }: InferenceFormProps) {
  const [provider, setProvider] = useState<LlmProviderType>(editing?.provider_type ?? 'open_router')
  const [name, setName] = useState(editing?.name ?? '')
  const [apiKey, setApiKey] = useState('')
  const [model, setModel] = useState(editing?.model ?? LLM_PROVIDERS[0].defaultModel)
  const [baseUrl, setBaseUrl] = useState(editing?.api_base_url ?? '')
  const [azureVersion, setAzureVersion] = useState(editing?.azure_api_version ?? '2024-12-01-preview')
  const [azureDeployments, setAzureDeployments] = useState<AzureDeploymentEntry[]>(
    editing?.provider_type === 'azure'
      ? [{ deployment: editing.azure_deployment ?? '', model: editing.model ?? 'gpt-4o' }]
      : [{ deployment: '', model: 'gpt-4o' }]
  )
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')
  const [testing, setTesting] = useState(false)
  const [testResult, setTestResult] = useState<{ ok: boolean; latency_ms: number; error?: string } | null>(null)

  const isEdit = !!editing
  const providerMeta = LLM_PROVIDERS.find(p => p.id === provider)!
  const isAzureMulti = provider === 'azure'

  const handleProviderChange = (p: LlmProviderType) => {
    setProvider(p)
    const meta = LLM_PROVIDERS.find(x => x.id === p)!
    if (!isEdit) {
      setModel(meta.defaultModel)
      if (!name) setName(meta.label)
    }
  }

  const updateAzureDeployment = (index: number, field: keyof AzureDeploymentEntry, value: string) => {
    setAzureDeployments(prev => prev.map((d, i) => i === index ? { ...d, [field]: value } : d))
  }

  const addAzureDeployment = () => {
    setAzureDeployments(prev => [...prev, { deployment: '', model: 'gpt-4o' }])
  }

  const removeAzureDeployment = (index: number) => {
    setAzureDeployments(prev => prev.filter((_, i) => i !== index))
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    if (!isEdit && !apiKey.trim()) { setError('API key is required.'); return }
    if (provider === 'azure' && !baseUrl.trim()) { setError('Azure resource endpoint is required.'); return }

    if (isAzureMulti) {
      const valid = azureDeployments.filter(d => d.deployment.trim() && d.model.trim())
      if (valid.length === 0) { setError('Add at least one deployment with a name and model.'); return }
      setSubmitting(true)
      try {
        let last: LlmConnector | null = null
        for (let i = 0; i < valid.length; i++) {
          const dep = valid[i]
          const connectorName = name.trim() ? `${name.trim()} / ${dep.deployment.trim()}` : dep.deployment.trim()
          if (isEdit && i === 0) {
            last = await api.llmConnectors.patch(editing.id, {
              name: connectorName, provider_type: 'azure', model: dep.model.trim(),
              ...(apiKey.trim() ? { api_key: apiKey.trim() } : {}),
              api_base_url: baseUrl.trim() || undefined,
              azure_deployment: dep.deployment.trim(),
              azure_api_version: azureVersion.trim() || undefined,
            })
          } else {
            if (!apiKey.trim()) { setError('API key is required for new deployments.'); setSubmitting(false); return }
            last = await api.llmConnectors.create({
              name: connectorName, provider_type: 'azure', api_key: apiKey.trim(), model: dep.model.trim(),
              api_base_url: baseUrl.trim() || undefined,
              azure_deployment: dep.deployment.trim(),
              azure_api_version: azureVersion.trim() || undefined,
            })
          }
        }
        if (last) onSave(last)
      } catch (err) { setError(String(err)) } finally { setSubmitting(false) }
      return
    }

    if (!name.trim() || !model.trim()) { setError('Name and model are required.'); return }
    setSubmitting(true)
    try {
      const c = isEdit
        ? await api.llmConnectors.patch(editing.id, {
            name: name.trim(), provider_type: provider, model: model.trim(),
            ...(apiKey.trim() ? { api_key: apiKey.trim() } : {}),
            api_base_url: baseUrl.trim() || undefined,
          })
        : await api.llmConnectors.create({
            name: name.trim(), provider_type: provider, api_key: apiKey.trim(), model: model.trim(),
            api_base_url: baseUrl.trim() || undefined,
          })
      onSave(c)
    } catch (err) { setError(String(err)) } finally { setSubmitting(false) }
  }

  const handleTestConnection = async () => {
    setTestResult(null); setError('')
    const effectiveKey = apiKey.trim() || (isEdit ? '__existing__' : '')
    if (!effectiveKey) { setError('Enter an API key to test.'); return }
    if (provider === 'azure' && !baseUrl.trim()) { setError('Azure base URL is required to test.'); return }
    const testModel = isAzureMulti ? (azureDeployments[0]?.model.trim() || 'gpt-4o') : model.trim()
    const testDeployment = isAzureMulti ? (azureDeployments[0]?.deployment.trim() || undefined) : undefined
    if (!testModel) { setError('Model is required to test.'); return }
    setTesting(true)
    try {
      const result = await api.llmConnectors.test({
        name: name || 'test', provider_type: provider,
        api_key: apiKey.trim() || (isEdit ? '' : ''), model: testModel,
        api_base_url: baseUrl.trim() || undefined,
        azure_deployment: testDeployment,
        azure_api_version: azureVersion.trim() || undefined,
      })
      setTestResult(result)
    } catch (err) { setTestResult({ ok: false, latency_ms: 0, error: String(err) }) } finally { setTesting(false) }
  }

  return (
    <div className={styles.inferenceAddCard}>
      <h4 className={styles.inferenceAddTitle}>{isEdit ? `Edit \u2014 ${editing.name}` : 'Add LLM Connector'}</h4>
      <div className={styles.providerPills}>
        {LLM_PROVIDERS.map(p => (
          <button key={p.id} type="button"
            className={`${styles.providerPill} ${provider === p.id ? styles.providerPillActive : ''}`}
            style={{ '--pc': p.color } as React.CSSProperties}
            onClick={() => handleProviderChange(p.id)}>
            <span className={styles.providerPillIcon} style={{ color: p.color }}>{p.icon}</span>
            <span>{p.label}</span>
            {provider === p.id && <Check size={11} className={styles.providerPillCheck} />}
          </button>
        ))}
      </div>
      <form onSubmit={handleSubmit}>
        {!isAzureMulti && (
          <div className={styles.formRow}>
            <div className={styles.formGroup}>
              <label className={styles.formLabel}>Connector Name</label>
              <input className={styles.formInput} value={name} onChange={e => setName(e.target.value)} placeholder={providerMeta.label} />
            </div>
            <div className={styles.formGroup}>
              <label className={styles.formLabel}>Model</label>
              <input className={styles.formInput} value={model} onChange={e => setModel(e.target.value)} placeholder={providerMeta.defaultModel} style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }} />
            </div>
          </div>
        )}
        {isAzureMulti && (
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Name Prefix</label>
            <input className={styles.formInput} value={name} onChange={e => setName(e.target.value)} placeholder="Azure OpenAI" />
          </div>
        )}
        <div className={styles.formGroup}>
          <label className={styles.formLabel}>API Key</label>
          <input className={styles.formInput} type="password" value={apiKey} onChange={e => setApiKey(e.target.value)}
            placeholder={isEdit ? 'Leave blank to keep existing key' : provider === 'azure' ? 'Azure resource key' : provider === 'open_router' ? 'sk-or-...' : 'sk-...'} autoComplete="off" />
        </div>
        {isAzureMulti && (
          <>
            <div className={styles.formGroup}>
              <label className={styles.formLabel}>Resource Endpoint</label>
              <input className={styles.formInput} value={baseUrl} onChange={e => setBaseUrl(e.target.value)} placeholder="https://my-resource.openai.azure.com" />
            </div>
            <div className={styles.formGroup}>
              <label className={styles.formLabel}>API Version</label>
              <input className={styles.formInput} value={azureVersion} onChange={e => setAzureVersion(e.target.value)} placeholder="2024-12-01-preview" />
            </div>
            <div className={styles.formGroup}>
              <label className={styles.formLabel}>Deployments</label>
              <div className={styles.azureDeploymentList}>
                {azureDeployments.map((dep, i) => (
                  <div key={i} className={styles.azureDeploymentRow}>
                    <input className={styles.formInput} value={dep.deployment} onChange={e => updateAzureDeployment(i, 'deployment', e.target.value)} placeholder="deployment-name" style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }} />
                    <input className={styles.formInput} value={dep.model} onChange={e => updateAzureDeployment(i, 'model', e.target.value)} placeholder="model (e.g. gpt-4o)" style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }} />
                    {azureDeployments.length > 1 && (
                      <button type="button" className={styles.azureDeploymentRemove} onClick={() => removeAzureDeployment(i)} title="Remove"><X size={14} /></button>
                    )}
                  </div>
                ))}
                <button type="button" className={styles.azureDeploymentAdd} onClick={addAzureDeployment}><Plus size={12} /> Add deployment</button>
              </div>
            </div>
          </>
        )}
        {provider === 'custom' && (
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Base URL</label>
            <input className={styles.formInput} value={baseUrl} onChange={e => setBaseUrl(e.target.value)} placeholder="http://localhost:11434/v1" />
          </div>
        )}
        {error && <p className={styles.errorMsg}>{error}</p>}
        {testResult && (
          <div className={`${styles.testResult} ${testResult.ok ? styles.testResultOk : styles.testResultFail}`}>
            {testResult.ok ? <><Check size={13} /> Connected &middot; {testResult.latency_ms}ms</> : <span>{testResult.error}</span>}
          </div>
        )}
        <div className={styles.formActions}>
          <Button variant="secondary" size="sm" type="button" onClick={onCancel}>Cancel</Button>
          <button type="button" className={styles.testBtn} onClick={handleTestConnection} disabled={testing}>
            {testing ? <><Loader2 size={12} className="spinning" /> Testing...</> : <><Zap size={12} /> Test</>}
          </button>
          <Button variant="primary" size="sm" type="submit" disabled={submitting}>
            {submitting && <Loader2 size={12} className="spinning" />}
            {isEdit ? 'Save' : 'Create'}
          </Button>
        </div>
      </form>
    </div>
  )
}

// ── Inline Access Panel (expands inside connector row) ──────────

interface InlineAccessPanelProps {
  connector: LlmConnector
  onUpdate: (c: LlmConnector) => void
}

function InlineAccessPanel({ connector, onUpdate }: InlineAccessPanelProps) {
  const [sharedWithAll, setSharedWithAll] = useState(connector.shared_with_all)
  const [allUsers, setAllUsers] = useState<User[]>([])
  const [grantedIds, setGrantedIds] = useState<Set<string>>(new Set())
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    Promise.all([
      api.admin.listUsers(),
      api.admin.getConnectorAccess(connector.id),
    ]).then(([users, access]) => {
      setAllUsers(users)
      setGrantedIds(new Set(access.map(a => a.user_id)))
    }).finally(() => setLoading(false))
  }, [connector.id])

  const handleToggleShared = async () => {
    const next = !sharedWithAll
    setSharedWithAll(next)
    const updated = await api.llmConnectors.patch(connector.id, { shared_with_all: next })
    onUpdate(updated)
  }

  const handleToggleUser = async (userId: string) => {
    const next = new Set(grantedIds)
    if (next.has(userId)) next.delete(userId)
    else next.add(userId)
    setGrantedIds(next)
    await api.admin.setConnectorAccess(connector.id, Array.from(next))
  }

  const grantedCount = grantedIds.size

  return (
    <div className={styles.accessInline}>
      <div className={styles.accessInlineHeader}>
        <span className={styles.accessInlineTitle}>Access Control</span>
      </div>

      <label className={styles.accessToggle}>
        <input type="checkbox" checked={sharedWithAll} onChange={handleToggleShared} />
        <span>Available to all users</span>
      </label>

      {!sharedWithAll && (
        <>
          {loading ? (
            <div className={styles.loadingRow}><Loader2 size={14} className="spinning" /> Loading users...</div>
          ) : (
            <div className={styles.accessUserGrid}>
              {allUsers.map(u => (
                <label key={u.id} className={styles.accessUserRow}>
                  <input type="checkbox" checked={grantedIds.has(u.id)} onChange={() => handleToggleUser(u.id)} />
                  <span className={styles.accessUserName}>{u.display_name}</span>
                  <span className={styles.accessUserEmail}>{u.email}</span>
                </label>
              ))}
            </div>
          )}
          <p className={styles.accessHint}>
            {grantedCount === 0
              ? 'No users selected — nobody can use this connector.'
              : `${grantedCount} user${grantedCount !== 1 ? 's' : ''} granted access. Users without access will see an error when chatting with agents that use this connector.`}
          </p>
        </>
      )}
    </div>
  )
}

// ── Model Row (inside a provider group) ─────────────────────────

interface ModelRowProps {
  connector: LlmConnector
  isDefault: boolean
  isEditing: boolean
  isAdmin: boolean
  onSetDefault: () => void
  onEdit: () => void
  onDelete: () => void
  onUpdate: (c: LlmConnector) => void
}

function ModelRow({ connector: c, isDefault, isEditing, isAdmin, onSetDefault, onEdit, onDelete, onUpdate }: ModelRowProps) {
  const [expanded, setExpanded] = useState(false)

  return (
    <div className={`${styles.modelRow} ${isDefault ? styles.modelRowDefault : ''} ${isEditing ? styles.modelRowEditing : ''}`}>
      <div className={styles.modelRowMain}>
        <div className={styles.modelRowLeft}>
          <span className={styles.modelName}>{c.model}</span>
          {c.azure_deployment && (
            <span className={styles.modelDeployment}>{c.azure_deployment}</span>
          )}
        </div>
        <div className={styles.modelRowActions}>
          {isDefault
            ? <span className={styles.defaultBadge}><Check size={10} /> Default</span>
            : <button className={styles.setDefaultBtn} onClick={onSetDefault}>Set default</button>}
          {isAdmin && (
            <button
              className={`${styles.accessIndicator} ${c.shared_with_all ? styles.accessIndicatorOpen : styles.accessIndicatorRestricted}`}
              onClick={() => setExpanded(prev => !prev)}
              title={c.shared_with_all ? 'All users' : 'Restricted access'}
            >
              {c.shared_with_all ? <Users size={11} /> : <Lock size={11} />}
              {c.shared_with_all ? 'All' : 'Restricted'}
            </button>
          )}
          <button className={styles.editBtn} onClick={onEdit} title="Edit"><Pencil size={14} /></button>
          <button className={styles.deleteBtn} onClick={onDelete} title="Delete"><Trash2 size={14} /></button>
        </div>
      </div>
      {expanded && isAdmin && (
        <InlineAccessPanel
          connector={c}
          onUpdate={(updated) => {
            onUpdate(updated)
            if (updated.shared_with_all) setExpanded(false)
          }}
        />
      )}
    </div>
  )
}

// ── Inline "Add Model" form (within a provider group) ───────────

interface AddModelFormProps {
  group: ConnectorGroup
  onSave: (c: LlmConnector) => void
  onCancel: () => void
}

function AddModelForm({ group, onSave, onCancel }: AddModelFormProps) {
  const providerMeta = LLM_PROVIDERS.find(p => p.id === group.providerType)
  const isAzure = group.providerType === 'azure'
  const [model, setModel] = useState(providerMeta?.defaultModel ?? '')
  const [deployment, setDeployment] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')

  const template = group.connectors[0]
  const [apiKey, setApiKey] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    if (!model.trim()) { setError('Model name is required.'); return }
    if (isAzure && !deployment.trim()) { setError('Deployment name is required for Azure.'); return }
    if (!apiKey.trim()) { setError('API key is required.'); return }
    setSubmitting(true)
    try {
      const connName = isAzure
        ? `${template.name.split(' / ')[0]} / ${deployment.trim()}`
        : model.trim()
      const c = await api.llmConnectors.create({
        name: connName,
        provider_type: group.providerType,
        api_key: apiKey.trim(),
        model: model.trim(),
        api_base_url: template.api_base_url || undefined,
        azure_deployment: isAzure ? deployment.trim() : undefined,
        azure_api_version: isAzure ? (template.azure_api_version ?? '2024-12-01-preview') : undefined,
      })
      onSave(c)
    } catch (err) { setError(String(err)) } finally { setSubmitting(false) }
  }

  return (
    <div className={styles.addModelForm}>
      <form onSubmit={handleSubmit}>
        <div className={styles.addModelFields}>
          <div className={styles.formGroup} style={{ marginBottom: 0 }}>
            <label className={styles.formLabel}>Model</label>
            <input className={styles.formInput} value={model} onChange={e => setModel(e.target.value)}
              placeholder={providerMeta?.defaultModel ?? 'model-name'}
              style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }} />
          </div>
          {isAzure && (
            <div className={styles.formGroup} style={{ marginBottom: 0 }}>
              <label className={styles.formLabel}>Deployment</label>
              <input className={styles.formInput} value={deployment} onChange={e => setDeployment(e.target.value)}
                placeholder="deployment-name" style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }} />
            </div>
          )}
          <div className={styles.formGroup} style={{ marginBottom: 0 }}>
            <label className={styles.formLabel}>API Key</label>
            <input className={styles.formInput} type="password" value={apiKey} onChange={e => setApiKey(e.target.value)}
              placeholder="Same key as provider" autoComplete="off" />
          </div>
        </div>
        {error && <p className={styles.errorMsg} style={{ marginTop: 8 }}>{error}</p>}
        <div className={styles.formActions} style={{ paddingTop: 4 }}>
          <Button variant="secondary" size="sm" type="button" onClick={onCancel}>Cancel</Button>
          <Button variant="primary" size="sm" type="submit" disabled={submitting}>
            {submitting && <Loader2 size={12} className="spinning" />}
            Add Model
          </Button>
        </div>
      </form>
    </div>
  )
}

// ── Provider Group Card ─────────────────────────────────────────

interface ProviderGroupCardProps {
  group: ConnectorGroup
  defaultConnectorId: string | undefined
  editingId: string | undefined
  isAdmin: boolean
  onSetDefault: (id: string) => void
  onEdit: (c: LlmConnector) => void
  onDelete: (id: string) => void
  onUpdate: (c: LlmConnector) => void
  onModelAdded: () => void
}

function ProviderGroupCard({ group, defaultConnectorId, editingId, isAdmin, onSetDefault, onEdit, onDelete, onUpdate, onModelAdded }: ProviderGroupCardProps) {
  const [collapsed, setCollapsed] = useState(false)
  const [addingModel, setAddingModel] = useState(false)
  const meta = LLM_PROVIDERS.find(p => p.id === group.providerType)
  const modelCount = group.connectors.length

  return (
    <div className={styles.providerGroup}>
      <div className={styles.providerGroupHeader} onClick={() => setCollapsed(prev => !prev)}>
        <div className={styles.providerGroupLeft}>
          <div className={styles.inferenceIcon} style={{ color: meta?.color ?? 'var(--accent-text)', background: `${meta?.color ?? 'var(--accent)'}18` }}>
            {meta?.icon ?? <Key size={14} />}
          </div>
          <div>
            <div className={styles.providerGroupName}>{meta?.label ?? group.providerType}</div>
            <div className={styles.providerGroupMeta}>
              {group.baseUrl && (
                <><span className={styles.maskedKey}>{maskString(group.baseUrl)}</span><span className={styles.sep}>&middot;</span></>
              )}
              {group.apiKey && (
                <><span className={styles.maskedKey}>{group.apiKey}</span><span className={styles.sep}>&middot;</span></>
              )}
              <span>{modelCount} model{modelCount !== 1 ? 's' : ''}</span>
            </div>
          </div>
        </div>
        <div className={styles.providerGroupRight}>
          <button className={styles.addModelBtn} onClick={e => { e.stopPropagation(); setAddingModel(true); setCollapsed(false) }}
            title="Add model">
            <Plus size={12} /> Model
          </button>
          {collapsed ? <ChevronRight size={16} className={styles.providerGroupChevron} /> : <ChevronDown size={16} className={styles.providerGroupChevron} />}
        </div>
      </div>
      {!collapsed && (
        <div className={styles.providerGroupBody}>
          {group.connectors.map(c => (
            <ModelRow
              key={c.id}
              connector={c}
              isDefault={c.id === defaultConnectorId}
              isEditing={c.id === editingId}
              isAdmin={isAdmin}
              onSetDefault={() => onSetDefault(c.id)}
              onEdit={() => onEdit(c)}
              onDelete={() => onDelete(c.id)}
              onUpdate={onUpdate}
            />
          ))}
          {addingModel && (
            <AddModelForm
              group={group}
              onSave={() => { setAddingModel(false); onModelAdded() }}
              onCancel={() => setAddingModel(false)}
            />
          )}
        </div>
      )}
    </div>
  )
}

// ── Inference Tab (grouped providers) ────────────────────────────

interface InferenceTabProps {
  llmConnectors: LlmConnector[]
  settings: Settings | null
  loading: boolean
  isAdmin: boolean
  formState: { mode: 'add' } | { mode: 'edit'; connector: LlmConnector } | null
  onFormStateChange: (s: { mode: 'add' } | { mode: 'edit'; connector: LlmConnector } | null) => void
  onSave: (c: LlmConnector) => void
  onSetDefault: (id: string) => void
  onDelete: (id: string) => void
  onConnectorsChange: (updater: (prev: LlmConnector[]) => LlmConnector[]) => void
  onRefresh: () => Promise<void>
}

function InferenceTab({ llmConnectors, settings, loading, isAdmin, formState, onFormStateChange, onSave, onSetDefault, onDelete, onConnectorsChange, onRefresh }: InferenceTabProps) {
  const groups = useMemo(() => groupConnectors(llmConnectors), [llmConnectors])

  return (
    <div className={styles.tabContent}>
      <div className={styles.tabContentHeader}>
        <div>
          <h3 className={styles.tabContentTitle}>LLM Connectors</h3>
          <p className={styles.tabContentDesc}>Configure inference providers for your agents.</p>
        </div>
        {!formState && (
          <Button size="sm" onClick={() => onFormStateChange({ mode: 'add' })}><Plus size={13} /> Add Provider</Button>
        )}
      </div>

      {formState && (
        <InferenceForm
          editing={formState.mode === 'edit' ? formState.connector : undefined}
          onSave={onSave}
          onCancel={() => onFormStateChange(null)}
        />
      )}

      {loading ? (
        <div className={styles.loadingRow}><Loader2 size={14} className="spinning" /> Loading...</div>
      ) : llmConnectors.length === 0 && !formState ? (
        <div className={styles.emptyState}>
          <Key size={28} strokeWidth={1} />
          <p>No LLM connectors configured.</p>
          <Button size="sm" onClick={() => onFormStateChange({ mode: 'add' })}><Plus size={13} /> Add Provider</Button>
        </div>
      ) : (
        <div className={styles.inferenceList}>
          {groups.map(g => (
            <ProviderGroupCard
              key={g.key}
              group={g}
              defaultConnectorId={settings?.default_llm_connector_id}
              editingId={formState?.mode === 'edit' ? formState.connector.id : undefined}
              isAdmin={isAdmin}
              onSetDefault={onSetDefault}
              onEdit={c => onFormStateChange({ mode: 'edit', connector: c })}
              onDelete={onDelete}
              onUpdate={updated => onConnectorsChange(prev => prev.map(x => x.id === updated.id ? updated : x))}
              onModelAdded={onRefresh}
            />
          ))}
        </div>
      )}
    </div>
  )
}

// ── Embeddings Panel ────────────────────────────────────────────

interface EmbeddingsPanelProps {
  settings: Settings | null
  onUpdate: (s: Settings) => void
  llmConnectors: LlmConnector[]
}

function EmbeddingsPanel({ settings, onUpdate, llmConnectors }: EmbeddingsPanelProps) {
  const [baseUrl, setBaseUrl] = useState('')
  const [model, setModel] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [saving, setSaving] = useState(false)
  const debounceRef = useRef<ReturnType<typeof setTimeout>>(undefined)
  const pendingRef = useRef<Record<string, string>>({})

  useEffect(() => {
    if (settings) {
      setBaseUrl(settings.embedding_api_base_url ?? '')
      setModel(settings.embedding_model ?? '')
    }
  }, [settings?.embedding_api_base_url, settings?.embedding_model])

  useEffect(() => {
    return () => {
      clearTimeout(debounceRef.current)
      const pending = pendingRef.current
      if (Object.keys(pending).length > 0) { api.settings.patch(pending).catch(() => {}) }
    }
  }, [])

  const debouncedSave = (patch: Record<string, string>) => {
    pendingRef.current = { ...pendingRef.current, ...patch }
    clearTimeout(debounceRef.current)
    setSaving(true)
    debounceRef.current = setTimeout(async () => {
      const toSave = { ...pendingRef.current }
      pendingRef.current = {}
      try { const s = await api.settings.patch(toSave); onUpdate(s) }
      catch (e) { console.error('Failed to save embedding settings:', e) }
      finally { setSaving(false) }
    }, 600)
  }

  return (
    <div className={styles.tabContent}>
      <div className={styles.tabContentHeader}>
        <div>
          <h3 className={styles.tabContentTitle}>Embeddings & ETL</h3>
          <p className={styles.tabContentDesc}>Configure the embedding provider and optional semantic chunking for Knowledge Base ingestion.</p>
        </div>
      </div>
      <Card>
        <div className={styles.formGroup}>
          <label className={styles.formLabel}>Embedding API Base URL</label>
          <input className={styles.formInput} value={baseUrl} placeholder="http://localhost:11434/v1" style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}
            onChange={e => { setBaseUrl(e.target.value); const val = e.target.value.trim(); if (val) debouncedSave({ embedding_api_base_url: val }) }} />
          <p className={styles.formHint}>Any OpenAI-compatible <code>/v1/embeddings</code> endpoint.</p>
        </div>
        <div className={styles.formGroup}>
          <label className={styles.formLabel}>Embedding API Key</label>
          <input className={styles.formInput} type="password" value={apiKey} placeholder="Leave blank to keep existing key" autoComplete="off"
            onChange={e => { setApiKey(e.target.value); const val = e.target.value.trim(); if (val) debouncedSave({ embedding_api_key: val }) }}
            onBlur={() => { if (apiKey.trim()) setApiKey('') }} />
          {settings?.embedding_api_key && <p className={styles.formHint}>Current key: <code>{settings.embedding_api_key}</code></p>}
        </div>
        <div className={styles.formGroup}>
          <label className={styles.formLabel}>
            Embedding Model
            {saving && <Loader2 size={11} className="spinning" style={{ marginLeft: 6, display: 'inline-block' }} />}
          </label>
          <input className={styles.formInput} value={model} placeholder="qwen3-embedding:8b" style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}
            onChange={e => { setModel(e.target.value); const val = e.target.value.trim(); if (val) debouncedSave({ embedding_model: val }) }} />
          <p className={styles.formHint}>The model used to generate vector embeddings for knowledge base entries.</p>
        </div>
      </Card>
      <Card>
        <div className={styles.formGroup}>
          <label className={styles.formLabel}>Semantic Chunking Model</label>
          <div className={styles.selectWrap}>
            <select className={styles.select} value={settings?.etl_llm_connector_id ?? ''}
              onChange={async e => { const s = await api.settings.patch({ etl_llm_connector_id: e.target.value === '' ? null : e.target.value }); onUpdate(s) }}>
              <option value="">None (heuristic chunking)</option>
              {llmConnectors.map(c => {
                const meta = LLM_PROVIDERS.find(p => p.id === c.provider_type)
                return <option key={c.id} value={c.id}>{c.name} — {meta?.label ?? c.provider_type} / {c.model}</option>
              })}
            </select>
            <ChevronDown size={14} className={styles.selectChevron} />
          </div>
          {settings?.etl_llm_connector_id && (
            <p className={styles.formHint}>The model locates semantically coherent chunk boundaries when documents exceed the maximum chunk size.</p>
          )}
        </div>
      </Card>
    </div>
  )
}

// ── Usage Stats Panel ───────────────────────────────────────────

function UsagePanel() {
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'
  const [adminUsage, setAdminUsage] = useState<UserTokenUsage[]>([])
  const [myUsage, setMyUsage] = useState<UsageSummaryWithCost[]>([])
  const [loading, setLoading] = useState(true)
  const [range, setRange] = useState('30d')
  const [pricing, setPricing] = useState<ModelPricing[]>([])
  const [pricingModel, setPricingModel] = useState('')
  const [pricingPrompt, setPricingPrompt] = useState('')
  const [pricingCompletion, setPricingCompletion] = useState('')

  useEffect(() => {
    setLoading(true)
    const since = range === 'all' ? undefined : range
    if (isAdmin) {
      Promise.all([
        api.admin.getUsage(since).then(setAdminUsage),
        api.admin.listPricing().then(setPricing),
      ]).finally(() => setLoading(false))
    } else {
      api.usage.me(since).then(setMyUsage).finally(() => setLoading(false))
    }
  }, [range, isAdmin])

  const formatTokens = (n: number) => {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`
    return String(n)
  }

  const formatCost = (n: number) => n < 0.01 ? '<$0.01' : `$${n.toFixed(2)}`

  const handleUpsertPricing = async () => {
    if (!pricingModel) return
    const p = parseFloat(pricingPrompt) || 0
    const c = parseFloat(pricingCompletion) || 0
    const entry = await api.admin.upsertPricing(pricingModel, p, c)
    setPricing(prev => {
      const filtered = prev.filter(x => x.model !== entry.model)
      return [...filtered, entry].sort((a, b) => a.model.localeCompare(b.model))
    })
    setPricingModel('')
    setPricingPrompt('')
    setPricingCompletion('')
  }

  const handleDeletePricing = async (id: string) => {
    await api.admin.deletePricing(id)
    setPricing(prev => prev.filter(x => x.id !== id))
  }

  // Admin view: per-user breakdown
  const totalTokens = adminUsage.reduce((sum, u) => sum + u.models.reduce((s, m) => s + m.total_tokens, 0), 0)
  const totalUsers = adminUsage.length

  // User view: own usage with cost
  const myTotalTokens = myUsage.reduce((s, m) => s + m.total_tokens, 0)
  const myTotalCost = myUsage.reduce((s, m) => s + m.estimated_cost_usd, 0)

  return (
    <div className={styles.tabContent}>
      <div className={styles.tabContentHeader}>
        <div>
          <h3 className={styles.tabContentTitle}>Token Usage</h3>
          <p className={styles.tabContentDesc}>
            {isAdmin ? 'Per-user LLM token consumption across all connectors.' : 'Your LLM token usage and estimated costs.'}
          </p>
        </div>
        <div className={styles.selectWrap} style={{ width: 'auto' }}>
          <select className={styles.select} value={range} onChange={e => setRange(e.target.value)}>
            <option value="24h">Last 24 hours</option>
            <option value="7d">Last 7 days</option>
            <option value="30d">Last 30 days</option>
            <option value="all">All time</option>
          </select>
          <ChevronDown size={14} className={styles.selectChevron} />
        </div>
      </div>

      {!loading && isAdmin && adminUsage.length > 0 && (
        <div className={styles.usageSummaryRow}>
          <div className={styles.usageStat}>
            <span className={styles.usageStatValue}>{formatTokens(totalTokens)}</span>
            <span className={styles.usageStatLabel}>total tokens</span>
          </div>
          <div className={styles.usageStat}>
            <span className={styles.usageStatValue}>{totalUsers}</span>
            <span className={styles.usageStatLabel}>{totalUsers === 1 ? 'user' : 'users'}</span>
          </div>
        </div>
      )}

      {!loading && !isAdmin && myUsage.length > 0 && (
        <div className={styles.usageSummaryRow}>
          <div className={styles.usageStat}>
            <span className={styles.usageStatValue}>{formatTokens(myTotalTokens)}</span>
            <span className={styles.usageStatLabel}>total tokens</span>
          </div>
          <div className={styles.usageStat}>
            <span className={styles.usageStatValue}>{formatCost(myTotalCost)}</span>
            <span className={styles.usageStatLabel}>estimated cost</span>
          </div>
        </div>
      )}

      <Card>
        {loading ? (
          <div className={styles.loadingRow}><Loader2 size={14} className="spinning" /> Loading...</div>
        ) : isAdmin ? (
          adminUsage.length === 0 ? (
            <div className={styles.emptyState}>
              <BarChart3 size={24} strokeWidth={1} />
              <p>No token usage recorded yet.</p>
            </div>
          ) : (
            <div className={styles.usageTable}>
              <div className={styles.usageTableHeader}>
                <span>User</span><span>Model</span>
                <span className={styles.usageNum}>Prompt</span>
                <span className={styles.usageNum}>Completion</span>
                <span className={styles.usageNum}>Total</span>
              </div>
              {adminUsage.map(u => u.models.map((m, i) => (
                <div key={`${u.user_id}-${m.model}`} className={styles.usageTableRow}>
                  <span className={i === 0 ? styles.usageUser : ''}>{i === 0 ? u.display_name : ''}</span>
                  <span className={styles.usageModel}>{m.model}</span>
                  <span className={styles.usageNum}>{formatTokens(m.prompt_tokens)}</span>
                  <span className={styles.usageNum}>{formatTokens(m.completion_tokens)}</span>
                  <span className={styles.usageNum}>{formatTokens(m.total_tokens)}</span>
                </div>
              )))}
            </div>
          )
        ) : (
          myUsage.length === 0 ? (
            <div className={styles.emptyState}>
              <BarChart3 size={24} strokeWidth={1} />
              <p>No token usage recorded yet.</p>
            </div>
          ) : (
            <div className={styles.usageTable}>
              <div className={styles.usageTableHeader}>
                <span>Model</span>
                <span className={styles.usageNum}>Prompt</span>
                <span className={styles.usageNum}>Completion</span>
                <span className={styles.usageNum}>Total</span>
                <span className={styles.usageNum}>Cost</span>
              </div>
              {myUsage.map(m => (
                <div key={m.model} className={styles.usageTableRow}>
                  <span className={styles.usageModel}>{m.model}</span>
                  <span className={styles.usageNum}>{formatTokens(m.prompt_tokens)}</span>
                  <span className={styles.usageNum}>{formatTokens(m.completion_tokens)}</span>
                  <span className={styles.usageNum}>{formatTokens(m.total_tokens)}</span>
                  <span className={styles.usageNum}>{formatCost(m.estimated_cost_usd)}</span>
                </div>
              ))}
            </div>
          )
        )}
      </Card>

      {isAdmin && (
        <>
          <div className={styles.tabContentHeader} style={{ marginTop: 24 }}>
            <div>
              <h3 className={styles.tabContentTitle}><DollarSign size={15} style={{ marginRight: 6, verticalAlign: -2 }} />Model Pricing</h3>
              <p className={styles.tabContentDesc}>Set cost per million tokens for each model to enable cost estimation.</p>
            </div>
          </div>
          <Card>
            <div className={styles.usageTable}>
              <div className={styles.usageTableHeader}>
                <span>Model</span>
                <span className={styles.usageNum}>Prompt $/M</span>
                <span className={styles.usageNum}>Completion $/M</span>
                <span className={styles.usageNum}></span>
              </div>
              {pricing.map(p => (
                <div key={p.id} className={styles.usageTableRow}>
                  <span className={styles.usageModel}>{p.model}</span>
                  <span className={styles.usageNum}>${p.prompt_cost_per_million.toFixed(2)}</span>
                  <span className={styles.usageNum}>${p.completion_cost_per_million.toFixed(2)}</span>
                  <span className={styles.usageNum}>
                    <button onClick={() => handleDeletePricing(p.id)} className={styles.iconBtn} title="Delete"><Trash2 size={13} /></button>
                  </span>
                </div>
              ))}
              <div className={styles.usageTableRow} style={{ borderTop: '1px solid var(--border)' }}>
                <input
                  className={styles.input}
                  placeholder="Model name"
                  value={pricingModel}
                  onChange={e => setPricingModel(e.target.value)}
                  style={{ fontSize: 12 }}
                />
                <input
                  className={styles.input}
                  placeholder="0.00"
                  type="number"
                  step="0.01"
                  value={pricingPrompt}
                  onChange={e => setPricingPrompt(e.target.value)}
                  style={{ fontSize: 12, textAlign: 'right' }}
                />
                <input
                  className={styles.input}
                  placeholder="0.00"
                  type="number"
                  step="0.01"
                  value={pricingCompletion}
                  onChange={e => setPricingCompletion(e.target.value)}
                  style={{ fontSize: 12, textAlign: 'right' }}
                />
                <span className={styles.usageNum}>
                  <button onClick={handleUpsertPricing} className={styles.iconBtn} title="Add" disabled={!pricingModel}><Plus size={13} /></button>
                </span>
              </div>
            </div>
          </Card>
        </>
      )}
    </div>
  )
}

// ── Theme helper ────────────────────────────────────────────────

function applyTheme(theme: string) {
  document.documentElement.setAttribute('data-theme', theme)
}

// ── Settings Page ───────────────────────────────────────────────

export function SettingsPage() {
  const { user } = useAuth()
  const isAdmin = user?.role === 'admin'
  const [activeTab, setActiveTab] = useState<SettingsTab>('inference')
  const [settings, setSettings] = useState<Settings | null>(null)
  const [llmConnectors, setLlmConnectors] = useState<LlmConnector[]>([])
  const [loading, setLoading] = useState(true)
  const [formState, setFormState] = useState<{ mode: 'add' } | { mode: 'edit'; connector: LlmConnector } | null>(null)

  const visibleTabs = TABS.filter(t => !t.adminOnly || isAdmin)

  useEffect(() => {
    Promise.all([api.settings.get(), api.llmConnectors.list()])
      .then(([s, conns]) => { setSettings(s); setLlmConnectors(conns); applyTheme(s.theme) })
      .finally(() => setLoading(false))
  }, [])

  const handleSave = async (_c: LlmConnector) => {
    const isEdit = formState?.mode === 'edit'
    const conns = await api.llmConnectors.list()
    setLlmConnectors(conns)
    if (!isEdit && !settings?.default_llm_connector_id && conns.length > 0) {
      const s = await api.settings.patch({ default_llm_connector_id: conns[0].id })
      setSettings(s)
    }
    setFormState(null)
  }

  const handleSetDefault = async (id: string) => {
    const s = await api.settings.patch({ default_llm_connector_id: id })
    setSettings(s)
  }

  const handleDelete = async (id: string) => {
    await api.llmConnectors.delete(id)
    setLlmConnectors(prev => prev.filter(c => c.id !== id))
    if (settings?.default_llm_connector_id === id) {
      const remaining = llmConnectors.filter(c => c.id !== id)
      const s = await api.settings.patch({ default_llm_connector_id: remaining[0]?.id ?? undefined })
      setSettings(s)
    }
  }

  return (
    <div className="fade-in">
      <PageHeader title="Settings" description="Manage inference providers, embeddings, usage, and application preferences." />

      <div className={styles.settingsLayout}>
        <nav className={styles.settingsNav}>
          {visibleTabs.map(tab => (
            <button key={tab.id}
              className={`${styles.navItem} ${activeTab === tab.id ? styles.navItemActive : ''}`}
              onClick={() => setActiveTab(tab.id)}>
              <span className={styles.navItemIcon}>{tab.icon}</span>
              <span>{tab.label}</span>
            </button>
          ))}
        </nav>

        <div className={styles.settingsContent}>
          {activeTab === 'inference' && (
            <InferenceTab
              llmConnectors={llmConnectors}
              settings={settings}
              loading={loading}
              isAdmin={isAdmin}
              formState={formState}
              onFormStateChange={setFormState}
              onSave={handleSave}
              onSetDefault={handleSetDefault}
              onDelete={handleDelete}
              onConnectorsChange={setLlmConnectors}
              onRefresh={async () => { const conns = await api.llmConnectors.list(); setLlmConnectors(conns) }}
            />
          )}

          {activeTab === 'embeddings' && <EmbeddingsPanel settings={settings} onUpdate={setSettings} llmConnectors={llmConnectors} />}
          {activeTab === 'usage' && isAdmin && <UsagePanel />}

          {activeTab === 'appearance' && (
            <div className={styles.tabContent}>
              <div className={styles.tabContentHeader}>
                <h3 className={styles.tabContentTitle}>Appearance</h3>
                <p className={styles.tabContentDesc}>Interface theme and display preferences.</p>
              </div>
              <Card>
                <div className={styles.formGroup}>
                  <label className={styles.formLabel}>Theme</label>
                  <div className={styles.selectWrap}>
                    <select className={styles.select} value={settings?.theme ?? 'dark'}
                      onChange={async e => { applyTheme(e.target.value); const s = await api.settings.patch({ theme: e.target.value }); setSettings(s) }}>
                      <option value="dark">Dark</option>
                      <option value="light">Light</option>
                      <option value="system">System</option>
                    </select>
                    <ChevronDown size={14} className={styles.selectChevron} />
                  </div>
                </div>
              </Card>
            </div>
          )}

          {activeTab === 'advanced' && (
            <div className={styles.tabContent}>
              <div className={styles.tabContentHeader}>
                <h3 className={styles.tabContentTitle}>Advanced</h3>
                <p className={styles.tabContentDesc}>Low-level tuning for LLM request behaviour.</p>
              </div>
              <Card>
                <div className={styles.formGroup}>
                  <label className={styles.formLabel}>LLM Request Timeout (seconds)</label>
                  <input className={styles.formInput} type="number" min={10} max={600} style={{ width: 120 }}
                    value={settings?.llm_request_timeout_secs ?? 120}
                    onChange={async e => { const val = parseInt(e.target.value, 10); if (isNaN(val)) return; const s = await api.settings.patch({ llm_request_timeout_secs: val }); setSettings(s) }} />
                  <p className={styles.formHint}>Maximum time (10 &ndash; 600 s) to wait for a single LLM response. Default: 120 s.</p>
                </div>
              </Card>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
