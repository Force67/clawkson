import { useState, useEffect, useRef } from 'react'
import {
  ChevronDown, Plus, Check, Loader2, X,
  Cloud, Zap, Globe, Star, Key, Trash2, Pencil, Cpu, Palette, Database,
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
            // Patch the existing connector with the first entry
            last = await api.llmConnectors.patch(editing.id, {
              name: connectorName,
              provider_type: 'azure',
              model: dep.model.trim(),
              ...(apiKey.trim() ? { api_key: apiKey.trim() } : {}),
              api_base_url: baseUrl.trim() || undefined,
              azure_deployment: dep.deployment.trim(),
              azure_api_version: azureVersion.trim() || undefined,
            })
          } else {
            // Create new connectors for additional entries (or all entries in add mode)
            if (!apiKey.trim()) { setError('API key is required for new deployments.'); setSubmitting(false); return }
            last = await api.llmConnectors.create({
              name: connectorName,
              provider_type: 'azure',
              api_key: apiKey.trim(),
              model: dep.model.trim(),
              api_base_url: baseUrl.trim() || undefined,
              azure_deployment: dep.deployment.trim(),
              azure_api_version: azureVersion.trim() || undefined,
            })
          }
        }
        if (last) onSave(last)
      } catch (err) {
        setError(String(err))
      } finally {
        setSubmitting(false)
      }
      return
    }

    if (!name.trim() || !model.trim()) { setError('Name and model are required.'); return }
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
        })
      } else {
        c = await api.llmConnectors.create({
          name: name.trim(),
          provider_type: provider,
          api_key: apiKey.trim(),
          model: model.trim(),
          api_base_url: baseUrl.trim() || undefined,
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

    // For Azure multi-mode, test the first deployment; for edit/other, test model field
    const testModel = isAzureMulti
      ? (azureDeployments[0]?.model.trim() || 'gpt-4o')
      : model.trim()
    const testDeployment = isAzureMulti
      ? (azureDeployments[0]?.deployment.trim() || undefined)
      : undefined

    if (!testModel) { setError('Model is required to test.'); return }
    setTesting(true)
    try {
      const result = await api.llmConnectors.test({
        name: name || 'test',
        provider_type: provider,
        api_key: apiKey.trim() || (isEdit ? '' : ''),
        model: testModel,
        api_base_url: baseUrl.trim() || undefined,
        azure_deployment: testDeployment,
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
          <input
            className={styles.formInput}
            type="password"
            value={apiKey}
            onChange={e => setApiKey(e.target.value)}
            placeholder={isEdit ? 'Leave blank to keep existing key' : provider === 'azure' ? 'Azure resource key' : provider === 'open_router' ? 'sk-or-...' : 'sk-...'}
            autoComplete="off"
          />
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
                    <input
                      className={styles.formInput}
                      value={dep.deployment}
                      onChange={e => updateAzureDeployment(i, 'deployment', e.target.value)}
                      placeholder="deployment-name"
                      style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}
                    />
                    <input
                      className={styles.formInput}
                      value={dep.model}
                      onChange={e => updateAzureDeployment(i, 'model', e.target.value)}
                      placeholder="model (e.g. gpt-4o)"
                      style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}
                    />
                    {azureDeployments.length > 1 && (
                      <button type="button" className={styles.azureDeploymentRemove} onClick={() => removeAzureDeployment(i)} title="Remove">
                        <X size={14} />
                      </button>
                    )}
                  </div>
                ))}
                <button type="button" className={styles.azureDeploymentAdd} onClick={addAzureDeployment}>
                  <Plus size={12} /> Add deployment
                </button>
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

// ── Embedding Config Form ────────────────────────────────────────

interface EmbeddingConfigFormProps {
  settings: Settings | null
  onUpdate: (s: Settings) => void
}

function EmbeddingConfigForm({ settings, onUpdate }: EmbeddingConfigFormProps) {
  const [baseUrl, setBaseUrl] = useState('')
  const [model, setModel] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [saving, setSaving] = useState(false)
  const debounceRef = useRef<ReturnType<typeof setTimeout>>()
  const pendingRef = useRef<Record<string, string>>({})

  // Sync local state from loaded settings (once, or when settings load)
  useEffect(() => {
    if (settings) {
      setBaseUrl(settings.embedding_api_base_url ?? '')
      setModel(settings.embedding_model ?? '')
    }
  }, [settings?.embedding_api_base_url, settings?.embedding_model])

  // Flush any pending save on unmount (page navigation)
  useEffect(() => {
    return () => {
      clearTimeout(debounceRef.current)
      const pending = pendingRef.current
      if (Object.keys(pending).length > 0) {
        api.settings.patch(pending).catch(() => {})
      }
    }
  }, [])

  const debouncedSave = (patch: Record<string, string>) => {
    // Accumulate fields so rapid edits to different fields are batched
    pendingRef.current = { ...pendingRef.current, ...patch }
    clearTimeout(debounceRef.current)
    setSaving(true)
    debounceRef.current = setTimeout(async () => {
      const toSave = { ...pendingRef.current }
      pendingRef.current = {}
      try {
        const s = await api.settings.patch(toSave)
        onUpdate(s)
      } catch (e) {
        console.error('Failed to save embedding settings:', e)
      } finally {
        setSaving(false)
      }
    }, 600)
  }

  return (
    <>
      <div className={styles.formGroup}>
        <label className={styles.formLabel}>Embedding API Base URL</label>
        <input
          className={styles.formInput}
          value={baseUrl}
          placeholder="http://localhost:11434/v1"
          style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}
          onChange={e => {
            setBaseUrl(e.target.value)
            const val = e.target.value.trim()
            if (val) debouncedSave({ embedding_api_base_url: val })
          }}
        />
        <p className={styles.formHint}>
          Any OpenAI-compatible <code>/v1/embeddings</code> endpoint — Ollama, vLLM, LiteLLM, OpenAI, etc.
        </p>
      </div>

      <div className={styles.formGroup}>
        <label className={styles.formLabel}>Embedding API Key</label>
        <input
          className={styles.formInput}
          type="password"
          value={apiKey}
          placeholder="Leave blank to keep existing key"
          autoComplete="off"
          onChange={e => {
            setApiKey(e.target.value)
            const val = e.target.value.trim()
            if (val) debouncedSave({ embedding_api_key: val })
          }}
          onBlur={() => {
            if (apiKey.trim()) setApiKey('')
          }}
        />
        {settings?.embedding_api_key && (
          <p className={styles.formHint}>
            Current key: <code>{settings.embedding_api_key}</code>
          </p>
        )}
      </div>

      <div className={styles.formGroup}>
        <label className={styles.formLabel}>
          Embedding Model
          {saving && <Loader2 size={11} className="spinning" style={{ marginLeft: 6, display: 'inline-block' }} />}
        </label>
        <input
          className={styles.formInput}
          value={model}
          placeholder="qwen3-embedding:8b"
          style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}
          onChange={e => {
            setModel(e.target.value)
            const val = e.target.value.trim()
            if (val) debouncedSave({ embedding_model: val })
          }}
        />
        <p className={styles.formHint}>
          The model used to generate vector embeddings for knowledge base entries and search queries.
          Each knowledge base can optionally override this with its own model.
        </p>
      </div>
    </>
  )
}

// ── Theme helper ────────────────────────────────────────────────

function applyTheme(theme: string) {
  document.documentElement.setAttribute('data-theme', theme)
}

// ── Settings Page ───────────────────────────────────────────────

export function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const [llmConnectors, setLlmConnectors] = useState<LlmConnector[]>([])
  const [loading, setLoading] = useState(true)
  const [formState, setFormState] = useState<{ mode: 'add' } | { mode: 'edit'; connector: LlmConnector } | null>(null)

  useEffect(() => {
    Promise.all([api.settings.get(), api.llmConnectors.list()])
      .then(([s, conns]) => {
        setSettings(s)
        setLlmConnectors(conns)
        applyTheme(s.theme)
      })
      .finally(() => setLoading(false))
  }, [])

  const handleSave = async (_c: LlmConnector) => {
    const isEdit = formState?.mode === 'edit'
    // Reload full list to capture batch-created Azure connectors
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

        {/* ── ETL Processing ── */}
        <Card>
          <div className={styles.sectionHeader} style={{ marginBottom: 16 }}>
            <div className={styles.sectionHeaderLeft}>
              <div className={styles.sectionIconWrap}><Database size={16} /></div>
              <div>
                <h3 className={styles.sectionTitle}>ETL Processing</h3>
                <p className={styles.sectionDesc}>
                  Configure the embedding provider and optional semantic chunking for Knowledge Base ingestion.
                </p>
              </div>
            </div>
          </div>

          <EmbeddingConfigForm settings={settings} onUpdate={setSettings} />

          <div className={styles.etlDivider} />

          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Semantic Chunking Model</label>
            <div className={styles.selectWrap}>
              <select
                className={styles.select}
                value={settings?.etl_llm_connector_id ?? ''}
                onChange={async e => {
                  const val = e.target.value
                  const s = await api.settings.patch({
                    etl_llm_connector_id: val === '' ? null : val,
                  })
                  setSettings(s)
                }}
              >
                <option value="">None (heuristic chunking)</option>
                {llmConnectors.map(c => {
                  const meta = LLM_PROVIDERS.find(p => p.id === c.provider_type)
                  return (
                    <option key={c.id} value={c.id}>
                      {c.name} — {meta?.label ?? c.provider_type} / {c.model}
                    </option>
                  )
                })}
              </select>
              <ChevronDown size={14} className={styles.selectChevron} />
            </div>
            {settings?.etl_llm_connector_id && (
              <p className={styles.formHint}>
                The selected model will be called to locate semantically coherent chunk
                boundaries when documents exceed the maximum chunk size. The full document
                is never sent to the LLM — only small context windows around potential split
                points.
              </p>
            )}
          </div>
        </Card>

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
                  const theme = e.target.value
                  applyTheme(theme)
                  const s = await api.settings.patch({ theme })
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

        {/* ── Advanced ── */}
        <Card>
          <div className={styles.sectionHeader} style={{ marginBottom: 16 }}>
            <div className={styles.sectionHeaderLeft}>
              <div className={styles.sectionIconWrap}><Zap size={16} /></div>
              <div>
                <h3 className={styles.sectionTitle}>Advanced</h3>
                <p className={styles.sectionDesc}>Low-level tuning for LLM request behaviour.</p>
              </div>
            </div>
          </div>

          <div className={styles.formGroup}>
            <label className={styles.formLabel}>LLM Request Timeout (seconds)</label>
            <input
              className={styles.formInput}
              type="number"
              min={10}
              max={600}
              style={{ width: 120 }}
              value={settings?.llm_request_timeout_secs ?? 120}
              onChange={async e => {
                const val = parseInt(e.target.value, 10)
                if (isNaN(val)) return
                const s = await api.settings.patch({ llm_request_timeout_secs: val })
                setSettings(s)
              }}
            />
            <p className={styles.formHint}>
              Maximum time (10–600 s) to wait for a single LLM response before aborting.
              Increase this if long-running models (e.g. Azure OpenAI o-series) time out.
              Default: 120 s.
            </p>
          </div>
        </Card>
      </div>
    </div>
  )
}
