import { useEffect, useState } from 'react'
import {
  Plus, Mail, MessageSquare, ToggleLeft, ToggleRight, Trash2, Send,
  Key, Check, Loader2, Cloud, Zap, Globe, Star, Cpu, Plug, Pencil,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import { api } from '../lib/api'
import type { Connector, ConnectorType, LlmConnector, LlmProviderType, Settings } from '../lib/api'
import styles from './Connectors.module.css'

// ── Tab type ──────────────────────────────────────────────────────

type Tab = 'platform' | 'inference'

// ══════════════════════════════════════════════════════════════════
// PLATFORM CONNECTORS
// ══════════════════════════════════════════════════════════════════

const PLATFORM_META: Record<ConnectorType, { label: string; icon: typeof MessageSquare; description: string }> = {
  telegram: { label: 'Telegram', icon: Send, description: 'Receive and send messages through Telegram Bot API.' },
  gmail: { label: 'Gmail', icon: Mail, description: 'Read and send emails via Gmail API.' },
  slack: { label: 'Slack', icon: MessageSquare, description: 'Integrate with Slack for team notifications.' },
  custom: { label: 'Custom', icon: MessageSquare, description: 'A custom connector.' },
}

interface AddPlatformModalProps {
  onClose: () => void
  onCreated: (c: Connector) => void
}

function AddPlatformModal({ onClose, onCreated }: AddPlatformModalProps) {
  const [type, setType] = useState<ConnectorType>('telegram')
  const [name, setName] = useState('My Telegram Bot')
  const [botToken, setBotToken] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setError(null)
    if (!name.trim()) { setError('Name is required.'); return }
    if (type === 'telegram' && !botToken.trim()) { setError('Bot token is required.'); return }
    setSaving(true)
    try {
      const connector = await api.connectors.create({
        name: name.trim(),
        connector_type: type,
        config: type === 'telegram' ? { bot_token: botToken.trim() } : {},
      })
      onCreated(connector)
    } catch (err) {
      setError(String(err))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className={styles.modalOverlay} onClick={onClose}>
      <div className={styles.modal} onClick={e => e.stopPropagation()}>
        <h2 className={styles.modalTitle}>Add Platform Connector</h2>
        <form onSubmit={handleSubmit} className={styles.form}>
          <label className={styles.fieldLabel}>
            Type
            <select className={styles.select} value={type} onChange={e => setType(e.target.value as ConnectorType)}>
              <option value="telegram">Telegram</option>
              <option value="gmail">Gmail</option>
              <option value="slack">Slack</option>
              <option value="custom">Custom</option>
            </select>
          </label>
          <label className={styles.fieldLabel}>
            Name
            <input className={styles.input} value={name} onChange={e => setName(e.target.value)} placeholder="e.g. My Bot" />
          </label>
          {type === 'telegram' && (
            <label className={styles.fieldLabel}>
              Bot Token
              <input className={styles.input} value={botToken} onChange={e => setBotToken(e.target.value)} placeholder="123456789:ABCdef…" autoComplete="off" />
              <span className={styles.hint}>Get from <a href="https://t.me/BotFather" target="_blank" rel="noreferrer">@BotFather</a> on Telegram.</span>
            </label>
          )}
          {error && <p className={styles.errorMsg}>{error}</p>}
          <div className={styles.modalActions}>
            <Button variant="secondary" type="button" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={saving}>{saving ? 'Adding…' : 'Add Connector'}</Button>
          </div>
        </form>
      </div>
    </div>
  )
}

interface PlatformSectionProps {
  connectors: Connector[]
  loading: boolean
  onToggle: (id: string, enabled: boolean) => void
  onDelete: (id: string) => void
  onAdd: () => void
}

function PlatformSection({ connectors, loading, onToggle, onDelete, onAdd }: PlatformSectionProps) {
  if (loading) return <div className={styles.loadingRow}><Loader2 size={15} className={styles.spinning} /> Loading…</div>

  return (
    <div className={styles.sectionBody}>
      {connectors.length === 0 ? (
        <div className={styles.emptyState}>
          <Plug size={28} strokeWidth={1.2} />
          <p>No platform connectors yet.</p>
          <Button size="sm" onClick={onAdd}><Plus size={14} /> Add Connector</Button>
        </div>
      ) : (
        <div className={styles.platformGrid}>
          {connectors.map(conn => {
            const meta = PLATFORM_META[conn.connector_type] ?? PLATFORM_META.custom
            const Icon = meta.icon
            return (
              <Card key={conn.id}>
                <div className={styles.connHeader}>
                  <div className={styles.connIcon}><Icon size={20} strokeWidth={1.5} /></div>
                  <div className={styles.connActions}>
                    <button className={styles.connToggle} onClick={() => onToggle(conn.id, conn.enabled)} title={conn.enabled ? 'Disable' : 'Enable'}>
                      {conn.enabled
                        ? <ToggleRight size={24} className={styles.toggleOn} />
                        : <ToggleLeft size={24} className={styles.toggleOff} />}
                    </button>
                    <button className={styles.deleteBtn} onClick={() => onDelete(conn.id)} title="Delete"><Trash2 size={15} /></button>
                  </div>
                </div>
                <h3 className={styles.connName}>{conn.name}</h3>
                <p className={styles.connDesc}>{meta.description}</p>
                <div className={styles.connStatus} data-connected={conn.enabled}>
                  <span className={styles.connDot} />
                  {conn.enabled ? 'Enabled' : 'Disabled'}
                </div>
              </Card>
            )
          })}
        </div>
      )}
    </div>
  )
}

// ══════════════════════════════════════════════════════════════════
// INFERENCE CONNECTORS
// ══════════════════════════════════════════════════════════════════

const LLM_PROVIDERS: {
  id: LlmProviderType
  label: string
  icon: React.ReactNode
  description: string
  defaultModel: string
  color: string
}[] = [
  { id: 'open_router', label: 'OpenRouter',     icon: <Zap size={15} />,   description: 'Access 100+ models via a single API', defaultModel: 'openai/gpt-4o-mini', color: '#f97316' },
  { id: 'azure',       label: 'Azure OpenAI',   icon: <Cloud size={15} />, description: 'Microsoft Azure hosted OpenAI models',  defaultModel: 'gpt-4o',            color: '#0ea5e9' },
  { id: 'open_ai',     label: 'OpenAI',         icon: <Star size={15} />,  description: 'Direct OpenAI API (api.openai.com)',    defaultModel: 'gpt-4o',            color: '#10b981' },
  { id: 'custom',      label: 'Custom / Ollama', icon: <Globe size={15} />, description: 'Any OpenAI-compatible endpoint',        defaultModel: 'llama3.2',          color: '#8b5cf6' },
]

interface InferenceFormProps {
  editing?: LlmConnector        // present = edit mode
  onSave: (c: LlmConnector) => void
  onCancel: () => void
}

function InferenceForm({ editing, onSave, onCancel }: InferenceFormProps) {
  const [provider, setProvider] = useState<LlmProviderType>(editing?.provider_type ?? 'open_router')
  const [name, setName] = useState(editing?.name ?? '')
  const [apiKey, setApiKey] = useState('')   // always blank — server masks the stored key
  const [model, setModel] = useState(editing?.model ?? LLM_PROVIDERS[0].defaultModel)
  const [baseUrl, setBaseUrl] = useState(editing?.api_base_url ?? '')
  const [azureDeployment, setAzureDeployment] = useState(editing?.azure_deployment ?? '')
  const [azureVersion, setAzureVersion] = useState(editing?.azure_api_version ?? '2024-12-01-preview')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')
  const [testing, setTesting] = useState(false)
  const [testResult, setTestResult] = useState<{ ok: boolean; latency_ms: number; error?: string } | null>(null)

  const isEdit = !!editing
  const providerMeta = LLM_PROVIDERS.find(p => p.id === provider)!

  const handleProviderChange = (p: LlmProviderType) => {
    setProvider(p)
    const meta = LLM_PROVIDERS.find(x => x.id === p)!
    if (!isEdit) {
      setModel(meta.defaultModel)
      if (!name) setName(meta.label)
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')
    if (!name.trim() || !model.trim()) { setError('Name and model are required.'); return }
    if (!isEdit && !apiKey.trim()) { setError('API key is required.'); return }
    if (provider === 'azure' && !baseUrl.trim()) { setError('Azure base URL is required.'); return }
    setSubmitting(true)
    try {
      let c: LlmConnector
      if (isEdit) {
        c = await api.llmConnectors.patch(editing.id, {
          name: name.trim(),
          provider_type: provider,
          model: model.trim(),
          ...(apiKey.trim() ? { api_key: apiKey.trim() } : {}),
          api_base_url: baseUrl.trim() || undefined,
          azure_deployment: azureDeployment.trim() || undefined,
          azure_api_version: azureVersion.trim() || undefined,
        })
      } else {
        c = await api.llmConnectors.create({
          name: name.trim(),
          provider_type: provider,
          api_key: apiKey.trim(),
          model: model.trim(),
          api_base_url: baseUrl.trim() || undefined,
          azure_deployment: azureDeployment.trim() || undefined,
          azure_api_version: azureVersion.trim() || undefined,
        })
      }
      onSave(c)
    } catch (err) {
      setError(String(err))
    } finally {
      setSubmitting(false)
    }
  }

  const handleTestConnection = async () => {
    setTestResult(null)
    setError('')
    const effectiveKey = apiKey.trim() || (isEdit ? '__existing__' : '')
    if (!effectiveKey) { setError('Enter an API key to test.'); return }
    if (provider === 'azure' && !baseUrl.trim()) { setError('Azure base URL is required to test.'); return }
    if (!model.trim()) { setError('Model is required to test.'); return }
    setTesting(true)
    try {
      const result = await api.llmConnectors.test({
        name: name || 'test',
        provider_type: provider,
        api_key: apiKey.trim() || (isEdit ? '' : ''),
        model: model.trim(),
        api_base_url: baseUrl.trim() || undefined,
        azure_deployment: azureDeployment.trim() || undefined,
        azure_api_version: azureVersion.trim() || undefined,
      })
      setTestResult(result)
    } catch (err) {
      setTestResult({ ok: false, latency_ms: 0, error: String(err) })
    } finally {
      setTesting(false)
    }
  }

  return (
    <div className={styles.inferenceAddCard}>
      <div className={styles.inferenceAddHeader}>
        <h4 className={styles.inferenceAddTitle}>{isEdit ? `Edit — ${editing.name}` : 'Add Inference Connector'}</h4>
      </div>

      <div className={styles.providerPills}>
        {LLM_PROVIDERS.map(p => (
          <button
            key={p.id}
            type="button"
            className={`${styles.providerPill} ${provider === p.id ? styles.providerPillActive : ''}`}
            style={{ '--pc': p.color } as React.CSSProperties}
            onClick={() => handleProviderChange(p.id)}
          >
            <span className={styles.providerPillIcon} style={{ color: p.color }}>{p.icon}</span>
            <span>{p.label}</span>
            {provider === p.id && <Check size={11} className={styles.providerPillCheck} />}
          </button>
        ))}
      </div>

      <form onSubmit={handleSubmit} className={styles.inferenceForm}>
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
        <div className={styles.formGroup}>
          <label className={styles.formLabel}>API Key</label>
          <input
            className={styles.formInput}
            type="password"
            value={apiKey}
            onChange={e => setApiKey(e.target.value)}
            placeholder={isEdit ? 'Leave blank to keep existing key' : provider === 'azure' ? 'Azure resource key' : provider === 'open_router' ? 'sk-or-…' : 'sk-…'}
            autoComplete="off"
          />
        </div>
        {provider === 'azure' && (
          <>
            <div className={styles.formGroup}>
              <label className={styles.formLabel}>Resource Endpoint</label>
              <input className={styles.formInput} value={baseUrl} onChange={e => setBaseUrl(e.target.value)} placeholder="https://my-resource.openai.azure.com" />
              <p className={styles.fieldHint}>
                Found in Azure portal under <strong>Keys and Endpoint</strong>. Two formats are supported:
                <br />• Classic: <code>https://&lt;name&gt;.openai.azure.com</code>
                <br />• AI Foundry: <code>https://&lt;name&gt;.cognitiveservices.azure.com</code>
              </p>
            </div>
            <div className={styles.formRow}>
              <div className={styles.formGroup}>
                <label className={styles.formLabel}>Deployment Name</label>
                <input className={styles.formInput} value={azureDeployment} onChange={e => setAzureDeployment(e.target.value)} placeholder="gpt-4o-deployment" />
                <p className={styles.fieldHint}>
                  The name you gave the model in <strong>Azure AI Foundry → Deployments</strong>. Use this same name as your <em>Model</em> above if you didn't set a custom name.
                </p>
              </div>
              <div className={styles.formGroup}>
                <label className={styles.formLabel}>API Version</label>
                <input className={styles.formInput} value={azureVersion} onChange={e => setAzureVersion(e.target.value)} placeholder="2024-12-01-preview" />
                <p className={styles.fieldHint}>
                  Use <code>2024-12-01-preview</code> for AI Foundry resources, or <code>2024-02-01</code> for classic Azure OpenAI.
                </p>
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
            {testResult.ok
              ? <><Check size={13} /> Connected successfully · {testResult.latency_ms}ms</>
              : <><span className={styles.testResultError}>⚠ {testResult.error}</span></>}
          </div>
        )}
        <div className={styles.formActions}>
          <Button variant="secondary" size="sm" type="button" onClick={onCancel}>Cancel</Button>
          <button type="button" className={styles.testBtn} onClick={handleTestConnection} disabled={testing}>
            {testing ? <><Loader2 size={12} className={styles.spinning} /> Testing…</> : <><Zap size={12} /> Test Connection</>}
          </button>
          <Button variant="primary" size="sm" type="submit" disabled={submitting}>
            {submitting && <Loader2 size={12} className={styles.spinning} />}
            {isEdit ? 'Save Changes' : 'Save Connector'}
          </Button>
        </div>
      </form>
    </div>
  )
}

interface InferenceSectionProps {
  connectors: LlmConnector[]
  settings: Settings | null
  loading: boolean
  formState: { mode: 'add' } | { mode: 'edit'; connector: LlmConnector } | null
  onShowAdd: () => void
  onShowEdit: (c: LlmConnector) => void
  onHideForm: () => void
  onSave: (c: LlmConnector, isEdit: boolean) => void
  onSetDefault: (id: string) => void
  onDelete: (id: string) => void
}

function InferenceSection({ connectors, settings, loading, formState, onShowAdd, onShowEdit, onHideForm, onSave, onSetDefault, onDelete }: InferenceSectionProps) {
  if (loading) return <div className={styles.loadingRow}><Loader2 size={15} className={styles.spinning} /> Loading…</div>

  return (
    <div className={styles.sectionBody}>
      {formState && (
        <InferenceForm
          editing={formState.mode === 'edit' ? formState.connector : undefined}
          onSave={c => onSave(c, formState.mode === 'edit')}
          onCancel={onHideForm}
        />
      )}

      {!formState && connectors.length === 0 ? (
        <div className={styles.emptyState}>
          <Key size={28} strokeWidth={1.2} />
          <p>No inference connectors yet.</p>
          <Button size="sm" onClick={onShowAdd}><Plus size={14} /> Add Connector</Button>
        </div>
      ) : (
        <div className={styles.inferenceList}>
          {connectors.map(c => {
            const meta = LLM_PROVIDERS.find(p => p.id === c.provider_type)
            const isDefault = c.id === settings?.default_llm_connector_id
            const isEditing = formState?.mode === 'edit' && formState.connector.id === c.id
            return (
              <div key={c.id} className={`${styles.inferenceRow} ${isDefault ? styles.inferenceRowDefault : ''} ${isEditing ? styles.inferenceRowEditing : ''}`}>
                <div className={styles.inferenceRowLeft}>
                  <div className={styles.inferenceIcon} style={{ color: meta?.color ?? 'var(--accent-text)', background: `${meta?.color ?? 'var(--accent)'}18` }}>
                    {meta?.icon ?? <Key size={14} />}
                  </div>
                  <div>
                    <div className={styles.inferenceName}>{c.name}</div>
                    <div className={styles.inferenceMeta}>
                      <span style={{ color: meta?.color ?? 'var(--text-secondary)', fontWeight: 500 }}>{meta?.label ?? c.provider_type}</span>
                      <span className={styles.sep}>·</span>
                      <span style={{ fontFamily: 'var(--font-mono)', fontSize: 11 }}>{c.model}</span>
                      {c.api_key && <><span className={styles.sep}>·</span><span className={styles.maskedKey}>{c.api_key}</span></>}
                    </div>
                  </div>
                </div>
                <div className={styles.inferenceRowActions}>
                  {isDefault
                    ? <span className={styles.defaultBadge}><Check size={10} /> Default</span>
                    : <button className={styles.setDefaultBtn} onClick={() => onSetDefault(c.id)}>Set default</button>}
                  <button className={styles.editBtn} onClick={() => onShowEdit(c)} title="Edit">
                    <Pencil size={14} />
                  </button>
                  <button className={styles.deleteBtn} onClick={() => onDelete(c.id)} title="Delete">
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

// ══════════════════════════════════════════════════════════════════
// PAGE
// ══════════════════════════════════════════════════════════════════

export function ConnectorsPage() {
  const [tab, setTab] = useState<Tab>('platform')

  // Platform state
  const [platformConnectors, setPlatformConnectors] = useState<Connector[]>([])
  const [platformLoading, setPlatformLoading] = useState(true)
  const [showAddPlatform, setShowAddPlatform] = useState(false)

  // Inference state
  const [inferenceConnectors, setInferenceConnectors] = useState<LlmConnector[]>([])
  const [settings, setSettings] = useState<Settings | null>(null)
  const [inferenceLoading, setInferenceLoading] = useState(true)
  const [inferenceFormState, setInferenceFormState] = useState<{ mode: 'add' } | { mode: 'edit'; connector: LlmConnector } | null>(null)

  useEffect(() => {
    api.connectors.list().then(setPlatformConnectors).finally(() => setPlatformLoading(false))
    Promise.all([api.llmConnectors.list(), api.settings.get()])
      .then(([conns, s]) => { setInferenceConnectors(conns); setSettings(s) })
      .finally(() => setInferenceLoading(false))
  }, [])

  // Platform handlers
  async function handlePlatformToggle(id: string, enabled: boolean) {
    try {
      const updated = await api.connectors.patch(id, { enabled: !enabled })
      setPlatformConnectors(cs => cs.map(c => c.id === id ? updated : c))
    } catch { /* swallow */ }
  }
  async function handlePlatformDelete(id: string) {
    try {
      await api.connectors.delete(id)
      setPlatformConnectors(cs => cs.filter(c => c.id !== id))
    } catch { /* swallow */ }
  }

  // Inference handlers
  const handleInferenceSave = async (c: LlmConnector, isEdit: boolean) => {
    if (isEdit) {
      setInferenceConnectors(prev => prev.map(x => x.id === c.id ? c : x))
    } else {
      setInferenceConnectors(prev => [...prev, c])
      if (!settings?.default_llm_connector_id) {
        const s = await api.settings.patch({ default_llm_connector_id: c.id })
        setSettings(s)
      }
    }
    setInferenceFormState(null)
  }
  const handleSetDefault = async (id: string) => {
    const s = await api.settings.patch({ default_llm_connector_id: id })
    setSettings(s)
  }
  const handleInferenceDelete = async (id: string) => {
    await api.llmConnectors.delete(id)
    setInferenceConnectors(prev => prev.filter(c => c.id !== id))
    if (settings?.default_llm_connector_id === id) {
      const remaining = inferenceConnectors.filter(c => c.id !== id)
      const s = await api.settings.patch({ default_llm_connector_id: remaining[0]?.id ?? undefined })
      setSettings(s)
    }
  }

  const canAddOnTab = (tab === 'platform' && !showAddPlatform) || (tab === 'inference' && !inferenceFormState)

  return (
    <div className="fade-in">
      <PageHeader
        title="Connectors"
        description="Connect platforms and inference providers to power your agents."
        actions={canAddOnTab ? (
          <Button onClick={() => tab === 'platform' ? setShowAddPlatform(true) : setInferenceFormState({ mode: 'add' })}>
            <Plus size={15} /> Add Connector
          </Button>
        ) : undefined}
      />

      {/* Tab switcher */}
      <div className={styles.tabBar}>
        <button
          className={`${styles.tabBtn} ${tab === 'platform' ? styles.tabBtnActive : ''}`}
          onClick={() => setTab('platform')}
        >
          <Plug size={13} />
          Platform
          {platformConnectors.length > 0 && <span className={styles.tabCount}>{platformConnectors.length}</span>}
        </button>
        <button
          className={`${styles.tabBtn} ${tab === 'inference' ? styles.tabBtnActive : ''}`}
          onClick={() => setTab('inference')}
        >
          <Cpu size={13} />
          Inference
          {inferenceConnectors.length > 0 && <span className={styles.tabCount}>{inferenceConnectors.length}</span>}
        </button>
      </div>

      {tab === 'platform' ? (
        <PlatformSection
          connectors={platformConnectors}
          loading={platformLoading}
          onToggle={handlePlatformToggle}
          onDelete={handlePlatformDelete}
          onAdd={() => setShowAddPlatform(true)}
        />
      ) : (
        <InferenceSection
          connectors={inferenceConnectors}
          settings={settings}
          loading={inferenceLoading}
          formState={inferenceFormState}
          onShowAdd={() => setInferenceFormState({ mode: 'add' })}
          onShowEdit={c => setInferenceFormState({ mode: 'edit', connector: c })}
          onHideForm={() => setInferenceFormState(null)}
          onSave={handleInferenceSave}
          onSetDefault={handleSetDefault}
          onDelete={handleInferenceDelete}
        />
      )}

      {showAddPlatform && (
        <AddPlatformModal
          onClose={() => setShowAddPlatform(false)}
          onCreated={c => { setPlatformConnectors(cs => [...cs, c]); setShowAddPlatform(false) }}
        />
      )}
    </div>
  )
}
