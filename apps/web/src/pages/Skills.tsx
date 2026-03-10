import { useState, useEffect, useCallback } from 'react'
import {
  Zap, Plus, Search, Trash2, Pencil, Clock, Loader2, Bot, BookTemplate, ChevronDown,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import { api, type Skill, type SkillTemplate } from '../lib/api'
import styles from './Skills.module.css'

// ── Template Picker ───────────────────────────────────────────

interface TemplatePickerProps {
  templates: SkillTemplate[]
  onSelect: (t: SkillTemplate) => void
}

function TemplatePicker({ templates, onSelect }: TemplatePickerProps) {
  const [open, setOpen] = useState(false)

  if (templates.length === 0) return null

  return (
    <div className={styles.templatePicker}>
      <button
        type="button"
        className={styles.templateToggle}
        onClick={() => setOpen(!open)}
      >
        <BookTemplate size={13} />
        Use a template
        <ChevronDown size={12} className={open ? styles.chevronOpen : ''} />
      </button>
      {open && (
        <div className={styles.templateGrid}>
          {templates.map(t => (
            <button
              key={t.name}
              type="button"
              className={styles.templateCard}
              onClick={() => { onSelect(t); setOpen(false) }}
            >
              <span className={styles.templateName}>/{t.name}</span>
              <span className={styles.templateDesc}>{t.description}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

// ── Create / Edit Form ────────────────────────────────────────

interface SkillFormProps {
  initial?: Skill
  templates: SkillTemplate[]
  onSave: (skill: Skill) => void
  onCancel: () => void
}

function SkillForm({ initial, templates, onSave, onCancel }: SkillFormProps) {
  const [name, setName] = useState(initial?.name ?? '')
  const [description, setDescription] = useState(initial?.description ?? '')
  const [instructions, setInstructions] = useState(initial?.instructions ?? '')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')

  const isEdit = !!initial

  const applyTemplate = (t: SkillTemplate) => {
    setName(t.name)
    setDescription(t.description)
    setInstructions(t.instructions)
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    const trimmedName = name.trim().toLowerCase()
    if (!trimmedName) { setError('Name is required.'); return }
    if (!/^[a-z0-9-]+$/.test(trimmedName)) {
      setError('Name must only contain lowercase letters, numbers, and hyphens.')
      return
    }
    if (trimmedName.length > 64) { setError('Name must be 64 characters or less.'); return }
    if (!description.trim()) { setError('Description is required.'); return }

    setError('')
    setSubmitting(true)
    try {
      const skill = isEdit
        ? await api.skills.patch(initial!.id, {
            name: trimmedName,
            description: description.trim(),
            instructions: instructions.trim(),
          })
        : await api.skills.create({
            name: trimmedName,
            description: description.trim(),
            instructions: instructions.trim(),
          })
      onSave(skill)
    } catch (err) {
      setError(String(err))
    } finally {
      setSubmitting(false)
    }
  }

  const content = (
    <form onSubmit={handleSubmit}>
      <h4 className={styles.formTitle}>{isEdit ? 'Edit Skill' : 'Create Skill'}</h4>

      {!isEdit && <TemplatePicker templates={templates} onSelect={applyTemplate} />}

      <div className={styles.formGroup}>
        <label className={styles.label}>Name</label>
        <input
          className={styles.input}
          value={name}
          onChange={e => setName(e.target.value)}
          placeholder="my-skill-name"
          autoFocus
        />
        <p className={styles.formHint}>Lowercase, hyphens only. Users invoke with /{name || 'skill-name'}</p>
      </div>
      <div className={styles.formGroup}>
        <label className={styles.label}>Description</label>
        <input
          className={styles.input}
          value={description}
          onChange={e => setDescription(e.target.value)}
          placeholder="Brief description of what this skill does"
        />
        <p className={styles.formHint}>Shown to the LLM so it knows when to use this skill.</p>
      </div>
      <div className={styles.formGroup}>
        <label className={styles.label}>Instructions</label>
        <textarea
          className={`${styles.input} ${styles.textarea}`}
          value={instructions}
          onChange={e => setInstructions(e.target.value)}
          placeholder="Step-by-step instructions for the agent to follow when this skill is invoked..."
          rows={8}
        />
        <p className={styles.formHint}>Injected into the conversation when /skill-name is used.</p>
      </div>
      {error && <p style={{ color: 'var(--danger)', fontSize: 13, marginBottom: 8 }}>{error}</p>}
      <div className={styles.formActions}>
        <Button variant="secondary" size="sm" type="button" onClick={onCancel}>Cancel</Button>
        <Button variant="primary" size="sm" type="submit" disabled={submitting}>
          {submitting && <Loader2 size={13} className="spinning" />}
          {isEdit ? 'Save Changes' : 'Create Skill'}
        </Button>
      </div>
    </form>
  )

  if (isEdit) {
    return (
      <div className={styles.overlay} onClick={onCancel}>
        <div className={styles.panel} onClick={e => e.stopPropagation()}>
          <div className={styles.panelHeader}>
            <span className={styles.panelTitle}>Edit Skill</span>
            <button className={styles.panelClose} onClick={onCancel}>&#x2715;</button>
          </div>
          <div className={styles.panelBody}>{content}</div>
        </div>
      </div>
    )
  }

  return <Card className={styles.formCard}>{content}</Card>
}

// ── Page ──────────────────────────────────────────────────────

export function SkillsPage() {
  const [skills, setSkills] = useState<Skill[]>([])
  const [templates, setTemplates] = useState<SkillTemplate[]>([])
  const [search, setSearch] = useState('')
  const [loading, setLoading] = useState(true)
  const [showCreate, setShowCreate] = useState(false)
  const [editing, setEditing] = useState<Skill | null>(null)

  const loadSkills = useCallback(async () => {
    try {
      const [sk, tpl] = await Promise.all([
        api.skills.list(),
        api.skills.templates(),
      ])
      setSkills(sk)
      setTemplates(tpl)
    } catch { /* */ }
    setLoading(false)
  }, [])

  useEffect(() => { loadSkills() }, [loadSkills])

  const handleCreate = (skill: Skill) => {
    setSkills(prev => [skill, ...prev])
    setShowCreate(false)
  }

  const handleUpdate = (skill: Skill) => {
    setSkills(prev => prev.map(s => s.id === skill.id ? skill : s))
    setEditing(null)
  }

  const handleDelete = async (id: string) => {
    try {
      await api.skills.delete(id)
      setSkills(prev => prev.filter(s => s.id !== id))
    } catch { /* */ }
  }

  const filtered = skills.filter(s =>
    s.name.toLowerCase().includes(search.toLowerCase()) ||
    s.description.toLowerCase().includes(search.toLowerCase())
  )

  return (
    <div className="fade-in">
      <PageHeader
        title="Skills"
        description="Reusable prompt modules that agents can invoke with /skill-name syntax."
        actions={
          !showCreate ? (
            <Button onClick={() => setShowCreate(true)}>
              <Plus size={15} /> New Skill
            </Button>
          ) : undefined
        }
      />

      {/* Syntax hint */}
      <div className={styles.syntaxHint}>
        <Zap size={14} />
        <span>
          Skills are invoked with <code className={styles.syntaxCode}>/skill-name</code> in conversations.
          Link skills to agents in the Dashboard agent config panel.
        </span>
      </div>

      {/* Create form */}
      {showCreate && (
        <SkillForm templates={templates} onSave={handleCreate} onCancel={() => setShowCreate(false)} />
      )}

      {/* Search */}
      {skills.length > 0 && (
        <div className={styles.searchBar}>
          <Search size={15} />
          <input
            type="text"
            placeholder="Search skills..."
            className={styles.searchInput}
            value={search}
            onChange={e => setSearch(e.target.value)}
          />
        </div>
      )}

      {loading ? (
        <div className={styles.loadingRow}><Loader2 size={15} className="spinning" /> Loading skills...</div>
      ) : filtered.length === 0 && !showCreate ? (
        <div className={styles.emptyState}>
          <Zap size={36} strokeWidth={1} />
          <p className={styles.emptyTitle}>{search ? 'No skills match your search' : 'No skills yet'}</p>
          <p className={styles.emptyDesc}>
            Skills package reusable instructions that agents can load on demand.
            Create your first skill to get started.
          </p>
          {!search && (
            <Button variant="primary" size="sm" onClick={() => setShowCreate(true)}>
              <Plus size={13} /> Create Skill
            </Button>
          )}
        </div>
      ) : (
        <div className={`${styles.list} stagger`}>
          {filtered.map(skill => (
            <Card key={skill.id}>
              <div className={styles.skillRow}>
                <div className={styles.skillIcon}>
                  <Zap size={16} strokeWidth={1.5} />
                </div>
                <div className={styles.skillInfo}>
                  <div className={styles.skillNameRow}>
                    <code className={styles.skillName}>/{skill.name}</code>
                    <div className={styles.skillActions}>
                      <button
                        className={styles.iconBtn}
                        onClick={() => setEditing(skill)}
                        title="Edit"
                      >
                        <Pencil size={14} />
                      </button>
                      <button
                        className={`${styles.iconBtn} ${styles.iconBtnDanger}`}
                        onClick={() => handleDelete(skill.id)}
                        title="Delete"
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>
                  <p className={styles.skillDesc}>{skill.description}</p>
                  <div className={styles.skillMeta}>
                    <span>
                      <Clock size={10} /> {relativeTime(skill.updated_at)}
                    </span>
                    {skill.instructions.length > 0 && (
                      <span>
                        <Bot size={10} /> {skill.instructions.length} chars
                      </span>
                    )}
                  </div>
                </div>
              </div>
            </Card>
          ))}
        </div>
      )}

      {/* Edit modal */}
      {editing && (
        <SkillForm
          initial={editing}
          templates={templates}
          onSave={handleUpdate}
          onCancel={() => setEditing(null)}
        />
      )}
    </div>
  )
}

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const m = Math.floor(diff / 60000)
  if (m < 1) return 'just now'
  if (m < 60) return `${m}m ago`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h ago`
  return `${Math.floor(h / 24)}d ago`
}
