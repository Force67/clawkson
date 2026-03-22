import { useState, useEffect, useCallback, useRef } from 'react'
import {
  Webhook as WebhookIcon, Plus, Trash2, Pencil, Copy, Check, Eye, EyeOff,
  Loader2, X, ChevronDown, ChevronRight, ExternalLink, Play,
} from 'lucide-react'
import { ToggleLeft, ToggleRight } from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import {
  api,
  type Agent,
  type Webhook,
  type WebhookExecution,
  type CreateWebhookRequest,
  type PatchWebhookRequest,
} from '../lib/api'
import styles from './Webhooks.module.css'

// ── Helpers ──────────────────────────────────────────────────────

function formatDuration(ms: number | null | undefined): string {
  if (!ms) return '-'
  if (ms < 1000) return `${ms}ms`
  const secs = Math.round(ms / 1000)
  if (secs < 60) return `${secs}s`
  return `${Math.floor(secs / 60)}m ${secs % 60}s`
}

function timeAgo(iso: string | null | undefined): string {
  if (!iso) return 'Never'
  const diff = Date.now() - new Date(iso).getTime()
  const mins = Math.floor(diff / 60000)
  if (mins < 1) return 'Just now'
  if (mins < 60) return `${mins}m ago`
  const hours = Math.floor(mins / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  return `${days}d ago`
}

function maskSecret(secret: string): string {
  if (secret.length <= 8) return '********'
  return secret.slice(0, 4) + '****' + secret.slice(-4)
}

// ── Webhook Modal ────────────────────────────────────────────────

interface WebhookModalProps {
  webhook: Webhook | null
  agents: Agent[]
  onClose: () => void
  onSaved: (w: Webhook) => void
}

function WebhookModal({ webhook, agents, onClose, onSaved }: WebhookModalProps) {
  const [name, setName] = useState(webhook?.name ?? '')
  const [description, setDescription] = useState(webhook?.description ?? '')
  const [agentId, setAgentId] = useState(webhook?.agent_id ?? (agents[0]?.id ?? ''))
  const [payloadTemplate, setPayloadTemplate] = useState(webhook?.payload_template ?? '')
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setError(null)
    if (!name.trim()) { setError('Name is required.'); return }
    if (!agentId && !webhook) { setError('Please select an agent.'); return }

    setSaving(true)
    try {
      if (webhook) {
        const body: PatchWebhookRequest = {}
        if (name !== webhook.name) body.name = name.trim()
        if (description !== webhook.description) body.description = description.trim()
        if (payloadTemplate !== (webhook.payload_template ?? '')) {
          body.payload_template = payloadTemplate.trim() || null
        }
        const updated = await api.webhooks.patch(webhook.id, body)
        onSaved(updated)
      } else {
        const body: CreateWebhookRequest = {
          name: name.trim(),
          agent_id: agentId,
          description: description.trim() || undefined,
          payload_template: payloadTemplate.trim() || null,
        }
        const created = await api.webhooks.create(body)
        onSaved(created)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className={styles.modalOverlay} onClick={onClose}>
      <div className={styles.modal} onClick={e => e.stopPropagation()}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
          <div className={styles.modalTitle}>{webhook ? 'Edit Webhook' : 'New Webhook'}</div>
          <button className={styles.actionBtn} onClick={onClose}><X size={16} /></button>
        </div>

        <form className={styles.form} onSubmit={handleSubmit}>
          <label className={styles.fieldLabel}>
            Name
            <input
              className={styles.input}
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="GitHub push handler"
            />
          </label>

          <label className={styles.fieldLabel}>
            Description
            <input
              className={styles.input}
              value={description}
              onChange={e => setDescription(e.target.value)}
              placeholder="Triggered on new commits..."
            />
          </label>

          {!webhook && (
            <label className={styles.fieldLabel}>
              Agent
              <select
                className={styles.select}
                value={agentId}
                onChange={e => setAgentId(e.target.value)}
              >
                {agents.map(a => (
                  <option key={a.id} value={a.id}>{a.name}</option>
                ))}
              </select>
            </label>
          )}

          <label className={styles.fieldLabel}>
            Payload Template
            <textarea
              className={styles.textarea}
              value={payloadTemplate}
              onChange={e => setPayloadTemplate(e.target.value)}
              placeholder={'Summarize the following webhook payload:\n{{payload}}'}
              rows={4}
            />
            <span className={styles.hint}>{'Use {{payload}} to inject the incoming JSON. Leave blank for raw forwarding.'}</span>
          </label>

          {error && <div className={styles.errorMsg}>{error}</div>}

          <div className={styles.modalActions}>
            <Button variant="ghost" type="button" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={saving}>
              {saving ? <Loader2 size={14} style={{ animation: 'spin 1s linear infinite' }} /> : null}
              {webhook ? 'Save Changes' : 'Create Webhook'}
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── Webhook Card ─────────────────────────────────────────────────

interface WebhookCardProps {
  webhook: Webhook
  agentName: string
  onToggle: () => void
  onEdit: () => void
  onDelete: () => void
}

function WebhookCard({ webhook, agentName, onToggle, onEdit, onDelete }: WebhookCardProps) {
  const [showHistory, setShowHistory] = useState(false)
  const [history, setHistory] = useState<WebhookExecution[]>([])
  const [loadingHistory, setLoadingHistory] = useState(false)
  const [secretRevealed, setSecretRevealed] = useState(false)
  const [copied, setCopied] = useState(false)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const webhookUrl = `${window.location.origin}/api/webhooks/${webhook.id}/incoming`

  // Cleanup polling on unmount
  useEffect(() => {
    return () => { if (pollRef.current) clearInterval(pollRef.current) }
  }, [])

  async function loadHistory() {
    setLoadingHistory(true)
    try {
      const h = await api.webhooks.executions(webhook.id)
      setHistory(h)
      // Check if any are still running
      const hasRunning = h.some(e => e.status === 'running')
      if (!hasRunning && pollRef.current) {
        clearInterval(pollRef.current)
        pollRef.current = null
      }
    } catch { /* ignore */ }
    setLoadingHistory(false)
  }

  async function toggleHistory() {
    if (!showHistory) {
      await loadHistory()
      // Start polling if any are running
      const hasRunning = history.some(e => e.status === 'running')
      if (hasRunning && !pollRef.current) {
        pollRef.current = setInterval(() => loadHistory(), 3000)
      }
    } else {
      if (pollRef.current) {
        clearInterval(pollRef.current)
        pollRef.current = null
      }
    }
    setShowHistory(prev => !prev)
  }

  function handleCopy() {
    navigator.clipboard.writeText(webhookUrl)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <Card>
      <div className={styles.webhookHeader}>
        <div className={styles.webhookName}>{webhook.name}</div>
        <div className={styles.webhookActions}>
          <button className={styles.actionBtn} onClick={onEdit} title="Edit">
            <Pencil size={14} />
          </button>
          <button className={styles.toggle} onClick={onToggle} title={webhook.enabled ? 'Disable' : 'Enable'}>
            {webhook.enabled
              ? <ToggleRight size={20} className={styles.toggleOn} />
              : <ToggleLeft size={20} className={styles.toggleOff} />}
          </button>
          <button className={`${styles.actionBtn} ${styles.deleteBtn}`} onClick={onDelete} title="Delete">
            <Trash2 size={14} />
          </button>
        </div>
      </div>

      <div className={styles.webhookAgent}>{agentName}</div>
      {webhook.description && (
        <div className={styles.webhookDescription}>{webhook.description}</div>
      )}

      {/* Webhook URL */}
      <div className={styles.urlRow}>
        <div className={styles.urlLabel}>URL</div>
        <div className={styles.urlValue}>
          <span className={styles.urlText}>{webhookUrl}</span>
          <button className={styles.copyBtn} onClick={handleCopy} title="Copy URL">
            {copied ? <Check size={12} /> : <Copy size={12} />}
          </button>
        </div>
      </div>

      {/* Secret */}
      <div className={styles.secretRow}>
        <div className={styles.secretLabel}>Secret</div>
        <div className={styles.secretValue}>
          <span className={styles.secretText}>
            {secretRevealed ? webhook.secret : maskSecret(webhook.secret)}
          </span>
          <button
            className={styles.revealBtn}
            onClick={() => setSecretRevealed(prev => !prev)}
            title={secretRevealed ? 'Hide secret' : 'Reveal secret'}
          >
            {secretRevealed ? <EyeOff size={12} /> : <Eye size={12} />}
          </button>
        </div>
      </div>

      <div className={styles.webhookMeta}>
        <div className={styles.metaItem}>
          Created {timeAgo(webhook.created_at)}
        </div>
        <div className={styles.metaItem}>
          Updated {timeAgo(webhook.updated_at)}
        </div>
      </div>

      {/* Execution history */}
      <div className={styles.historySection}>
        <button className={styles.historyToggle} onClick={toggleHistory}>
          {showHistory ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
          Recent executions
          {loadingHistory && <Loader2 size={10} style={{ animation: 'spin 1s linear infinite' }} />}
        </button>

        {showHistory && (
          <div className={styles.historyList}>
            {history.length === 0 && !loadingHistory && (
              <div className={styles.historyItem} style={{ justifyContent: 'center', color: 'var(--text-tertiary)' }}>
                No executions yet
              </div>
            )}
            {history.map(exec => (
              <div key={exec.id} className={styles.historyEntry}>
                <div className={styles.historyItem}>
                  <div className={styles.historyLeft}>
                    <span className={`${styles.badge} ${
                      exec.status === 'success' ? styles.badgeSuccess :
                      exec.status === 'error' ? styles.badgeError :
                      styles.badgeRunning
                    }`}>
                      {exec.status === 'running' && <Loader2 size={9} style={{ animation: 'spin 1s linear infinite' }} />}
                      {exec.status}
                    </span>
                    <span>{timeAgo(exec.started_at)}</span>
                  </div>
                  <div className={styles.historyRight}>
                    <span>{formatDuration(exec.duration_ms)}</span>
                    {exec.conversation_id && (
                      <a
                        href={`/conversations/${exec.conversation_id}`}
                        className={styles.historyLink}
                        title="View full conversation"
                      >
                        View output <ExternalLink size={10} />
                      </a>
                    )}
                  </div>
                </div>
                {exec.result_summary && (
                  <div className={styles.historySummary}>{exec.result_summary}</div>
                )}
                {exec.error_message && (
                  <div className={styles.historyError}>{exec.error_message}</div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </Card>
  )
}

// ── Main Page ────────────────────────────────────────────────────

export function WebhooksPage() {
  const [webhooks, setWebhooks] = useState<Webhook[]>([])
  const [agents, setAgents] = useState<Agent[]>([])
  const [loading, setLoading] = useState(true)
  const [showModal, setShowModal] = useState(false)
  const [editingWebhook, setEditingWebhook] = useState<Webhook | null>(null)

  const agentMap = new Map(agents.map(a => [a.id, a.name]))

  const loadData = useCallback(async () => {
    try {
      const [w, a] = await Promise.all([
        api.webhooks.list(),
        api.agents.list(),
      ])
      setWebhooks(w)
      setAgents(a)
    } catch (err) {
      console.error('Failed to load webhooks:', err)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  async function handleToggle(webhook: Webhook) {
    try {
      const updated = await api.webhooks.patch(webhook.id, { enabled: !webhook.enabled })
      setWebhooks(ws => ws.map(w => w.id === webhook.id ? updated : w))
    } catch (err) {
      console.error('Failed to toggle webhook:', err)
    }
  }

  async function handleDelete(id: string) {
    try {
      await api.webhooks.delete(id)
      setWebhooks(ws => ws.filter(w => w.id !== id))
    } catch (err) {
      console.error('Failed to delete webhook:', err)
    }
  }

  function handleSaved(saved: Webhook) {
    setWebhooks(ws => {
      const exists = ws.find(w => w.id === saved.id)
      if (exists) return ws.map(w => w.id === saved.id ? saved : w)
      return [saved, ...ws]
    })
    setShowModal(false)
    setEditingWebhook(null)
  }

  if (loading) {
    return (
      <>
        <PageHeader title="Webhooks" description="HTTP endpoints that trigger agent conversations." />
        <div className={styles.loadingRow}>
          <Loader2 size={14} style={{ animation: 'spin 1s linear infinite' }} /> Loading...
        </div>
      </>
    )
  }

  return (
    <>
      <PageHeader
        title="Webhooks"
        description="Create HTTP endpoints that trigger agent conversations from external services."
        actions={
          <Button onClick={() => { setEditingWebhook(null); setShowModal(true) }}>
            <Plus size={14} /> New Webhook
          </Button>
        }
      />

      {webhooks.length === 0 ? (
        <div className={styles.emptyState}>
          <WebhookIcon size={32} className={styles.emptyIcon} />
          <div className={styles.emptyTitle}>No webhooks</div>
          <div className={styles.emptyDesc}>
            Create a webhook to trigger an agent conversation from an external HTTP request.
          </div>
          <Button onClick={() => setShowModal(true)}>
            <Plus size={14} /> Create Webhook
          </Button>
        </div>
      ) : (
        <div className={styles.webhookGrid}>
          {webhooks.map(webhook => (
            <WebhookCard
              key={webhook.id}
              webhook={webhook}
              agentName={agentMap.get(webhook.agent_id) ?? 'Unknown agent'}
              onToggle={() => handleToggle(webhook)}
              onEdit={() => { setEditingWebhook(webhook); setShowModal(true) }}
              onDelete={() => handleDelete(webhook.id)}
            />
          ))}
        </div>
      )}

      {showModal && (
        <WebhookModal
          webhook={editingWebhook}
          agents={agents}
          onClose={() => { setShowModal(false); setEditingWebhook(null) }}
          onSaved={handleSaved}
        />
      )}
    </>
  )
}
