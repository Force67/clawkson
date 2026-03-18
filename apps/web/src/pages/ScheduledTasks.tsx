import { useState, useEffect, useCallback, useRef } from 'react'
import {
  Plus, Play, Pencil, Trash2, Timer, ToggleLeft, ToggleRight,
  Loader2, Clock, ChevronDown, ChevronUp, X, ExternalLink, Zap,
  Download, FileText,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import {
  api,
  type Agent,
  type AgentSkillInfo,
  type ScheduledTask,
  type TaskExecution,
  type CreateScheduledTaskRequest,
  type PatchScheduledTaskRequest,
} from '../lib/api'
import styles from './ScheduledTasks.module.css'

// ── Schedule presets ─────────────────────────────────────────────────

interface Preset {
  label: string
  value: string | null
}

const PRESETS: Preset[] = [
  { label: 'Manual only', value: null },
  { label: 'Every hour', value: '0 0 * * * *' },
  { label: 'Every 6h', value: '0 0 */6 * * *' },
  { label: 'Daily 9 AM', value: '0 0 9 * * *' },
  { label: 'Mon 9 AM', value: '0 0 9 * * MON' },
  { label: 'Custom', value: '__custom__' },
]

function cronToHuman(cron: string | null | undefined): string {
  if (!cron) return 'Manual only'
  const preset = PRESETS.find(p => p.value === cron)
  if (preset) return preset.label
  return cron
}

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

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

// ── Task Modal ───────────────────────────────────────────────────────

interface TaskModalProps {
  task: ScheduledTask | null
  agents: Agent[]
  onClose: () => void
  onSaved: (t: ScheduledTask) => void
}

function TaskModal({ task, agents, onClose, onSaved }: TaskModalProps) {
  const [name, setName] = useState(task?.name ?? '')
  const [agentId, setAgentId] = useState(task?.agent_id ?? (agents[0]?.id ?? ''))
  const [prompt, setPrompt] = useState(task?.prompt ?? '')
  const [selectedPreset, setSelectedPreset] = useState<string | null>(() => {
    if (!task?.cron_expression) return null
    const match = PRESETS.find(p => p.value === task.cron_expression)
    return match ? match.value : '__custom__'
  })
  const [customCron, setCustomCron] = useState(task?.cron_expression ?? '')
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)

  // Skill autocomplete state
  const [agentSkills, setAgentSkills] = useState<AgentSkillInfo[]>([])
  const [showSkillDropdown, setShowSkillDropdown] = useState(false)
  const [slashFilter, setSlashFilter] = useState('')
  const [skillDropdownIndex, setSkillDropdownIndex] = useState(0)
  const promptRef = useRef<HTMLTextAreaElement>(null)
  const promptWrapRef = useRef<HTMLDivElement>(null)

  const effectiveAgentId = task?.agent_id ?? agentId
  const effectiveCron = selectedPreset === '__custom__' ? customCron : selectedPreset

  // Load skills when agent changes
  useEffect(() => {
    if (!effectiveAgentId) { setAgentSkills([]); return }
    api.agentSkills.full(effectiveAgentId)
      .then(setAgentSkills)
      .catch(() => setAgentSkills([]))
  }, [effectiveAgentId])

  // Skill detection in prompt textarea
  const handlePromptChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = e.target.value
    setPrompt(val)

    const cursorPos = e.target.selectionStart ?? val.length
    const textBeforeCursor = val.slice(0, cursorPos)
    const slashMatch = textBeforeCursor.match(/(?:^|\s)(\/[a-z0-9-]*)$/)

    if (slashMatch && agentSkills.length > 0) {
      setSlashFilter(slashMatch[1].slice(1))
      setSkillDropdownIndex(0)
      setShowSkillDropdown(true)
    } else {
      setShowSkillDropdown(false)
    }
  }, [agentSkills])

  const handleSkillSelect = useCallback((skill: AgentSkillInfo) => {
    const cursorPos = promptRef.current?.selectionStart ?? prompt.length
    const before = prompt.slice(0, cursorPos)
    const after = prompt.slice(cursorPos)

    const replaced = before.replace(/(?:^|\s)(\/[a-z0-9-]*)$/, (match) => {
      const prefix = match.startsWith('/') ? '' : match[0]
      return `${prefix}/${skill.name}`
    })

    const newVal = replaced + (after.startsWith(' ') ? after : ' ' + after)
    setPrompt(newVal.trimEnd() + ' ')
    setShowSkillDropdown(false)
    promptRef.current?.focus()
  }, [prompt])

  const handlePromptKeyDown = (e: React.KeyboardEvent) => {
    if (showSkillDropdown) {
      const filtered = agentSkills.filter(s =>
        s.name.toLowerCase().includes(slashFilter.toLowerCase())
      )
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setSkillDropdownIndex(i => Math.min(i + 1, filtered.length - 1))
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setSkillDropdownIndex(i => Math.max(i - 1, 0))
        return
      }
      if ((e.key === 'Tab' || e.key === 'Enter') && filtered.length > 0) {
        e.preventDefault()
        handleSkillSelect(filtered[skillDropdownIndex])
        return
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        setShowSkillDropdown(false)
        return
      }
    }
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setError(null)
    if (!name.trim()) { setError('Name is required.'); return }
    if (!agentId && !task) { setError('Please select an agent.'); return }
    if (!prompt.trim()) { setError('Prompt is required.'); return }

    setSaving(true)
    try {
      if (task) {
        const body: PatchScheduledTaskRequest = {}
        if (name !== task.name) body.name = name.trim()
        if (prompt !== task.prompt) body.prompt = prompt.trim()
        if (effectiveCron !== task.cron_expression) body.cron_expression = effectiveCron
        const updated = await api.scheduledTasks.patch(task.id, body)
        onSaved(updated)
      } else {
        const body: CreateScheduledTaskRequest = {
          name: name.trim(),
          agent_id: agentId,
          prompt: prompt.trim(),
          cron_expression: effectiveCron,
        }
        const created = await api.scheduledTasks.create(body)
        onSaved(created)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save')
    } finally {
      setSaving(false)
    }
  }

  const filteredSkills = agentSkills.filter(s =>
    s.name.toLowerCase().includes(slashFilter.toLowerCase())
  )

  return (
    <div className={styles.modalOverlay} onClick={onClose}>
      <div className={styles.modal} onClick={e => e.stopPropagation()}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
          <div className={styles.modalTitle}>{task ? 'Edit Task' : 'New Scheduled Task'}</div>
          <button className={styles.actionBtn} onClick={onClose}><X size={16} /></button>
        </div>

        <form className={styles.form} onSubmit={handleSubmit}>
          <label className={styles.fieldLabel}>
            Name
            <input
              className={styles.input}
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="Daily report"
            />
          </label>

          {!task && (
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
            Prompt
            <div ref={promptWrapRef} className={styles.promptWrap}>
              <textarea
                ref={promptRef}
                className={styles.textarea}
                value={prompt}
                onChange={handlePromptChange}
                onKeyDown={handlePromptKeyDown}
                placeholder={agentSkills.length > 0 ? 'Type a prompt or / for skills...' : 'Generate the daily status report...'}
                rows={4}
              />
              {showSkillDropdown && filteredSkills.length > 0 && (
                <div className={styles.skillDropdown}>
                  <div className={styles.skillDropdownHeader}>
                    <Zap size={11} />
                    <span>Skills</span>
                  </div>
                  <div className={styles.skillDropdownList}>
                    {filteredSkills.map((skill, i) => (
                      <button
                        key={skill.id}
                        type="button"
                        className={`${styles.skillDropdownItem} ${i === skillDropdownIndex ? styles.skillDropdownItemActive : ''}`}
                        onMouseDown={e => { e.preventDefault(); handleSkillSelect(skill) }}
                      >
                        <div className={styles.skillDropdownIcon}>
                          <Zap size={12} />
                        </div>
                        <div className={styles.skillDropdownContent}>
                          <span className={styles.skillDropdownName}>/{skill.name}</span>
                          <span className={styles.skillDropdownDesc}>{skill.description}</span>
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              )}
              <div className={styles.promptHints}>
                Use <span className={styles.promptHintTag}>@toolname</span> for tools{agentSkills.length > 0 ? <>{' '}or <span className={styles.promptHintTag}>/skill</span> to invoke skills</> : null}
              </div>
            </div>
          </label>

          <label className={styles.fieldLabel}>
            Schedule
            <div className={styles.presetGrid}>
              {PRESETS.map(p => (
                <button
                  key={p.label}
                  type="button"
                  className={`${styles.presetBtn} ${
                    selectedPreset === p.value ? styles.presetBtnActive : ''
                  }`}
                  onClick={() => setSelectedPreset(p.value)}
                >
                  {p.label}
                </button>
              ))}
            </div>
          </label>

          {selectedPreset === '__custom__' && (
            <label className={styles.fieldLabel}>
              Cron Expression
              <input
                className={styles.input}
                value={customCron}
                onChange={e => setCustomCron(e.target.value)}
                placeholder="0 0 9 * * *"
              />
              <span className={styles.hint}>7-field format: sec min hour day month weekday</span>
            </label>
          )}

          {error && <div className={styles.errorMsg}>{error}</div>}

          <div className={styles.modalActions}>
            <Button variant="ghost" type="button" onClick={onClose}>Cancel</Button>
            <Button type="submit" disabled={saving}>
              {saving ? <Loader2 size={14} style={{ animation: 'spin 1s linear infinite' }} /> : null}
              {task ? 'Save Changes' : 'Create Task'}
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── Task Card ────────────────────────────────────────────────────────

interface TaskCardProps {
  task: ScheduledTask
  agentName: string
  createdByAgentName?: string
  onToggle: () => void
  onEdit: () => void
  onDelete: () => void
  onRunNow: () => Promise<TaskExecution | null>
  onTaskUpdated: (t: ScheduledTask) => void
}

function TaskCard({ task, agentName, createdByAgentName, onToggle, onEdit, onDelete, onRunNow, onTaskUpdated }: TaskCardProps) {
  const [showHistory, setShowHistory] = useState(false)
  const [history, setHistory] = useState<TaskExecution[]>([])
  const [loadingHistory, setLoadingHistory] = useState(false)
  const [running, setRunning] = useState(false)
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)

  // Cleanup polling on unmount
  useEffect(() => {
    return () => { if (pollRef.current) clearInterval(pollRef.current) }
  }, [])

  async function loadHistory() {
    setLoadingHistory(true)
    try {
      const h = await api.scheduledTasks.history(task.id)
      setHistory(h)
      // Check if any are still running
      const hasRunning = h.some(e => e.status === 'running')
      if (!hasRunning && pollRef.current) {
        clearInterval(pollRef.current)
        pollRef.current = null
        setRunning(false)
        // Refresh task to get updated last_run_at
        try {
          const updated = await api.scheduledTasks.get(task.id)
          onTaskUpdated(updated)
        } catch { /* ignore */ }
      }
    } catch { /* ignore */ }
    setLoadingHistory(false)
  }

  async function toggleHistory() {
    if (!showHistory) {
      await loadHistory()
    }
    setShowHistory(prev => !prev)
  }

  async function handleRun() {
    setRunning(true)
    const exec = await onRunNow()
    if (exec) {
      // Add the running execution to history immediately
      setHistory(prev => [exec, ...prev])
      setShowHistory(true)
      // Poll every 3s until done
      pollRef.current = setInterval(() => loadHistory(), 3000)
    } else {
      setRunning(false)
    }
  }

  return (
    <Card>
      <div className={styles.taskHeader}>
        <div className={styles.taskName}>{task.name}</div>
        <div className={styles.taskActions}>
          <button
            className={styles.actionBtn}
            onClick={handleRun}
            title="Run now"
            disabled={running}
          >
            {running
              ? <Loader2 size={14} style={{ animation: 'spin 1s linear infinite' }} />
              : <Play size={14} />}
          </button>
          <button className={styles.actionBtn} onClick={onEdit} title="Edit">
            <Pencil size={14} />
          </button>
          <button className={styles.toggle} onClick={onToggle} title={task.enabled ? 'Disable' : 'Enable'}>
            {task.enabled
              ? <ToggleRight size={20} className={styles.toggleOn} />
              : <ToggleLeft size={20} className={styles.toggleOff} />}
          </button>
          <button className={`${styles.actionBtn} ${styles.deleteBtn}`} onClick={onDelete} title="Delete">
            <Trash2 size={14} />
          </button>
        </div>
      </div>

      <div className={styles.taskAgent}>
        {agentName}
        {task.created_by_agent_id && (
          <span className={styles.provenanceBadge} title={createdByAgentName ? `Workflow created by ${createdByAgentName}` : 'Created by an agent workflow'}>
            <Zap size={10} />
            {createdByAgentName ? `via ${createdByAgentName}` : 'Workflow'}
          </span>
        )}
        {task.created_by_conversation_id && (
          <a
            href={`/conversations/${task.created_by_conversation_id}`}
            className={styles.provenanceLink}
            title="View the conversation that created this task"
          >
            <ExternalLink size={9} /> Source
          </a>
        )}
      </div>
      <div className={styles.taskPrompt}>{task.prompt}</div>

      <div className={styles.taskMeta}>
        <div className={styles.metaItem}>
          <Clock size={11} />
          {cronToHuman(task.cron_expression)}
        </div>
        {task.last_run_at && (
          <div className={styles.metaItem}>
            Last: {timeAgo(task.last_run_at)}
          </div>
        )}
        {task.next_run_at && (
          <div className={styles.metaItem}>
            Next: {timeAgo(task.next_run_at)}
          </div>
        )}
      </div>

      {/* Execution history */}
      <div className={styles.historySection}>
        <button className={styles.historyToggle} onClick={toggleHistory}>
          {showHistory ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
          Recent runs
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
                {exec.output_files && exec.output_files.length > 0 && (
                  <div className={styles.outputFiles}>
                    {exec.output_files.map(file => (
                      <a
                        key={file.id}
                        href={api.uploads.downloadUrl(file.id)}
                        className={styles.outputFile}
                        title={`${file.filename} (${formatFileSize(file.size_bytes)})`}
                      >
                        <FileText size={11} />
                        <span className={styles.outputFileName}>{file.filename}</span>
                        <span className={styles.outputFileSize}>{formatFileSize(file.size_bytes)}</span>
                        <Download size={10} />
                      </a>
                    ))}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </Card>
  )
}

// ── Main Page ────────────────────────────────────────────────────────

export function ScheduledTasksPage() {
  const [tasks, setTasks] = useState<ScheduledTask[]>([])
  const [agents, setAgents] = useState<Agent[]>([])
  const [loading, setLoading] = useState(true)
  const [showModal, setShowModal] = useState(false)
  const [editingTask, setEditingTask] = useState<ScheduledTask | null>(null)

  const agentMap = new Map(agents.map(a => [a.id, a.name]))

  const loadData = useCallback(async () => {
    try {
      const [t, a] = await Promise.all([
        api.scheduledTasks.list(),
        api.agents.list(),
      ])
      setTasks(t)
      setAgents(a)
    } catch (err) {
      console.error('Failed to load scheduled tasks:', err)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  async function handleToggle(task: ScheduledTask) {
    try {
      const updated = await api.scheduledTasks.patch(task.id, { enabled: !task.enabled })
      setTasks(ts => ts.map(t => t.id === task.id ? updated : t))
    } catch (err) {
      console.error('Failed to toggle task:', err)
    }
  }

  async function handleDelete(id: string) {
    try {
      await api.scheduledTasks.delete(id)
      setTasks(ts => ts.filter(t => t.id !== id))
    } catch (err) {
      console.error('Failed to delete task:', err)
    }
  }

  async function handleRunNow(task: ScheduledTask): Promise<TaskExecution | null> {
    try {
      return await api.scheduledTasks.run(task.id)
    } catch (err) {
      console.error('Failed to run task:', err)
      return null
    }
  }

  function handleSaved(saved: ScheduledTask) {
    setTasks(ts => {
      const exists = ts.find(t => t.id === saved.id)
      if (exists) return ts.map(t => t.id === saved.id ? saved : t)
      return [saved, ...ts]
    })
    setShowModal(false)
    setEditingTask(null)
  }

  if (loading) {
    return (
      <>
        <PageHeader title="Scheduled Tasks" description="Recurring agent jobs on a cron cadence." />
        <div className={styles.loadingRow}>
          <Loader2 size={14} style={{ animation: 'spin 1s linear infinite' }} /> Loading...
        </div>
      </>
    )
  }

  return (
    <>
      <PageHeader
        title="Scheduled Tasks"
        description="Create saved tasks that run automatically on a schedule."
        actions={
          <Button onClick={() => { setEditingTask(null); setShowModal(true) }}>
            <Plus size={14} /> New Task
          </Button>
        }
      />

      {tasks.length === 0 ? (
        <div className={styles.emptyState}>
          <Timer size={32} className={styles.emptyIcon} />
          <div className={styles.emptyTitle}>No scheduled tasks</div>
          <div className={styles.emptyDesc}>
            Create a task to run an agent on demand or automatically on a cron schedule.
          </div>
          <Button onClick={() => setShowModal(true)}>
            <Plus size={14} /> Create Task
          </Button>
        </div>
      ) : (
        <>
          {/* Workflow-created tasks */}
          {(() => {
            const workflowTasks = tasks.filter(t => t.created_by_agent_id)
            const manualTasks = tasks.filter(t => !t.created_by_agent_id)

            const renderCard = (task: ScheduledTask) => (
              <TaskCard
                key={task.id}
                task={task}
                agentName={agentMap.get(task.agent_id) ?? 'Unknown agent'}
                createdByAgentName={task.created_by_agent_id ? agentMap.get(task.created_by_agent_id) : undefined}
                onToggle={() => handleToggle(task)}
                onEdit={() => { setEditingTask(task); setShowModal(true) }}
                onDelete={() => handleDelete(task.id)}
                onRunNow={() => handleRunNow(task)}
                onTaskUpdated={(updated) => setTasks(ts => ts.map(t => t.id === updated.id ? updated : t))}
              />
            )

            return (
              <>
                {workflowTasks.length > 0 && (
                  <>
                    <div className={styles.sectionLabel}>
                      <Zap size={11} /> Workflow Tasks
                    </div>
                    <div className={styles.taskGrid}>
                      {workflowTasks.map(renderCard)}
                    </div>
                  </>
                )}
                {manualTasks.length > 0 && (
                  <>
                    {workflowTasks.length > 0 && (
                      <div className={styles.sectionLabel}>
                        <Timer size={11} /> Manual Tasks
                      </div>
                    )}
                    <div className={styles.taskGrid}>
                      {manualTasks.map(renderCard)}
                    </div>
                  </>
                )}
              </>
            )
          })()}
        </>
      )}

      {showModal && (
        <TaskModal
          task={editingTask}
          agents={agents}
          onClose={() => { setShowModal(false); setEditingTask(null) }}
          onSaved={handleSaved}
        />
      )}
    </>
  )
}
