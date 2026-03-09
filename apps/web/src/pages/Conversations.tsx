import { useState, useEffect, useRef, useCallback } from 'react'
import { Plus, Search, Send, Bot, MessageSquare, ChevronRight, X, Loader2, Brain, Paperclip, SlidersHorizontal, Globe, File as FileIcon, Image as ImageIcon, FileText } from 'lucide-react'
import { Button } from '../components/Button'
import { EmptyState } from '../components/EmptyState'
import { api, streamChat, type Agent, type Conversation, type Message, type ReasoningEffort, type AttachmentInfo } from '../lib/api'
import styles from './Conversations.module.css'

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

function MsgBubble({ msg, agentName }: MsgBubbleProps) {
  const isUser = msg.role === 'user'
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
          {msg.content}
        </div>
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
  const [reasoningEnabled, setReasoningEnabled] = useState(false)
  const [reasoningEffort, setReasoningEffort] = useState<ReasoningEffort>('medium')
  const [searchEnabled, setSearchEnabled] = useState(true)
  const [pendingFiles, setPendingFiles] = useState<File[]>([])
  const [uploading, setUploading] = useState(false)

  const messagesEndRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLTextAreaElement>(null)
  const stopStreamRef = useRef<(() => void) | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)

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
        setConversations(convos.sort((a, b) => b.updated_at.localeCompare(a.updated_at)))
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

  // Scroll to bottom on new messages or stream buffer changes
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages, streamBuffer, reasoningBuffer])

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

    // Upload pending files first
    let attachmentIds: string[] = []
    const filesToUpload = [...pendingFiles]
    setPendingFiles([])

    if (filesToUpload.length > 0) {
      setUploading(true)
      try {
        const result = await api.uploads.upload(filesToUpload, selectedId)
        attachmentIds = result.files.map(f => f.id)
      } catch (err) {
        console.error('File upload failed:', err)
        // Continue sending the message without attachments
      } finally {
        setUploading(false)
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
          setStreaming(false)
          // Refresh conversation list (updated_at changed)
          api.conversations.list().then(convos =>
            setConversations(convos.sort((a, b) => b.updated_at.localeCompare(a.updated_at)))
          )
        })
      },
      (err) => {
        console.error('Stream error, falling back to non-streaming:', err)
        // Fall back to non-streaming chat
        setStreamBuffer('')
        setReasoningBuffer('')
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
    )
    stopStreamRef.current = stop
  }, [input, selectedId, streaming, uploading, reasoningEnabled, reasoningEffort, searchEnabled, pendingFiles])

  const handleKeyDown = (e: React.KeyboardEvent) => {
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
    const files = Array.from(e.target.files ?? [])
    if (files.length > 0) {
      setPendingFiles(prev => [...prev, ...files])
    }
    // Reset so the same file can be re-selected
    e.target.value = ''
  }

  const removePendingFile = (index: number) => {
    setPendingFiles(prev => prev.filter((_, i) => i !== index))
  }

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
            <button className={styles.iconBtn} type="button" title="Conversation settings">
              <SlidersHorizontal size={15} />
            </button>
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
                        <span className={styles.convoTime}>
                          {relativeTime(conv.updated_at)}
                        </span>
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
                      <div className={`${styles.bubbleContent} ${styles.bubbleContentAssistant}`}>
                        {streamBuffer}
                        <span className={styles.cursor} />
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
                      {!reasoningBuffer && (
                        <div className={`${styles.bubbleContent} ${styles.bubbleContentAssistant} ${styles.thinking}`}>
                          <span className={styles.dot} /><span className={styles.dot} /><span className={styles.dot} />
                        </div>
                      )}
                    </div>
                  </div>
                )}

                <div ref={messagesEndRef} />
              </div>

              <div className={styles.inputDock}>
                <input
                  ref={fileInputRef}
                  type="file"
                  multiple
                  className={styles.hiddenFileInput}
                  onChange={handleFileChange}
                />
                <div className={styles.inputNotice}>Use <span>@toolname</span> to trigger tools inside the conversation.</div>

                {pendingFiles.length > 0 && (
                  <div className={styles.attachmentPreview}>
                    {pendingFiles.map((file, i) => (
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
                    ))}
                  </div>
                )}

                <div className={styles.inputBar}>
                  <textarea
                    ref={inputRef}
                    className={styles.input}
                    value={input}
                    onChange={e => setInput(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder={streaming ? 'Waiting for response...' : 'Type your message here...'}
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
  const groups = new Map<string, Conversation[]>()

  conversations.forEach(conversation => {
    const label = groupLabel(conversation.updated_at)
    const items = groups.get(label) ?? []
    items.push(conversation)
    groups.set(label, items)
  })

  return Array.from(groups.entries()).map(([label, items]) => ({ label, items }))
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
