import { useState, useEffect, useCallback } from 'react'
import {
  KeyRound, Plus, Search, Trash2, Pencil, Clock, Loader2, X,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Button } from '../components/Button'
import { api, type Credential, type CredentialType } from '../lib/api'
import styles from './Credentials.module.css'

const CREDENTIAL_TYPES: { value: CredentialType; label: string }[] = [
  { value: 'api_key', label: 'API Key' },
  { value: 'password', label: 'Password' },
  { value: 'token', label: 'Token' },
  { value: 'secret', label: 'Secret' },
  { value: 'header', label: 'Custom Header' },
]

function formatDate(iso: string) {
  return new Date(iso).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' })
}

// ── Create / Edit Modal ────────────────────────────────────────

interface CredentialFormProps {
  initial?: Credential | null
  onSave: () => void
  onClose: () => void
}

function CredentialForm({ initial, onSave, onClose }: CredentialFormProps) {
  const [name, setName] = useState(initial?.name ?? '')
  const [description, setDescription] = useState(initial?.description ?? '')
  const [credType, setCredType] = useState<CredentialType>(
    (initial?.credential_type as CredentialType) ?? 'api_key'
  )
  const [value, setValue] = useState('')
  const [headerName, setHeaderName] = useState(
    (initial?.metadata?.header_name as string) ?? ''
  )
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  const isEdit = !!initial

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setError('')

    if (!name.trim()) { setError('Name is required'); return }
    if (!isEdit && !value.trim()) { setError('Value is required'); return }
    if (credType === 'header' && !headerName.trim()) { setError('Header name is required'); return }

    setSaving(true)
    try {
      const metadata: Record<string, unknown> = {}
      if (credType === 'header' && headerName.trim()) {
        metadata.header_name = headerName.trim()
      }

      if (isEdit) {
        await api.credentials.patch(initial.id, {
          name: name.trim().toLowerCase(),
          description: description.trim(),
          credential_type: credType,
          ...(value.trim() ? { value: value.trim() } : {}),
          metadata,
        })
      } else {
        await api.credentials.create({
          name: name.trim().toLowerCase(),
          description: description.trim(),
          credential_type: credType,
          value: value.trim(),
          metadata,
        })
      }
      onSave()
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to save credential')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.panel} onClick={e => e.stopPropagation()}>
        <div className={styles.panelHeader}>
          <span className={styles.panelTitle}>{isEdit ? 'Edit Credential' : 'New Credential'}</span>
          <button className={styles.panelClose} onClick={onClose}><X size={16} /></button>
        </div>
        <div className={styles.panelBody}>
          <form onSubmit={handleSubmit}>
            <div className={styles.formGroup}>
              <label className={styles.label}>Name</label>
              <input
                className={styles.input}
                value={name}
                onChange={e => setName(e.target.value)}
                placeholder="stripe-api-key"
                autoFocus
              />
              <div className={styles.formHint}>Lowercase, hyphens and underscores only</div>
            </div>

            <div className={styles.formGroup}>
              <label className={styles.label}>Description</label>
              <input
                className={styles.input}
                value={description}
                onChange={e => setDescription(e.target.value)}
                placeholder="Stripe production API key"
              />
            </div>

            <div className={styles.formGroup}>
              <label className={styles.label}>Type</label>
              <select
                className={styles.select}
                value={credType}
                onChange={e => setCredType(e.target.value as CredentialType)}
              >
                {CREDENTIAL_TYPES.map(t => (
                  <option key={t.value} value={t.value}>{t.label}</option>
                ))}
              </select>
            </div>

            {credType === 'header' && (
              <div className={styles.formGroup}>
                <label className={styles.label}>Header Name</label>
                <input
                  className={styles.input}
                  value={headerName}
                  onChange={e => setHeaderName(e.target.value)}
                  placeholder="X-Custom-Auth"
                />
              </div>
            )}

            <div className={styles.formGroup}>
              <label className={styles.label}>{isEdit ? 'Value (leave blank to keep current)' : 'Value'}</label>
              <input
                className={styles.input}
                type="password"
                value={value}
                onChange={e => setValue(e.target.value)}
                placeholder={isEdit ? '(unchanged)' : 'sk_live_...'}
                autoComplete="new-password"
              />
            </div>

            {error && (
              <div style={{ color: 'var(--error)', fontSize: 13, marginBottom: 12 }}>{error}</div>
            )}

            <div className={styles.formActions}>
              <Button variant="ghost" type="button" onClick={onClose}>Cancel</Button>
              <Button type="submit" disabled={saving}>
                {saving ? <Loader2 size={14} className="spin" /> : isEdit ? 'Save' : 'Create'}
              </Button>
            </div>
          </form>
        </div>
      </div>
    </div>
  )
}

// ── Main Page ──────────────────────────────────────────────────

export function CredentialsPage() {
  const [credentials, setCredentials] = useState<Credential[]>([])
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')
  const [showForm, setShowForm] = useState(false)
  const [editing, setEditing] = useState<Credential | null>(null)

  const load = useCallback(async () => {
    try {
      setCredentials(await api.credentials.list())
    } catch { /* ignore */ }
    setLoading(false)
  }, [])

  useEffect(() => { load() }, [load])

  const filtered = credentials.filter(c =>
    c.name.toLowerCase().includes(search.toLowerCase()) ||
    c.description.toLowerCase().includes(search.toLowerCase())
  )

  const handleDelete = async (id: string) => {
    if (!confirm('Delete this credential? Any agents using it will lose access.')) return
    try {
      await api.credentials.delete(id)
      load()
    } catch { /* ignore */ }
  }

  const handleSave = () => {
    setShowForm(false)
    setEditing(null)
    load()
  }

  return (
    <>
      <PageHeader
        title="Credentials"
        description="Named secrets that agents can use without seeing the values"
        actions={
          <Button onClick={() => { setEditing(null); setShowForm(true) }}>
            <Plus size={14} /> New Credential
          </Button>
        }
      />

      {/* Search */}
      <div className={styles.searchBar}>
        <Search size={15} />
        <input
          className={styles.searchInput}
          placeholder="Search credentials..."
          value={search}
          onChange={e => setSearch(e.target.value)}
        />
      </div>

      {loading ? (
        <div className={styles.loadingRow}><Loader2 size={16} className="spin" /> Loading...</div>
      ) : filtered.length === 0 ? (
        <div className={styles.emptyState}>
          <KeyRound size={40} />
          <div className={styles.emptyTitle}>
            {search ? 'No matches' : 'No credentials yet'}
          </div>
          <div className={styles.emptyDesc}>
            {search
              ? 'Try a different search term.'
              : 'Create a credential to securely give agents access to third-party APIs without exposing secret values.'}
          </div>
          {!search && (
            <Button onClick={() => { setEditing(null); setShowForm(true) }}>
              <Plus size={14} /> New Credential
            </Button>
          )}
        </div>
      ) : (
        <div className={styles.list}>
          {filtered.map(cred => (
            <div key={cred.id} className={styles.credRow}>
              <div className={styles.credIcon}>
                <KeyRound size={18} />
              </div>
              <div className={styles.credInfo}>
                <div className={styles.credNameRow}>
                  <span className={styles.credName}>{cred.name}</span>
                  <div className={styles.credActions}>
                    <button
                      className={styles.iconBtn}
                      title="Edit"
                      onClick={() => { setEditing(cred); setShowForm(true) }}
                    >
                      <Pencil size={14} />
                    </button>
                    <button
                      className={`${styles.iconBtn} ${styles.iconBtnDanger}`}
                      title="Delete"
                      onClick={() => handleDelete(cred.id)}
                    >
                      <Trash2 size={14} />
                    </button>
                  </div>
                </div>
                {cred.description && <div className={styles.credDesc}>{cred.description}</div>}
                <div className={styles.credMeta}>
                  <span className={styles.typeBadge}>{cred.credential_type}</span>
                  <Clock size={11} />
                  <span>{formatDate(cred.created_at)}</span>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {showForm && (
        <CredentialForm
          initial={editing}
          onSave={handleSave}
          onClose={() => { setShowForm(false); setEditing(null) }}
        />
      )}
    </>
  )
}
