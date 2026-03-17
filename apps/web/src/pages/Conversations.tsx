import { useState, useEffect, useRef, useCallback } from 'react'
import { Plus, Search, Send, Bot, MessageSquare, ChevronRight, ChevronDown, X, Loader2, Brain, Paperclip, SlidersHorizontal, Globe, File as FileIcon, Image as ImageIcon, FileText, Trash2, Eraser, Zap, Download, Share2, UserPlus, Shield, Eye, Pencil, Pin, AlertTriangle, WifiOff, Check, Terminal, FolderOpen, Wrench, Maximize2, Minimize2, GitBranch, Upload } from 'lucide-react'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import { Button } from '../components/Button'
import { EmptyState } from '../components/EmptyState'
import { api, streamChat, type Agent, type Conversation, type Message, type ReasoningEffort, type AgentSkillInfo, type ShareResponse, type SharePermission, type ToolEvent, type PreviewInfo, type Tool } from '../lib/api'
import styles from './Conversations.module.css'

// ── Folder drag-and-drop traversal ──────────────────────────────

const IGNORED_FILES = new Set(['.DS_Store', 'Thumbs.db', 'desktop.ini', '.gitkeep'])

async function readAllEntries(reader: FileSystemDirectoryReader): Promise<FileSystemEntry[]> {
  const all: FileSystemEntry[] = []
  let batch: FileSystemEntry[]
  do {
    batch = await new Promise<FileSystemEntry[]>((resolve, reject) =>
      reader.readEntries(resolve, reject)
    )
    all.push(...batch)
  } while (batch.length > 0)
  return all
}

async function traverseEntry(entry: FileSystemEntry): Promise<File[]> {
  if (entry.isFile) {
    const file = await new Promise<File>((resolve, reject) =>
      (entry as FileSystemFileEntry).file(resolve, reject)
    )
    if (IGNORED_FILES.has(file.name) || file.name.startsWith('.')) return []
    return [file]
  }
  if (entry.isDirectory) {
    const reader = (entry as FileSystemDirectoryEntry).createReader()
    const entries = await readAllEntries(reader)
    const nested = await Promise.all(entries.map(traverseEntry))
    return nested.flat()
  }
  return []
}

async function getFilesFromDataTransfer(dataTransfer: DataTransfer): Promise<File[]> {
  const items = Array.from(dataTransfer.items)
  const entries = items
    .map(item => item.webkitGetAsEntry?.())
    .filter((e): e is FileSystemEntry => e !== null && e !== undefined)

  if (entries.length > 0) {
    const nested = await Promise.all(entries.map(traverseEntry))
    return nested.flat()
  }
  // Fallback: plain file list (no folder support)
  return Array.from(dataTransfer.files)
}

// ── Markdown renderer ───────────────────────────────────────────

function MarkdownContent({ content }: { content: string }) {
  return (
    <Markdown
      remarkPlugins={[remarkGfm]}
      rehypePlugins={[rehypeHighlight]}
      components={{
        a: ({ href, children, ...props }) => (
          <a href={href} target="_blank" rel="noopener noreferrer" {...props}>{children}</a>
        ),
      }}
    >
      {content}
    </Markdown>
  )
}

// ── New Conversation Dialog ───────────────────────────────────────

interface NewConvoDialogProps {
  agents: Agent[]
  onClose: () => void
  onCreate: (conv: Conversation) => void
}

function NewConvoDialog({ agents, onClose, onCreate }: NewConvoDialogProps) {
  const [title, setTitle] = useState('')
  const [agentId, setAgentId] = useState(agents[0]?.id ?? '')
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!title.trim() || !agentId) return
    setSubmitting(true)
    try {
      const conv = await api.conversations.create({ title: title.trim(), agent_id: agentId })
      onCreate(conv)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className={styles.dialogOverlay} onClick={onClose}>
      <div className={styles.dialog} onClick={e => e.stopPropagation()}>
        <div className={styles.dialogHeader}>
          <h3 className={styles.dialogTitle}>New Conversation</h3>
          <button className={styles.dialogClose} onClick={onClose}><X size={16} /></button>
        </div>
        <form onSubmit={handleSubmit}>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Title</label>
            <input
              className={styles.formInput}
              value={title}
              onChange={e => setTitle(e.target.value)}
              placeholder="What are we working on?"
              autoFocus
            />
          </div>
          <div className={styles.formGroup}>
            <label className={styles.formLabel}>Agent</label>
            {agents.length === 0 ? (
              <p className={styles.formHint}>No agents configured. Create one in Dashboard first.</p>
            ) : (
              <div className={styles.agentPicker}>
                {agents.map(a => (
                  <button
                    key={a.id}
                    type="button"
                    className={`${styles.agentOption} ${agentId === a.id ? styles.agentOptionSelected : ''}`}
                    onClick={() => setAgentId(a.id)}
                  >
                    <div className={styles.agentOptionIcon}><Bot size={14} /></div>
                    <span>{a.name}</span>
                    {agentId === a.id && <ChevronRight size={12} className={styles.agentOptionCheck} />}
                  </button>
                ))}
              </div>
            )}
          </div>
          <div className={styles.dialogActions}>
            <Button variant="secondary" size="sm" type="button" onClick={onClose}>Cancel</Button>
            <Button variant="primary" size="sm" type="submit" disabled={submitting || !title.trim() || agents.length === 0}>
              {submitting ? <Loader2 size={14} className={styles.spinning} /> : null}
              Start Chat
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}

// ── Message bubble ────────────────────────────────────────────────

interface MsgBubbleProps {
  msg: Message
  agentName?: string
}

function AttachmentIcon({ contentType }: { contentType: string }) {
  if (contentType.startsWith('image/')) return <ImageIcon size={11} />
  if (contentType === 'application/pdf') return <FileText size={11} />
  return <FileIcon size={11} />
}

function ArtifactPreview({ id, filename }: { id: string; filename: string }) {
  const [expanded, setExpanded] = useState(false)
  return (
    <div className={styles.artifactFrame}>
      <div className={styles.artifactHeader}>
        <FileText size={12} />
        <span className={styles.artifactHeaderName}>{filename}</span>
        <a
          href={api.uploads.downloadUrl(id)}
          download={filename}
          className={styles.artifactDownload}
          title="Download"
          onClick={e => e.stopPropagation()}
        >
          <Download size={12} />
        </a>
      </div>
      <iframe
        src={api.uploads.downloadUrl(id)}
        sandbox="allow-scripts"
        className={`${styles.artifactIframe} ${expanded ? styles.artifactIframeExpanded : ''}`}
        title={filename}
      />
      <button
        className={styles.artifactToggle}
        onClick={() => setExpanded(!expanded)}
        type="button"
      >
        {expanded ? <><Minimize2 size={10} /> Collapse</> : <><Maximize2 size={10} /> Expand</>}
      </button>
    </div>
  )
}

function MsgBubble({ msg, agentName }: MsgBubbleProps) {
  const isUser = msg.role === 'user'
  const attachments = msg.attachments ?? []
  const imageAttachments = attachments.filter(a => a.content_type.startsWith('image/'))
  const htmlAttachments = isUser ? [] : attachments.filter(a =>
    a.content_type === 'text/html' || a.filename.endsWith('.html') || a.filename.endsWith('.htm')
  )
  const htmlIds = new Set(htmlAttachments.map(a => a.id))
  const fileAttachments = attachments.filter(a => !a.content_type.startsWith('image/') && !htmlIds.has(a.id))

  return (
    <div className={`${styles.messageRow} ${isUser ? styles.messageRowUser : styles.messageRowAssistant}`}>
      <div className={`${styles.bubble} ${isUser ? styles.bubbleUser : styles.bubbleAssistant}`}>
        {!isUser && (
          <div className={styles.bubbleMeta}>
            <div className={styles.bubbleAvatar}><Bot size={12} /></div>
            <span className={styles.bubbleAuthor}>{agentName ?? 'Assistant'}</span>
          </div>
        )}
        <div className={`${styles.bubbleContent} ${isUser ? styles.bubbleContentUser : styles.bubbleContentAssistant}`}>
          {isUser ? msg.content : (
            <div className={styles.markdown}>
              <MarkdownContent content={msg.content} />
            </div>
          )}
        </div>
        {imageAttachments.length > 0 && (
          <div className={styles.msgImages}>
            {imageAttachments.map(att => (
              <a key={att.id} href={api.uploads.downloadUrl(att.id)} target="_blank" rel="noopener noreferrer" className={styles.msgImageLink}>
                <img
                  src={api.uploads.downloadUrl(att.id)}
                  alt={att.filename}
                  className={styles.msgImage}
                  loading="lazy"
                />
                <span className={styles.msgImageCaption}>
                  <Download size={10} />
                  {att.filename}
                </span>
              </a>
            ))}
          </div>
        )}
        {htmlAttachments.length > 0 && (
          <div className={styles.artifactPreview}>
            {htmlAttachments.map(att => (
              <ArtifactPreview key={att.id} id={att.id} filename={att.filename} />
            ))}
          </div>
        )}
        {fileAttachments.length > 0 && (
          <div className={styles.msgAttachments}>
            {fileAttachments.map(att => (
              <a
                key={att.id}
                href={api.uploads.downloadUrl(att.id)}
                download={att.filename}
                className={`${styles.msgAttachmentChip} ${isUser ? styles.msgAttachmentUser : styles.msgAttachmentAssistant}`}
                title={`${att.filename} (${formatFileSize(att.size_bytes)})`}
              >
                <AttachmentIcon contentType={att.content_type} />
                <span className={styles.msgAttachmentName}>{att.filename}</span>
                {!isUser && <Download size={10} className={styles.msgAttachmentDl} />}
              </a>
            ))}
          </div>
        )}
        <span className={styles.bubbleTime}>
          {new Date(msg.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
        </span>
      </div>
    </div>
  )
}

// ── Reasoning block (collapsible thinking trace) ─────────────

interface ReasoningBlockProps {
  content: string
  isStreaming?: boolean
}

function ReasoningBlock({ content, isStreaming }: ReasoningBlockProps) {
  const [expanded, setExpanded] = useState(false)

  if (!content) return null

  return (
    <div className={styles.reasoningBlock}>
      <button
        className={`${styles.reasoningToggle} ${expanded ? styles.reasoningToggleOpen : ''}`}
        onClick={() => setExpanded(!expanded)}
        type="button"
      >
        <ChevronRight size={12} />
        <Brain size={12} />
        Thinking{isStreaming ? '...' : ''}
        {isStreaming && (
          <span className={styles.reasoningStreamIndicator}>
            <span className={styles.reasoningDotPulse} />
          </span>
        )}
      </button>
      {expanded && (
        <div className={styles.reasoningContent}>
          {content}
        </div>
      )}
    </div>
  )
}

// ── Activity Feed (live tool execution preview) ──────────────────

interface ActivityStep {
  id: string
  name: string
  round: number
  status: 'running' | 'done' | 'error'
  description: string
  result?: string
  durationMs?: number
  /** Whether this step is a sub-task (from delegate_tasks) */
  isSubtask?: boolean
}

const TOOL_NAME_MAP: Record<string, string> = {
  code_execution: 'Code Execution',
  workspace_read: 'Read File',
  workspace_write: 'Write File',
  workspace_list: 'List Files',
  knowledge_search: 'Knowledge Search',
  delegate_tasks: 'Sub-Agent Coordination',
}

function formatToolName(name: string): string {
  return TOOL_NAME_MAP[name] || name.split('_').map(w => w[0].toUpperCase() + w.slice(1)).join(' ')
}

function ToolTypeIcon({ name, isSubtask }: { name: string; isSubtask?: boolean }) {
  const s = 11
  if (isSubtask) return <GitBranch size={s} />
  switch (name) {
    case 'code_execution': return <Terminal size={s} />
    case 'workspace_read': return <FileText size={s} />
    case 'workspace_write': return <Pencil size={s} />
    case 'workspace_list': return <FolderOpen size={s} />
    case 'knowledge_search': return <Search size={s} />
    case 'delegate_tasks': return <GitBranch size={s} />
    default: return <Wrench size={s} />
  }
}

function ActivityFeed({ steps }: { steps: ActivityStep[] }) {
  if (steps.length === 0) return null

  const toolSteps = steps.filter(s => !s.isSubtask)
  const subtaskSteps = steps.filter(s => s.isSubtask)
  const runningCount = steps.filter(s => s.status === 'running').length
  const completedCount = steps.filter(s => s.status !== 'running').length
  const hasSubtasks = subtaskSteps.length > 0
  const subtaskRunning = subtaskSteps.filter(s => s.status === 'running').length
  const subtaskCompleted = subtaskSteps.filter(s => s.status !== 'running').length

  return (
    <div className={styles.activityFeed}>
      {toolSteps.length > 0 && (
        <>
          <div className={styles.activityHeader}>
            {runningCount > 0 && <span className={styles.activityPulse} />}
            <span>
              {runningCount > 0
                ? `Working\u2002·\u2002${completedCount}/${steps.length} tools`
                : `${completedCount} tool${completedCount !== 1 ? 's' : ''} executed`}
            </span>
          </div>
          <div className={styles.activitySteps}>
            {toolSteps.map(step => (
              <div key={step.id} className={styles.activityStep}>
                <div className={`${styles.activityIcon} ${
                  step.status === 'running' ? styles.activityIconRunning
                  : step.status === 'done' ? styles.activityIconDone
                  : styles.activityIconError
                }`}>
                  {step.status === 'running'
                    ? <div className={styles.activitySpinner} />
                    : step.status === 'done'
                    ? <Check size={11} />
                    : <X size={11} />}
                </div>
                <div className={styles.activityInfo}>
                  <div className={styles.activityName}>
                    <ToolTypeIcon name={step.name} />
                    {formatToolName(step.name)}
                  </div>
                  <div className={styles.activityDesc}>
                    {step.status === 'running'
                      ? step.description
                      : step.result || step.description}
                  </div>
                </div>
                {step.durationMs != null && (
                  <span className={styles.activityDuration}>
                    {step.durationMs < 1000
                      ? `${step.durationMs}ms`
                      : `${(step.durationMs / 1000).toFixed(1)}s`}
                  </span>
                )}
              </div>
            ))}
          </div>
        </>
      )}
      {hasSubtasks && (
        <>
          <div className={`${styles.activityHeader} ${styles.subtaskHeader}`}>
            <GitBranch size={12} />
            {subtaskRunning > 0 && <span className={styles.activityPulse} />}
            <span>
              {subtaskRunning > 0
                ? `Sub-tasks\u2002·\u2002${subtaskCompleted}/${subtaskSteps.length} completed`
                : `${subtaskCompleted} sub-task${subtaskCompleted !== 1 ? 's' : ''} completed`}
            </span>
          </div>
          <div className={styles.activitySteps}>
            {subtaskSteps.map(step => (
              <div key={step.id} className={`${styles.activityStep} ${styles.subtaskStep}`}>
                <div className={`${styles.activityIcon} ${
                  step.status === 'running' ? styles.activityIconRunning
                  : step.status === 'done' ? styles.activityIconDone
                  : styles.activityIconError
                }`}>
                  {step.status === 'running'
                    ? <div className={styles.activitySpinner} />
                    : step.status === 'done'
                    ? <Check size={11} />
                    : <X size={11} />}
                </div>
                <div className={styles.activityInfo}>
                  <div className={styles.activityName}>
                    <ToolTypeIcon name={step.name} isSubtask />
                    {step.description}
                  </div>
                  {step.result && step.status !== 'running' && (
                    <div className={styles.activityDesc}>{step.result}</div>
                  )}
                </div>
                {step.durationMs != null && (
                  <span className={styles.activityDuration}>
                    {step.durationMs < 1000
                      ? `${step.durationMs}ms`
                      : `${(step.durationMs / 1000).toFixed(1)}s`}
                  </span>
                )}
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  )
}

// ── Live Preview (container web server) ──────────────────────────

function LivePreview({ preview }: { preview: PreviewInfo }) {
  const [expanded, setExpanded] = useState(false)
  return (
    <div className={styles.artifactFrame}>
      <div className={styles.artifactHeader}>
        <Globe size={12} />
        <span className={styles.artifactHeaderName}>{preview.title}</span>
        <span className={styles.previewPort}>:{preview.port}</span>
        <a
          href={preview.preview_url}
          target="_blank"
          rel="noopener noreferrer"
          className={styles.artifactDownload}
          title="Open in new tab"
        >
          <Maximize2 size={12} />
        </a>
      </div>
      <iframe
        src={preview.preview_url}
        sandbox="allow-scripts allow-forms allow-popups allow-downloads"
        className={`${styles.artifactIframe} ${expanded ? styles.artifactIframeExpanded : ''}`}
        title={preview.title}
      />
      <button
        className={styles.artifactToggle}
        onClick={() => setExpanded(!expanded)}
        type="button"
      >
        {expanded ? <><Minimize2 size={10} /> Collapse</> : <><Maximize2 size={10} /> Expand</>}
      </button>
    </div>
  )
}

// ── Skill Slash Command Dropdown ──────────────────────────────────

interface SkillDropdownProps {
  skills: AgentSkillInfo[]
  filter: string
  selectedIndex: number
  onSelect: (skill: AgentSkillInfo) => void
  position: { bottom: number; left: number }
}

function SkillDropdown({ skills, filter, selectedIndex, onSelect, position }: SkillDropdownProps) {
  const filtered = skills.filter(s =>
    s.name.toLowerCase().includes(filter.toLowerCase())
  )

  if (filtered.length === 0) return null

  return (
    <div
      className={styles.skillDropdown}
      style={{ bottom: position.bottom, left: position.left }}
    >
      <div className={styles.skillDropdownHeader}>
        <Zap size={11} />
        <span>Skills</span>
      </div>
      <div className={styles.skillDropdownList}>
        {filtered.map((skill, i) => (
          <button
            key={skill.id}
            type="button"
            className={`${styles.skillDropdownItem} ${i === selectedIndex ? styles.skillDropdownItemActive : ''}`}
            onMouseDown={e => { e.preventDefault(); onSelect(skill) }}
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
  )
}

// ── Tool Dropdown (@mention) ──────────────────────────────────────

interface ToolDropdownProps {
  tools: Tool[]
  filter: string
  selectedIndex: number
  onSelect: (tool: Tool) => void
  position: { bottom: number; left: number }
}

function ToolDropdown({ tools, filter, selectedIndex, onSelect, position }: ToolDropdownProps) {
  const filtered = tools.filter(t =>
    t.name.toLowerCase().includes(filter.toLowerCase())
  )

  if (filtered.length === 0) return null

  return (
    <div
      className={styles.skillDropdown}
      style={{ bottom: position.bottom, left: position.left }}
    >
      <div className={styles.skillDropdownHeader}>
        <Wrench size={11} />
        <span>Tools</span>
      </div>
      <div className={styles.skillDropdownList}>
        {filtered.map((tool, i) => (
          <button
            key={tool.id}
            type="button"
            className={`${styles.skillDropdownItem} ${i === selectedIndex ? styles.skillDropdownItemActive : ''}`}
            onMouseDown={e => { e.preventDefault(); onSelect(tool) }}
          >
            <div className={`${styles.skillDropdownIcon} ${styles.toolDropdownIcon}`}>
              <Wrench size={12} />
            </div>
            <div className={styles.skillDropdownContent}>
              <span className={styles.skillDropdownName}>@{tool.name}</span>
              <span className={styles.skillDropdownDesc}>{tool.description}</span>
            </div>
            {tool.tool_type === 'connector' && (
              <span className={styles.toolBadge}>connector</span>
            )}
          </button>
        ))}
      </div>
    </div>
  )
}

// ── Share Dialog ──────────────────────────────────────────────────

interface ShareDialogProps {
  conversationId: string
  onClose: () => void
}

function ShareDialog({ conversationId, onClose }: ShareDialogProps) {
  const [shares, setShares] = useState<ShareResponse[]>([])
  const [loading, setLoading] = useState(true)
  const [email, setEmail] = useState('')
  const [permission, setPermission] = useState<SharePermission>('read')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')
  const [removingId, setRemovingId] = useState<string | null>(null)

  useEffect(() => {
    api.shares.list(conversationId)
      .then(setShares)
      .catch(() => setShares([]))
      .finally(() => setLoading(false))
  }, [conversationId])

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault()
    const trimmed = email.trim()
    if (!trimmed) return
    setSubmitting(true)
    setError('')
    try {
      const share = await api.shares.create(conversationId, trimmed, permission)
      setShares(prev => [...prev, share])
      setEmail('')
    } catch (err: any) {
      setError(err.message ?? 'Failed to share')
    } finally {
      setSubmitting(false)
    }
  }

  const handleRemove = async (userId: string) => {
    setRemovingId(userId)
    try {
      await api.shares.remove(conversationId, userId)
      setShares(prev => prev.filter(s => s.shared_with_user.id !== userId))
    } catch (err: any) {
      setError(err.message ?? 'Failed to remove')
    } finally {
      setRemovingId(null)
    }
  }

  return (
    <div className={styles.dialogOverlay} onClick={onClose}>
      <div className={styles.dialog} onClick={e => e.stopPropagation()}>
        <div className={styles.dialogHeader}>
          <h3 className={styles.dialogTitle}>Share Conversation</h3>
          <button className={styles.dialogClose} onClick={onClose}><X size={16} /></button>
        </div>

        <form onSubmit={handleAdd} className={styles.shareForm}>
          <div className={styles.shareInputRow}>
            <input
              className={styles.formInput}
              value={email}
              onChange={e => { setEmail(e.target.value); setError('') }}
              placeholder="Email address"
              type="email"
              autoFocus
            />
            <div className={styles.sharePermToggle}>
              <button
                type="button"
                className={`${styles.permBtn} ${permission === 'read' ? styles.permBtnActive : ''}`}
                onClick={() => setPermission('read')}
                title="Read only"
              >
                <Eye size={12} />
              </button>
              <button
                type="button"
                className={`${styles.permBtn} ${permission === 'write' ? styles.permBtnActive : ''}`}
                onClick={() => setPermission('write')}
                title="Can write"
              >
                <Pencil size={12} />
              </button>
            </div>
            <Button variant="primary" size="sm" type="submit" disabled={submitting || !email.trim()}>
              {submitting ? <Loader2 size={14} className={styles.spinning} /> : <UserPlus size={14} />}
              Share
            </Button>
          </div>
          {error && <p className={styles.shareError}>{error}</p>}
        </form>

        <div className={styles.shareList}>
          {loading ? (
            <div className={styles.shareEmpty}>
              <Loader2 size={14} className={styles.spinning} />
              <span>Loading...</span>
            </div>
          ) : shares.length === 0 ? (
            <div className={styles.shareEmpty}>
              <Shield size={14} />
              <span>Not shared with anyone yet</span>
            </div>
          ) : (
            shares.map(s => (
              <div key={s.share.id} className={styles.shareRow}>
                <div className={styles.shareUser}>
                  <div className={styles.shareAvatar}>
                    {s.shared_with_user.display_name?.charAt(0)?.toUpperCase() || s.shared_with_user.email.charAt(0).toUpperCase()}
                  </div>
                  <div className={styles.shareUserInfo}>
                    <span className={styles.shareUserName}>
                      {s.shared_with_user.display_name || s.shared_with_user.email}
                    </span>
                    <span className={styles.shareUserEmail}>{s.shared_with_user.email}</span>
                  </div>
                </div>
                <div className={styles.shareRowActions}>
                  <span className={`${styles.permBadge} ${s.share.permission === 'write' ? styles.permBadgeWrite : ''}`}>
                    {s.share.permission === 'write' ? <><Pencil size={10} /> Write</> : <><Eye size={10} /> Read</>}
                  </span>
                  <button
                    className={styles.shareRemoveBtn}
                    onClick={() => handleRemove(s.shared_with_user.id)}
                    disabled={removingId === s.shared_with_user.id}
                    title="Remove access"
                    type="button"
                  >
                    {removingId === s.shared_with_user.id
                      ? <Loader2 size={12} className={styles.spinning} />
                      : <X size={12} />
                    }
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  )
}

// ── Main page ─────────────────────────────────────────────────────

export function ConversationsPage() {
  const [conversations, setConversations] = useState<Conversation[]>([])
  const [agents, setAgents] = useState<Agent[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [messages, setMessages] = useState<Message[]>([])
  const [input, setInput] = useState('')
  const [streaming, setStreaming] = useState(false)
  const [streamBuffer, setStreamBuffer] = useState('')
  const [reasoningBuffer, setReasoningBuffer] = useState('')
  const [showNewDialog, setShowNewDialog] = useState(false)
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')
  const [reasoningEnabled, setReasoningEnabled] = useState(true)
  const [reasoningEffort, setReasoningEffort] = useState<ReasoningEffort>('medium')
  const [searchEnabled, setSearchEnabled] = useState(true)
  const [pendingFiles, setPendingFiles] = useState<File[]>([])
  const [uploading, setUploading] = useState(false)
  const [uploadProgress, setUploadProgress] = useState(0) // 0-100
  const [dragOver, setDragOver] = useState(false)
  const [filesExpanded, setFilesExpanded] = useState(false)
  const [clearingMessages, setClearingMessages] = useState(false)
  const [showDeleteAllConfirm, setShowDeleteAllConfirm] = useState(false)
  const [deletingAll, setDeletingAll] = useState(false)
  const [agentSkills, setAgentSkills] = useState<AgentSkillInfo[]>([])
  const [showSkillDropdown, setShowSkillDropdown] = useState(false)
  const [slashFilter, setSlashFilter] = useState('')
  const [skillDropdownIndex, setSkillDropdownIndex] = useState(0)
  const [userTools, setUserTools] = useState<import('../lib/api').Tool[]>([])
  const [showToolDropdown, setShowToolDropdown] = useState(false)
  const [atFilter, setAtFilter] = useState('')
  const [toolDropdownIndex, setToolDropdownIndex] = useState(0)
  const [showShareDialog, setShowShareDialog] = useState(false)
  const [activitySteps, setActivitySteps] = useState<ActivityStep[]>([])
  const [livePreview, setLivePreview] = useState<PreviewInfo | null>(null)

  const messagesEndRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLTextAreaElement>(null)
  const stopStreamRef = useRef<(() => void) | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const folderInputRef = useRef<HTMLInputElement>(null)
  const inputDockRef = useRef<HTMLDivElement>(null)
  const dragCounterRef = useRef(0)

  const selectedConvo = conversations.find(c => c.id === selectedId)
  const selectedAgent = agents.find(a => a.id === selectedConvo?.agent_id)

  const filtered = conversations.filter(c =>
    c.title.toLowerCase().includes(search.toLowerCase()) ||
    agents.find(a => a.id === c.agent_id)?.name.toLowerCase().includes(search.toLowerCase())
  )
  const groupedConversations = groupConversations(filtered)

  // Load conversations and agents
  useEffect(() => {
    Promise.all([api.conversations.list(), api.agents.list()])
      .then(([convos, agts]) => {
        setConversations(convos.sort((a, b) => {
          if (a.pinned !== b.pinned) return b.pinned ? 1 : -1
          return b.updated_at.localeCompare(a.updated_at)
        }))
        setAgents(agts)
        if (convos.length > 0) setSelectedId(convos[0].id)
      })
      .finally(() => setLoading(false))
  }, [])

  // Load messages when conversation changes
  useEffect(() => {
    if (!selectedId) return
    setMessages([])
    api.conversations.messages(selectedId).then(setMessages)
  }, [selectedId])

  // Load agent skills when the selected agent changes
  useEffect(() => {
    if (!selectedAgent) { setAgentSkills([]); return }
    api.agentSkills.full(selectedAgent.id)
      .then(setAgentSkills)
      .catch(() => setAgentSkills([]))
  }, [selectedAgent?.id])

  // Load available tools (user-level)
  useEffect(() => {
    api.tools.list()
      .then(setUserTools)
      .catch(() => setUserTools([]))
  }, [])

  // Scroll to bottom on new messages or stream buffer changes
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages, streamBuffer, reasoningBuffer, activitySteps])

  useEffect(() => {
    const el = inputRef.current
    if (!el) return
    el.style.height = '0px'
    el.style.height = `${Math.min(el.scrollHeight, 180)}px`
  }, [input, selectedId])

  const sendMessage = useCallback(async () => {
    const content = input.trim()
    if (!content || !selectedId || streaming || uploading) return

    setInput('')
    setStreaming(true)
    setStreamBuffer('')
    setReasoningBuffer('')
    setActivitySteps([])
    setLivePreview(null)

    // Upload pending files first
    let attachmentIds: string[] = []
    const filesToUpload = [...pendingFiles]
    setPendingFiles([])
    setFilesExpanded(false)

    if (filesToUpload.length > 0) {
      setUploading(true)
      setUploadProgress(0)
      try {
        const result = await api.uploads.upload(filesToUpload, selectedId, (loaded, total) => {
          setUploadProgress(Math.round((loaded / total) * 100))
        })
        attachmentIds = result.files.map(f => f.id)
      } catch (err) {
        console.error('File upload failed:', err)
        // Continue sending the message without attachments
      } finally {
        setUploading(false)
        setUploadProgress(0)
      }
    }

    // Optimistically show user message
    const tempUserMsg: Message = {
      id: `tmp-${Date.now()}`,
      conversation_id: selectedId,
      role: 'user',
      content,
      created_at: new Date().toISOString(),
    }
    setMessages(prev => [...prev, tempUserMsg])

    const options = {
      reasoning_effort: reasoningEnabled ? reasoningEffort : undefined,
      search_enabled: searchEnabled,
      attachment_ids: attachmentIds.length > 0 ? attachmentIds : undefined,
    }

    // Try streaming first, fall back to non-streaming
    const stop = streamChat(
      selectedId,
      content,
      (delta) => {
        setStreamBuffer(prev => prev + delta)
      },
      () => {
        // Reload messages from server to get the persisted version
        api.conversations.messages(selectedId).then(msgs => {
          setMessages(msgs)
          setStreamBuffer('')
          setReasoningBuffer('')
          setActivitySteps([])
          setStreaming(false)
          // Refresh conversation list (updated_at changed)
          api.conversations.list().then(convos =>
            setConversations(convos.sort((a, b) => {
          if (a.pinned !== b.pinned) return b.pinned ? 1 : -1
          return b.updated_at.localeCompare(a.updated_at)
        }))
          )
        })
      },
      (err) => {
        console.error('Stream error, falling back to non-streaming:', err)
        // Fall back to non-streaming chat
        setStreamBuffer('')
        setReasoningBuffer('')
        setActivitySteps([])
        api.conversations.chat(selectedId, content)
          .then(({ user_message, assistant_message }) => {
            setMessages(prev => {
              const without = prev.filter(m => m.id !== tempUserMsg.id)
              return [...without, user_message, assistant_message]
            })
          })
          .catch(console.error)
          .finally(() => setStreaming(false))
      },
      (reasoning) => {
        setReasoningBuffer(prev => prev + reasoning)
      },
      options,
      (event: ToolEvent) => {
        if (event.type === 'tool_start') {
          setActivitySteps(prev => [...prev, {
            id: `step-${prev.length}`,
            name: event.name,
            round: event.round,
            status: 'running',
            description: event.description || event.name,
          }])
        } else if (event.type === 'tool_end') {
          setActivitySteps(prev => {
            // Find the last running step with this name
            const reversed = [...prev].reverse()
            const idx = reversed.findIndex(s => s.name === event.name && s.status === 'running')
            if (idx === -1) return prev
            const realIdx = prev.length - 1 - idx
            return prev.map((s, i) => i === realIdx ? {
              ...s,
              status: (event.ok ? 'done' : 'error') as ActivityStep['status'],
              result: event.result,
              durationMs: event.duration_ms,
            } : s)
          })
          // Detect start_preview tool and extract preview info
          if (event.name === 'start_preview' && event.ok && event.result) {
            try {
              const data = JSON.parse(event.result)
              if (data.preview_url) {
                setLivePreview({
                  preview_url: data.preview_url,
                  port: data.port,
                  title: data.title || 'Live Preview',
                })
              }
            } catch { /* not JSON */ }
          }
        } else if (event.type === 'subtask_start') {
          setActivitySteps(prev => [...prev, {
            id: `subtask-${event.id}`,
            name: event.name || 'subtask',
            round: event.round || 0,
            status: 'running',
            description: event.description || event.id || 'Sub-task',
            isSubtask: true,
          }])
        } else if (event.type === 'subtask_end') {
          setActivitySteps(prev => {
            const idx = prev.findIndex(s => s.id === `subtask-${event.id}`)
            if (idx === -1) return prev
            return prev.map((s, i) => i === idx ? {
              ...s,
              status: (event.ok ? 'done' : 'error') as ActivityStep['status'],
              result: event.result,
              durationMs: event.duration_ms,
            } : s)
          })
        }
      },
    )
    stopStreamRef.current = stop
  }, [input, selectedId, streaming, uploading, reasoningEnabled, reasoningEffort, searchEnabled, pendingFiles])

  const handleInputChange = useCallback((e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = e.target.value
    setInput(val)

    const cursorPos = e.target.selectionStart ?? val.length
    const textBeforeCursor = val.slice(0, cursorPos)

    // Detect slash command: look for `/` preceded by start-of-string or whitespace
    const slashMatch = textBeforeCursor.match(/(?:^|\s)(\/[a-z0-9-]*)$/)
    if (slashMatch && agentSkills.length > 0) {
      const query = slashMatch[1].slice(1) // remove the leading /
      setSlashFilter(query)
      setSkillDropdownIndex(0)
      setShowSkillDropdown(true)
      setShowToolDropdown(false)
      return
    } else {
      setShowSkillDropdown(false)
    }

    // Detect @tool mention: look for `@` preceded by start-of-string or whitespace
    const atMatch = textBeforeCursor.match(/(?:^|\s)(@[a-z0-9_:.-]*)$/i)
    if (atMatch && userTools.length > 0) {
      const query = atMatch[1].slice(1) // remove the leading @
      setAtFilter(query)
      setToolDropdownIndex(0)
      setShowToolDropdown(true)
    } else {
      setShowToolDropdown(false)
    }
  }, [agentSkills, userTools])

  const handleSkillSelect = useCallback((skill: AgentSkillInfo) => {
    const cursorPos = inputRef.current?.selectionStart ?? input.length
    const textBeforeCursor = input.slice(0, cursorPos)
    const textAfterCursor = input.slice(cursorPos)

    // Replace the partial /slash with the full skill name
    const replaced = textBeforeCursor.replace(/(?:^|\s)(\/[a-z0-9-]*)$/, (match) => {
      const prefix = match.startsWith('/') ? '' : match[0] // keep the whitespace prefix
      return `${prefix}/${skill.name}`
    })

    const newVal = replaced + (textAfterCursor.startsWith(' ') ? textAfterCursor : ' ' + textAfterCursor)
    setInput(newVal.trimEnd() + ' ')
    setShowSkillDropdown(false)
    inputRef.current?.focus()
  }, [input])

  const handleToolSelect = useCallback((tool: Tool) => {
    const cursorPos = inputRef.current?.selectionStart ?? input.length
    const textBeforeCursor = input.slice(0, cursorPos)
    const textAfterCursor = input.slice(cursorPos)

    // Replace the partial @text with the full tool name
    const replaced = textBeforeCursor.replace(/(?:^|\s)(@[a-z0-9_:.-]*)$/i, (match) => {
      const prefix = match.startsWith('@') ? '' : match[0] // keep the whitespace prefix
      return `${prefix}@${tool.name}`
    })

    const newVal = replaced + (textAfterCursor.startsWith(' ') ? textAfterCursor : ' ' + textAfterCursor)
    setInput(newVal.trimEnd() + ' ')
    setShowToolDropdown(false)
    inputRef.current?.focus()
  }, [input])

  const handleKeyDown = (e: React.KeyboardEvent) => {
    // Handle tool dropdown navigation
    if (showToolDropdown) {
      const filtered = userTools.filter(t =>
        t.name.toLowerCase().includes(atFilter.toLowerCase())
      )
      if (e.key === 'ArrowDown') {
        e.preventDefault()
        setToolDropdownIndex(i => Math.min(i + 1, filtered.length - 1))
        return
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault()
        setToolDropdownIndex(i => Math.max(i - 1, 0))
        return
      }
      if ((e.key === 'Tab' || e.key === 'Enter') && filtered.length > 0) {
        e.preventDefault()
        handleToolSelect(filtered[toolDropdownIndex])
        return
      }
      if (e.key === 'Escape') {
        e.preventDefault()
        setShowToolDropdown(false)
        return
      }
    }

    // Handle skill dropdown navigation
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

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault()
      sendMessage()
    }
  }

  const handleNewConvo = (conv: Conversation) => {
    setConversations(prev => [conv, ...prev])
    setSelectedId(conv.id)
    setShowNewDialog(false)
    setMessages([])
  }

  const handleFilePick = () => {
    fileInputRef.current?.click()
  }

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? []).filter(
      f => !IGNORED_FILES.has(f.name) && !f.name.startsWith('.')
    )
    if (files.length > 0) {
      setPendingFiles(prev => [...prev, ...files])
      if (files.length > 4) setFilesExpanded(true)
    }
    // Reset so the same file can be re-selected
    e.target.value = ''
  }

  const handleFolderPick = () => {
    folderInputRef.current?.click()
  }

  const removePendingFile = (index: number) => {
    setPendingFiles(prev => prev.filter((_, i) => i !== index))
  }

  const clearAllPendingFiles = () => {
    setPendingFiles([])
    setFilesExpanded(false)
  }

  const handleDragEnter = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
    dragCounterRef.current++
    if (e.dataTransfer.types.includes('Files')) setDragOver(true)
  }

  const handleDragLeave = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
    dragCounterRef.current--
    if (dragCounterRef.current === 0) setDragOver(false)
  }

  const handleDragOver = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
  }

  const handleDrop = async (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault()
    e.stopPropagation()
    dragCounterRef.current = 0
    setDragOver(false)
    const files = await getFilesFromDataTransfer(e.dataTransfer)
    if (files.length > 0) {
      setPendingFiles(prev => [...prev, ...files])
      // Auto-expand if dropping many files so user sees what was picked up
      if (files.length > 4) setFilesExpanded(true)
    }
  }

  const handleClearMessages = useCallback(async () => {
    if (!selectedId || clearingMessages || streaming) return
    setClearingMessages(true)
    try {
      await api.conversations.clearMessages(selectedId)
      setMessages([])
      setStreamBuffer('')
      setReasoningBuffer('')
    } catch (err) {
      console.error('Failed to clear messages:', err)
    } finally {
      setClearingMessages(false)
    }
  }, [selectedId, clearingMessages, streaming])

  const handleDeleteAll = useCallback(async () => {
    setDeletingAll(true)
    try {
      await api.conversations.deleteAll()
      setConversations([])
      setSelectedId(null)
      setMessages([])
      setStreamBuffer('')
      setReasoningBuffer('')
    } catch (err) {
      console.error('Failed to delete all conversations:', err)
    } finally {
      setDeletingAll(false)
      setShowDeleteAllConfirm(false)
    }
  }, [])

  if (loading) {
    return (
      <div className={`fade-in ${styles.container}`}>
        <div className={styles.loadingState}>
          <div className={styles.loadingPanel}>
            <Loader2 size={16} className={styles.spinning} />
            <span>Loading conversations...</span>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className={`fade-in ${styles.container}`}>
      <div className={styles.layout}>
        <aside className={styles.sidebar}>
          <div className={styles.sidebarIntro}>
            <div>
              <p className={styles.sidebarEyebrow}>Thread Deck</p>
              <h1 className={styles.sidebarTitle}>Conversations</h1>
            </div>
            <div className={styles.sidebarActions}>
              <button
                className={`${styles.iconBtn} ${styles.iconBtnDanger}`}
                type="button"
                title="Delete all conversations"
                onClick={() => setShowDeleteAllConfirm(true)}
                disabled={conversations.length === 0}
              >
                <Trash2 size={15} />
              </button>
              <button className={styles.iconBtn} type="button" title="Conversation settings">
                <SlidersHorizontal size={15} />
              </button>
            </div>
          </div>

          <button className={styles.newConversationBtn} onClick={() => setShowNewDialog(true)} type="button">
            <Plus size={14} />
            New Chat
          </button>

          <div className={styles.sidebarHeader}>
            <div className={styles.searchWrap}>
              <Search size={13} className={styles.searchIcon} />
              <input
                className={styles.searchInput}
                placeholder="Search your threads..."
                value={search}
                onChange={e => setSearch(e.target.value)}
              />
            </div>
          </div>

          <div className={styles.convoList}>
            {filtered.length === 0 && (
              <div className={styles.emptyList}>
                {conversations.length === 0
                  ? 'No conversations yet'
                  : 'No results'}
              </div>
            )}
            {groupedConversations.map(group => (
              <section key={group.label} className={styles.convoGroup}>
                <p className={styles.convoGroupLabel}>{group.label}</p>
                {group.items.map(conv => {
                  const agent = agents.find(a => a.id === conv.agent_id)
                  return (
                    <button
                      key={conv.id}
                      className={`${styles.convoItem} ${selectedId === conv.id ? styles.convoItemActive : ''}`}
                      onClick={() => setSelectedId(conv.id)}
                      type="button"
                    >
                      <div className={styles.convoItemTop}>
                        <span className={styles.convoTitle}>{conv.title}</span>
                        <div className={styles.convoMeta}>
                          <span
                            className={`${styles.pinBtn} ${conv.pinned ? styles.pinBtnActive : ''}`}
                            role="button"
                            tabIndex={-1}
                            onClick={e => {
                              e.stopPropagation()
                              api.conversations.patch(conv.id, { pinned: !conv.pinned })
                                .then(updated => setConversations(prev =>
                                  prev.map(c => c.id === updated.id ? updated : c)
                                    .sort((a, b) => {
                                      if (a.pinned !== b.pinned) return b.pinned ? 1 : -1
                                      return b.updated_at.localeCompare(a.updated_at)
                                    })
                                ))
                            }}
                          >
                            <Pin size={11} />
                          </span>
                          <span className={styles.convoTime}>
                            {relativeTime(conv.updated_at)}
                          </span>
                        </div>
                      </div>
                      {agent && (
                        <div className={styles.convoAgent}>
                          <Bot size={10} />
                          {agent.name}
                        </div>
                      )}
                    </button>
                  )
                })}
              </section>
            ))}
          </div>
        </aside>

        <main className={styles.chat}>
          {!selectedConvo ? (
            <div className={styles.emptyPanel}>
              <EmptyState
                icon={MessageSquare}
                title="Select a conversation"
                description="Pick one from the sidebar or start a new one."
                action={<Button variant="primary" size="sm" onClick={() => setShowNewDialog(true)}>New Conversation</Button>}
              />
            </div>
          ) : (
            <>
              <div className={styles.chatHeader}>
                <div className={styles.chatHeaderInfo}>
                  <span className={styles.chatLabel}>Live</span>
                  <span className={styles.chatTitle}>{selectedConvo.title}</span>
                  {selectedAgent && (
                    <span className={styles.chatAgent}>
                      <Bot size={11} />
                      {selectedAgent.name}
                    </span>
                  )}
                </div>
                <div className={styles.chatHeaderActions}>
                  <button
                    className={styles.iconBtn}
                    type="button"
                    title="Share conversation"
                    onClick={() => setShowShareDialog(true)}
                  >
                    <Share2 size={14} />
                  </button>
                  <button
                    className={`${styles.iconBtn} ${styles.iconBtnDanger}`}
                    type="button"
                    title="Clear all messages"
                    onClick={handleClearMessages}
                    disabled={clearingMessages || streaming || messages.length === 0}
                  >
                    {clearingMessages
                      ? <Loader2 size={14} className={styles.spinning} />
                      : <Eraser size={14} />
                    }
                  </button>
                  <button className={styles.iconBtn} type="button" title="Conversation controls">
                    <SlidersHorizontal size={14} />
                  </button>
                </div>
              </div>

              <div className={styles.messages}>
                {messages.length === 0 && !streaming && (
                  <div className={styles.emptyChat}>
                    <div className={styles.assistantTag}>
                      <Bot size={13} />
                      {selectedAgent?.name ?? 'Assistant'}
                    </div>
                    <p className={styles.emptyChatTitle}>How can I help you?</p>
                    <p className={styles.emptyChatText}>
                      {selectedAgent
                        ? `${selectedAgent.name} is ready for research, writing, or tool calls.`
                        : 'Send a message to begin.'}
                    </p>
                  </div>
                )}

                {messages.map(msg => (
                  <MsgBubble
                    key={msg.id}
                    msg={msg}
                    agentName={selectedAgent?.name}
                  />
                ))}

                {streaming && streamBuffer && (
                  <div className={`${styles.messageRow} ${styles.messageRowAssistant}`}>
                    <div className={`${styles.bubble} ${styles.bubbleAssistant}`}>
                      <div className={styles.bubbleMeta}>
                        <div className={styles.bubbleAvatar}><Bot size={12} /></div>
                        <span className={styles.bubbleAuthor}>{selectedAgent?.name ?? 'Assistant'}</span>
                      </div>
                      {reasoningBuffer && (
                        <ReasoningBlock content={reasoningBuffer} isStreaming={!streamBuffer} />
                      )}
                      {activitySteps.length > 0 && <ActivityFeed steps={activitySteps} />}
                      {livePreview && <LivePreview preview={livePreview} />}
                      <div className={`${styles.bubbleContent} ${styles.bubbleContentAssistant}`}>
                        <div className={`${styles.markdown} ${styles.markdownStreaming}`}>
                          <MarkdownContent content={streamBuffer} />
                        </div>
                      </div>
                    </div>
                  </div>
                )}

                {streaming && !streamBuffer && (
                  <div className={`${styles.messageRow} ${styles.messageRowAssistant}`}>
                    <div className={`${styles.bubble} ${styles.bubbleAssistant}`}>
                      <div className={styles.bubbleMeta}>
                        <div className={styles.bubbleAvatar}><Bot size={12} /></div>
                        <span className={styles.bubbleAuthor}>{selectedAgent?.name ?? 'Assistant'}</span>
                      </div>
                      {reasoningBuffer && (
                        <ReasoningBlock content={reasoningBuffer} isStreaming />
                      )}
                      {activitySteps.length > 0 && <ActivityFeed steps={activitySteps} />}
                      {livePreview && <LivePreview preview={livePreview} />}
                      {!reasoningBuffer && activitySteps.length === 0 && (
                        <div className={`${styles.bubbleContent} ${styles.bubbleContentAssistant} ${styles.thinking}`}>
                          <span className={styles.dot} /><span className={styles.dot} /><span className={styles.dot} />
                        </div>
                      )}
                    </div>
                  </div>
                )}

                <div ref={messagesEndRef} />
              </div>

              <div
                className={`${styles.inputDock}${dragOver ? ` ${styles.inputDockDragOver}` : ''}`}
                ref={inputDockRef}
                onDragEnter={handleDragEnter}
                onDragLeave={handleDragLeave}
                onDragOver={handleDragOver}
                onDrop={handleDrop}
              >
                {dragOver && (
                  <div className={styles.dragOverlay}>
                    <Upload size={22} />
                    <span>Drop files or folders to attach</span>
                  </div>
                )}
                <input
                  ref={fileInputRef}
                  type="file"
                  multiple
                  className={styles.hiddenFileInput}
                  onChange={handleFileChange}
                />
                {/* @ts-expect-error webkitdirectory is non-standard but widely supported */}
                <input
                  ref={folderInputRef}
                  type="file"
                  webkitdirectory=""
                  multiple
                  className={styles.hiddenFileInput}
                  onChange={handleFileChange}
                />
                {selectedAgent?.container_enabled && !selectedAgent.container_config?.permissions?.network?.enabled && !selectedAgent.container_config?.network_enabled && (
                  <div className={styles.sandboxWarning}>
                    <WifiOff size={13} />
                    <span>
                      <strong>Sandbox has no internet access.</strong> Package installation (<code>pip install</code>) and downloads will fail.
                      Enable network permissions in agent settings to allow it.
                    </span>
                  </div>
                )}
                <div className={styles.inputNotice}>
                  Use <span>@toolname</span> for tools{agentSkills.length > 0 ? <> or <span>/skill</span> to invoke skills</> : null}.
                </div>

                {showSkillDropdown && (
                  <SkillDropdown
                    skills={agentSkills}
                    filter={slashFilter}
                    selectedIndex={skillDropdownIndex}
                    onSelect={handleSkillSelect}
                    position={{ bottom: inputDockRef.current ? inputDockRef.current.offsetHeight - 8 : 60, left: 24 }}
                  />
                )}

                {showToolDropdown && (
                  <ToolDropdown
                    tools={userTools}
                    filter={atFilter}
                    selectedIndex={toolDropdownIndex}
                    onSelect={handleToolSelect}
                    position={{ bottom: inputDockRef.current ? inputDockRef.current.offsetHeight - 8 : 60, left: 24 }}
                  />
                )}

                {uploading && (
                  <div className={styles.uploadProgressWrap}>
                    <div className={styles.uploadProgressInfo}>
                      <Loader2 size={13} className={styles.spinning} />
                      <span>Uploading{uploadProgress > 0 ? ` ${uploadProgress}%` : '...'}</span>
                    </div>
                    <div className={styles.uploadProgressTrack}>
                      <div className={styles.uploadProgressBar} style={{ width: `${uploadProgress}%` }} />
                    </div>
                  </div>
                )}

                {pendingFiles.length > 0 && (
                  <div className={styles.attachmentPreview}>
                    {pendingFiles.length > 4 ? (
                      <>
                        <div className={styles.bulkSummary}>
                          <button
                            className={styles.bulkSummaryToggle}
                            onClick={() => setFilesExpanded(prev => !prev)}
                            type="button"
                          >
                            {filesExpanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                            <Upload size={13} />
                            <strong>{pendingFiles.length}</strong> files
                            <span className={styles.attachmentSize}>
                              ({formatFileSize(pendingFiles.reduce((sum, f) => sum + f.size, 0))})
                            </span>
                          </button>
                          <button
                            className={styles.bulkClearBtn}
                            onClick={clearAllPendingFiles}
                            type="button"
                            title="Clear all files"
                          >
                            <X size={12} />
                            Clear all
                          </button>
                        </div>
                        {filesExpanded && (
                          <div className={styles.bulkFileList}>
                            {pendingFiles.map((file, i) => (
                              <div key={`${file.name}-${i}`} className={styles.bulkFileRow}>
                                {file.type.startsWith('image/') ? <ImageIcon size={11} /> : file.type === 'application/pdf' ? <FileText size={11} /> : <FileIcon size={11} />}
                                <span className={styles.attachmentName}>{file.name}</span>
                                <span className={styles.attachmentSize}>{formatFileSize(file.size)}</span>
                                <button
                                  className={styles.attachmentRemove}
                                  onClick={() => removePendingFile(i)}
                                  type="button"
                                  title="Remove"
                                >
                                  <X size={9} />
                                </button>
                              </div>
                            ))}
                          </div>
                        )}
                      </>
                    ) : (
                      pendingFiles.map((file, i) => (
                        <div key={`${file.name}-${i}`} className={styles.attachmentChip}>
                          {file.type.startsWith('image/') ? <ImageIcon size={12} /> : file.type === 'application/pdf' ? <FileText size={12} /> : <FileIcon size={12} />}
                          <span className={styles.attachmentName}>{file.name}</span>
                          <span className={styles.attachmentSize}>{formatFileSize(file.size)}</span>
                          <button
                            className={styles.attachmentRemove}
                            onClick={() => removePendingFile(i)}
                            type="button"
                            title="Remove"
                          >
                            <X size={10} />
                          </button>
                        </div>
                      ))
                    )}
                  </div>
                )}

                <div className={styles.inputBar}>
                  <textarea
                    ref={inputRef}
                    className={styles.input}
                    value={input}
                    onChange={handleInputChange}
                    onKeyDown={handleKeyDown}
                    placeholder={streaming ? 'Waiting for response...' : agentSkills.length > 0 ? 'Type a message or / for skills...' : 'Type your message here...'}
                    rows={1}
                    disabled={streaming}
                  />
                  <div className={styles.inputFooter}>
                    <div className={styles.inputTools}>
                      <button className={`${styles.toolChip} ${styles.toolChipPrimary}`} type="button">
                        <span className={styles.statusDot} />
                        {selectedAgent?.name ?? 'Assistant'}
                      </button>
                      <button
                        className={`${styles.toolChip} ${reasoningEnabled ? styles.toolChipActive : ''}`}
                        type="button"
                        onClick={() => {
                          if (!reasoningEnabled) {
                            setReasoningEnabled(true)
                            setReasoningEffort('medium')
                          } else {
                            // Cycle: medium -> high -> low -> off
                            const cycle: Record<ReasoningEffort, ReasoningEffort | null> = { low: null, medium: 'high', high: 'low' }
                            const next = cycle[reasoningEffort]
                            if (next === null) {
                              setReasoningEnabled(false)
                            } else {
                              setReasoningEffort(next)
                            }
                          }
                        }}
                        title={reasoningEnabled ? `Reasoning: ${reasoningEffort} (click to cycle)` : 'Enable reasoning'}
                      >
                        <Brain size={12} />
                        {reasoningEnabled ? `Reason (${reasoningEffort})` : 'Reasoning'}
                      </button>
                      <button
                        className={`${styles.toolChip} ${searchEnabled ? styles.toolChipActive : ''}`}
                        type="button"
                        onClick={() => setSearchEnabled(prev => !prev)}
                        title={searchEnabled ? 'Knowledge search enabled' : 'Knowledge search disabled'}
                      >
                        <Globe size={12} />
                        Search{searchEnabled ? '' : ' (off)'}
                      </button>
                      <button
                        className={`${styles.toolChip} ${pendingFiles.length > 0 ? styles.toolChipActive : ''}`}
                        type="button"
                        onClick={handleFilePick}
                        title={pendingFiles.length > 0 ? `${pendingFiles.length} file(s) attached` : 'Attach files'}
                      >
                        <Paperclip size={12} />
                        Attach{pendingFiles.length > 0 ? ` (${pendingFiles.length})` : ''}
                      </button>
                      <button
                        className={styles.toolChip}
                        type="button"
                        onClick={handleFolderPick}
                        title="Upload a folder"
                      >
                        <FolderOpen size={12} />
                        Folder
                      </button>
                    </div>
                    <button
                      className={styles.sendBtn}
                      onClick={sendMessage}
                      disabled={!input.trim() || streaming || uploading}
                      title="Send (Enter)"
                      type="button"
                    >
                      {streaming || uploading ? <Loader2 size={16} className={styles.spinning} /> : <Send size={16} />}
                    </button>
                  </div>
                </div>
              </div>
            </>
          )}
        </main>
      </div>

      {showNewDialog && (
        <NewConvoDialog
          agents={agents}
          onClose={() => setShowNewDialog(false)}
          onCreate={handleNewConvo}
        />
      )}

      {showDeleteAllConfirm && (
        <div className={styles.dialogOverlay} onClick={() => !deletingAll && setShowDeleteAllConfirm(false)}>
          <div className={styles.dialog} onClick={e => e.stopPropagation()}>
            <div className={styles.dialogHeader}>
              <h3 className={styles.dialogTitle}>Delete all conversations?</h3>
              <button className={styles.dialogClose} onClick={() => setShowDeleteAllConfirm(false)} disabled={deletingAll}><X size={16} /></button>
            </div>
            <p className={styles.dialogBody}>
              This will permanently delete all your conversations and their messages. This action cannot be undone.
            </p>
            <div className={styles.dialogActions}>
              <Button variant="secondary" size="sm" type="button" onClick={() => setShowDeleteAllConfirm(false)} disabled={deletingAll}>
                Cancel
              </Button>
              <Button variant="primary" size="sm" type="button" onClick={handleDeleteAll} disabled={deletingAll}>
                {deletingAll ? <Loader2 size={14} className={styles.spinning} /> : <Trash2 size={14} />}
                Delete all
              </Button>
            </div>
          </div>
        </div>
      )}

      {showShareDialog && selectedId && (
        <ShareDialog
          conversationId={selectedId}
          onClose={() => setShowShareDialog(false)}
        />
      )}
    </div>
  )
}

// ── Utility ────────────────────────────────────────────────────────

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime()
  const m = Math.floor(diff / 60000)
  if (m < 1) return 'just now'
  if (m < 60) return `${m}m`
  const h = Math.floor(m / 60)
  if (h < 24) return `${h}h`
  return `${Math.floor(h / 24)}d`
}

function groupConversations(conversations: Conversation[]) {
  const pinned = conversations.filter(c => c.pinned)
  const unpinned = conversations.filter(c => !c.pinned)

  const groups: { label: string; items: Conversation[] }[] = []

  if (pinned.length > 0) {
    groups.push({ label: 'Pinned', items: pinned })
  }

  const timeGroups = new Map<string, Conversation[]>()
  unpinned.forEach(conversation => {
    const label = groupLabel(conversation.updated_at)
    const items = timeGroups.get(label) ?? []
    items.push(conversation)
    timeGroups.set(label, items)
  })
  for (const [label, items] of timeGroups.entries()) {
    groups.push({ label, items })
  }

  return groups
}

function groupLabel(iso: string) {
  const date = new Date(iso)
  const now = new Date()
  if (date.toDateString() === now.toDateString()) return 'Today'

  const yesterday = new Date(now)
  yesterday.setDate(now.getDate() - 1)
  if (date.toDateString() === yesterday.toDateString()) return 'Yesterday'

  return 'Earlier'
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
