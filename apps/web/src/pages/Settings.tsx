import { useState, useEffect } from 'react'
import {
  ChevronDown, Plus, Check, Loader2,
  Cloud, Zap, Globe, Star, Key, Trash2, Pencil, Cpu, Palette,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import { api, type Settings, type LlmConnector, type LlmProviderType } from '../lib/api'
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

// ── Inference Form ──────────────────────────────────────────────

interface InferenceFormProps {
  editing?: LlmConnector
  onSave: (c: LlmConnector) => void
  onCancel: () => void
}

function InferenceForm({ editing, onSave, onCancel }: InferenceFormProps) {
  const [provider, setProvider] = useState<LlmProviderType>(editing?.provider_type ?? 'open_router')
  const [name, setName] = useState(editing?.name ?? '')
  const [apiKey, setApiKey] = useState('')
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
      <h4 className={styles.inferenceAddTitle}>{isEdit ? `Edit \u2014 ${editing.name}` : 'Add LLM Connector'}</h4>

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

      <form onSubmit={handleSubmit}>
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
            placeholder={isEdit ? 'Leave blank to keep existing key' : provider === 'azure' ? 'Azure resource key' : provider === 'open_router' ? 'sk-or-...' : 'sk-...'}
            autoComplete="off"
          />
        </div>
        {provider === 'azure' && (
          <>
            <div className={styles.formGroup}>
              <label className={styles.formLabel}>Resource Endpoint</label>
              <input className={styles.formInput} value={baseUrl} onChange={e => setBaseUrl(e.target.value)} placeholder="https://my-resource.openai.azure.com" />
            </div>
            <div className={styles.formRow}>
              <div className={styles.formGroup}>
                <label className={styles.formLabel}>Deployment Name</label>
                <input className={styles.formInput} value={azureDeployment} onChange={e => setAzureDeployment(e.target.value)} placeholder="gpt-4o-deployment" />
              </div>
              <div className={styles.formGroup}>
                <label className={styles.formLabel}>API Version</label>
                <input className={styles.formInput} value={azureVersion} onChange={e => setAzureVersion(e.target.value)} placeholder="2024-12-01-preview" />
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
              ? <><Check size={13} /> Connected \u00b7 {testResult.latency_ms}ms</>
              : <span>{testResult.error}</span>}
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

// ── Settings Page ───────────────────────────────────────────────

export function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [llmConnectors, setLlmConnectors] = useState<LlmConnector[]>([])
  const [loading, setLoading] = useState(true)
  const [formState, setFormState] = useState<{ mode: 'add' } | { mode: 'edit'; connector: LlmConnector } | null>(null)

  useEffect(() => {
    Promise.all([api.settings.get(), api.llmConnectors.list()])
      .then(([s, conns]) => { setSettings(s); setLlmConnectors(conns) })
      .finally(() => setLoading(false))
  }, [])

  const handleSave = async (c: LlmConnector) => {
    const isEdit = formState?.mode === 'edit'
    if (isEdit) {
      setLlmConnectors(prev => prev.map(x => x.id === c.id ? c : x))
    } else {
      setLlmConnectors(prev => [...prev, c])
      if (!settings?.default_llm_connector_id) {
        const s = await api.settings.patch({ default_llm_connector_id: c.id })
        setSettings(s)
      }
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
      <PageHeader
        title="Settings"
        description="Manage LLM inference providers, appearance, and application preferences."
      />

      <div className={styles.sections}>
        {/* ── LLM Inference Connectors ── */}
        <section>
          <div className={styles.sectionHeader}>
            <div className={styles.sectionHeaderLeft}>
              <div className={styles.sectionIconWrap}><Cpu size={16} /></div>
              <div>
                <h3 className={styles.sectionTitle}>LLM Connectors</h3>
                <p className={styles.sectionDesc}>Configure inference providers for your agents.</p>
              </div>
            </div>
            {!formState && (
              <Button size="sm" onClick={() => setFormState({ mode: 'add' })}>
                <Plus size={13} /> Add
              </Button>
            )}
          </div>

          {formState && (
            <InferenceForm
              editing={formState.mode === 'edit' ? formState.connector : undefined}
              onSave={handleSave}
              onCancel={() => setFormState(null)}
            />
          )}

          {loading ? (
            <div className={styles.loadingRow}><Loader2 size={14} className="spinning" /> Loading...</div>
          ) : llmConnectors.length === 0 && !formState ? (
            <div className={styles.emptyConnectors}>
              <Key size={28} strokeWidth={1} />
              <p>No LLM connectors configured.</p>
              <Button size="sm" onClick={() => setFormState({ mode: 'add' })}><Plus size={13} /> Add Connector</Button>
            </div>
          ) : (
            <div className={styles.inferenceList}>
              {llmConnectors.map(c => {
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
                          <span className={styles.sep}>&middot;</span>
                          <span style={{ fontFamily: 'var(--font-mono)', fontSize: 11 }}>{c.model}</span>
                          {c.api_key && <><span className={styles.sep}>&middot;</span><span className={styles.maskedKey}>{c.api_key}</span></>}
                        </div>
                      </div>
                    </div>
                    <div className={styles.inferenceRowActions}>
                      {isDefault
                        ? <span className={styles.defaultBadge}><Check size={10} /> Default</span>
                        : <button className={styles.setDefaultBtn} onClick={() => handleSetDefault(c.id)}>Set default</button>}
                      <button className={styles.editBtn} onClick={() => setFormState({ mode: 'edit', connector: c })} title="Edit">
                        <Pencil size={14} />
                      </button>
                      <button className={styles.deleteBtn} onClick={() => handleDelete(c.id)} title="Delete">
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </section>

        {/* ── Appearance ── */}
        <Card>
          <div className={styles.sectionHeader} style={{ marginBottom: 16 }}>
            <div className={styles.sectionHeaderLeft}>
              <div className={styles.sectionIconWrap}><Palette size={16} /></div>
              <div>
                <h3 className={styles.sectionTitle}>Appearance</h3>
                <p className={styles.sectionDesc}>Interface theme and display preferences.</p>
              </div>
            </div>
          </div>

          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Theme</label>
            <div className={styles.selectWrap}>
              <select
                className={styles.select}
                value={settings?.theme ?? 'dark'}
                onChange={async e => {
                  const s = await api.settings.patch({ theme: e.target.value })
                  setSettings(s)
                }}
              >
                <option value="dark">Dark</option>
                <option value="light">Light</option>
                <option value="system">System</option>
              </select>
              <ChevronDown size={14} className={styles.selectChevron} />
            </div>
          </div>
        </Card>
      </div>
    </div>
  )
}
