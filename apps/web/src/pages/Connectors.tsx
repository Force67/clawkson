import { useEffect, useState } from 'react'
import {
  Plus, Mail, MessageSquare, ToggleLeft, ToggleRight, Trash2, Send,
  Loader2, Plug, GitBranch,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import { api } from '../lib/api'
import type { Agent, Connector, ConnectorType } from '../lib/api'
import styles from './Connectors.module.css'

const PLATFORM_META: Record<ConnectorType, { label: string; icon: typeof MessageSquare; description: string; color: string }> = {
  telegram: { label: 'Telegram', icon: Send, description: 'Receive and send messages through Telegram Bot API.', color: '#2AABEE' },
  gmail: { label: 'Gmail', icon: Mail, description: 'Read and send emails via Gmail API.', color: '#EA4335' },
  slack: { label: 'Slack', icon: MessageSquare, description: 'Integrate with Slack for team notifications.', color: '#4A154B' },
  azure_devops: { label: 'Azure DevOps', icon: GitBranch, description: 'Access work items, pipelines, and repos via Azure DevOps.', color: '#0078D4' },
  custom: { label: 'Custom', icon: Plug, description: 'A custom integration connector.', color: 'var(--accent)' },
}

interface AddPlatformModalProps {
  onClose: () => void
  onCreated: (c: Connector) => void
}

function AddPlatformModal({ onClose, onCreated }: AddPlatformModalProps) {
  const [type, setType] = useState<ConnectorType>('telegram')
  const [name, setName] = useState('My Telegram Bot')
  // Telegram
  const [botToken, setBotToken] = useState('')
  const [agentId, setAgentId] = useState('')
  const [agents, setAgents] = useState<Agent[]>([])
  // Azure DevOps
  const [azureOrg, setAzureOrg] = useState('')
  const [azurePat, setAzurePat] = useState('')

  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    api.agents.list().then(list => {
      setAgents(list)
      if (list.length > 0 && !agentId) setAgentId(list[0].id)
    })
  }, [])

  function handleTypeChange(t: ConnectorType) {
    setType(t)
    const defaults: Record<ConnectorType, string> = {
      telegram: 'My Telegram Bot',
      gmail: 'My Gmail',
      slack: 'My Slack',
      azure_devops: 'My Azure DevOps',
      custom: 'My Connector',
    }
    setName(defaults[t])
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setError(null)
    if (!name.trim()) { setError('Name is required.'); return }
    if (type === 'telegram' && !botToken.trim()) { setError('Bot token is required.'); return }
    if (type === 'telegram' && !agentId) { setError('Please select an agent to handle messages.'); return }
    if (type === 'azure_devops') {
      if (!azureOrg.trim()) { setError('Organization is required.'); return }
      if (!azurePat.trim()) { setError('Personal Access Token is required.'); return }
    }
    setSaving(true)
    try {
      let config: Record<string, string> = {}
      if (type === 'telegram') config = { bot_token: botToken.trim(), agent_id: agentId }
      if (type === 'azure_devops') config = { organization: azureOrg.trim(), pat: azurePat.trim() }
      const connector = await api.connectors.create({
        name: name.trim(),
        connector_type: type,
        config,
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
            <select className={styles.select} value={type} onChange={e => handleTypeChange(e.target.value as ConnectorType)}>
              <option value="telegram">Telegram</option>
              <option value="gmail">Gmail</option>
              <option value="slack">Slack</option>
              <option value="azure_devops">Azure DevOps</option>
              <option value="custom">Custom</option>
            </select>
          </label>
          <label className={styles.fieldLabel}>
            Name
            <input className={styles.input} value={name} onChange={e => setName(e.target.value)} placeholder="e.g. My Bot" />
          </label>
          {type === 'telegram' && (
            <>
              <label className={styles.fieldLabel}>
                Bot Token
                <input className={styles.input} value={botToken} onChange={e => setBotToken(e.target.value)} placeholder="123456789:ABCdef..." autoComplete="off" />
                <span className={styles.hint}>Get from <a href="https://t.me/BotFather" target="_blank" rel="noreferrer">@BotFather</a> on Telegram.</span>
              </label>
              <label className={styles.fieldLabel}>
                Agent
                <select className={styles.select} value={agentId} onChange={e => setAgentId(e.target.value)}>
                  {agents.length === 0 && <option value="">No agents available</option>}
                  {agents.map(a => <option key={a.id} value={a.id}>{a.name}</option>)}
                </select>
                <span className={styles.hint}>The agent that will respond to incoming Telegram messages.</span>
              </label>
            </>
          )}
          {type === 'azure_devops' && (
            <>
              <label className={styles.fieldLabel}>
                Organization
                <input
                  className={styles.input}
                  value={azureOrg}
                  onChange={e => setAzureOrg(e.target.value)}
                  placeholder="myorg"
                  autoComplete="off"
                />
                <span className={styles.hint}>Your Azure DevOps organization name (dev.azure.com/<strong>myorg</strong>).</span>
              </label>
              <label className={styles.fieldLabel}>
                Personal Access Token (PAT)
                <input
                  className={styles.input}
                  type="password"
                  value={azurePat}
                  onChange={e => setAzurePat(e.target.value)}
                  placeholder="Paste your PAT here"
                  autoComplete="new-password"
                />
                <span className={styles.hint}>
                  Generate a PAT in Azure DevOps under <strong>User Settings → Personal access tokens</strong>.
                  Required scopes depend on the tools you intend to use (e.g. Work Items Read &amp; Write, Code Read).
                </span>
              </label>
            </>
          )}
          {error && <p className={styles.errorMsg}>{error}</p>}
          <div className={styles.modalActions}>
            <Button variant="secondary" type="button" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={saving}>{saving ? 'Adding...' : 'Add Connector'}</Button>
          </div>
        </form>
      </div>
    </div>
  )
}

export function ConnectorsPage() {
  const [connectors, setConnectors] = useState<Connector[]>([])
  const [loading, setLoading] = useState(true)
  const [showAdd, setShowAdd] = useState(false)

  useEffect(() => {
    api.connectors.list().then(setConnectors).finally(() => setLoading(false))
  }, [])

  async function handleToggle(id: string, enabled: boolean) {
    try {
      const updated = await api.connectors.patch(id, { enabled: !enabled })
      setConnectors(cs => cs.map(c => c.id === id ? updated : c))
    } catch { /* swallow */ }
  }

  async function handleDelete(id: string) {
    try {
      await api.connectors.delete(id)
      setConnectors(cs => cs.filter(c => c.id !== id))
    } catch { /* swallow */ }
  }

  return (
    <div className="fade-in">
      <PageHeader
        title="Connectors"
        description="Connect platforms like Telegram, Gmail, Slack, and Azure DevOps to enable agent interactions."
        actions={
          <Button onClick={() => setShowAdd(true)}>
            <Plus size={15} /> Add Connector
          </Button>
        }
      />

      {loading ? (
        <div className={styles.loadingRow}><Loader2 size={15} className="spinning" /> Loading...</div>
      ) : connectors.length === 0 ? (
        <div className={styles.emptyState}>
          <div className={styles.emptyIcon}>
            <Plug size={40} strokeWidth={1} />
          </div>
          <p className={styles.emptyTitle}>No platform connectors yet</p>
          <p className={styles.emptyDesc}>
            Add a connector to let your agents interact with external platforms.
          </p>
          <Button size="sm" onClick={() => setShowAdd(true)}><Plus size={14} /> Add Connector</Button>
        </div>
      ) : (
        <div className={`${styles.platformGrid} stagger`}>
          {connectors.map(conn => {
            const meta = PLATFORM_META[conn.connector_type] ?? PLATFORM_META.custom
            const Icon = meta.icon
            return (
              <Card key={conn.id}>
                <div className={styles.connHeader}>
                  <div className={styles.connIcon} style={{ color: meta.color, background: `color-mix(in srgb, ${meta.color} 12%, transparent)` }}>
                    <Icon size={20} strokeWidth={1.5} />
                  </div>
                  <div className={styles.connActions}>
                    <button className={styles.connToggle} onClick={() => handleToggle(conn.id, conn.enabled)} title={conn.enabled ? 'Disable' : 'Enable'}>
                      {conn.enabled
                        ? <ToggleRight size={24} className={styles.toggleOn} />
                        : <ToggleLeft size={24} className={styles.toggleOff} />}
                    </button>
                    <button className={styles.deleteBtn} onClick={() => handleDelete(conn.id)} title="Delete"><Trash2 size={15} /></button>
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

      {showAdd && (
        <AddPlatformModal
          onClose={() => setShowAdd(false)}
          onCreated={c => { setConnectors(cs => [...cs, c]); setShowAdd(false) }}
        />
      )}
    </div>
  )
}
