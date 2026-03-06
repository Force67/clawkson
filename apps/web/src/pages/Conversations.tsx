import { useState, useEffect, useRef, useCallback } from 'react'
import { Plus, Search, Send, Bot, Sparkles, ChevronRight, X, Loader2 } from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Button } from '../components/Button'
import { EmptyState } from '../components/EmptyState'
import { api, streamChat, type Agent, type Conversation, type Message } from '../lib/api'
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
  const [showNewDialog, setShowNewDialog] = useState(false)
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')

  const messagesEndRef = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLTextAreaElement>(null)
  const stopStreamRef = useRef<(() => void) | null>(null)

  const selectedConvo = conversations.find(c => c.id === selectedId)
  const selectedAgent = agents.find(a => a.id === selectedConvo?.agent_id)

  const filtered = conversations.filter(c =>
    c.title.toLowerCase().includes(search.toLowerCase()) ||
    agents.find(a => a.id === c.agent_id)?.name.toLowerCase().includes(search.toLowerCase())
  )

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
  }, [messages, streamBuffer])

  const sendMessage = useCallback(() => {
    const content = input.trim()
    if (!content || !selectedId || streaming) return

    setInput('')
    setStreaming(true)
    setStreamBuffer('')

    // Optimistically show user message
    const tempUserMsg: Message = {
      id: `tmp-${Date.now()}`,
      conversation_id: selectedId,
      role: 'user',
      content,
      created_at: new Date().toISOString(),
    }
    setMessages(prev => [...prev, tempUserMsg])

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
    )
    stopStreamRef.current = stop
  }, [input, selectedId, streaming])

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

  if (loading) {
    return (
      <div className={`fade-in ${styles.container}`}>
        <PageHeader title="Conversations" description="Chat with your agents." />
        <div className={styles.loadingState}>
          <Loader2 size={24} className={styles.spinning} />
          <span>Loading conversations…</span>
        </div>
      </div>
    )
  }

  return (
    <div className={`fade-in ${styles.container}`}>
      <PageHeader
        title="Conversations"
        description="Chat with your AI agents. Use @toolname to invoke tools."
      />

      <div className={styles.layout}>
        {/* ── Sidebar list ── */}
        <aside className={styles.sidebar}>
          <div className={styles.sidebarHeader}>
            <div className={styles.searchWrap}>
              <Search size={13} className={styles.searchIcon} />
              <input
                className={styles.searchInput}
                placeholder="Search…"
                value={search}
                onChange={e => setSearch(e.target.value)}
              />
            </div>
            <button className={styles.newBtn} onClick={() => setShowNewDialog(true)} title="New conversation">
              <Plus size={16} />
            </button>
          </div>

          <div className={styles.convoList}>
            {filtered.length === 0 && (
              <div className={styles.emptyList}>
                {conversations.length === 0
                  ? 'No conversations yet'
                  : 'No results'}
              </div>
            )}
            {filtered.map(conv => {
              const agent = agents.find(a => a.id === conv.agent_id)
              return (
                <button
                  key={conv.id}
                  className={`${styles.convoItem} ${selectedId === conv.id ? styles.convoItemActive : ''}`}
                  onClick={() => setSelectedId(conv.id)}
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
          </div>
        </aside>

        {/* ── Chat area ── */}
        <main className={styles.chat}>
          {!selectedConvo ? (
            <EmptyState
              icon={Sparkles}
              title="Select a conversation"
              description="Pick one from the sidebar or start a new one."
              action={<Button variant="primary" size="sm" onClick={() => setShowNewDialog(true)}>New Conversation</Button>}
            />
          ) : (
            <>
              {/* Chat header */}
              <div className={styles.chatHeader}>
                <div className={styles.chatHeaderInfo}>
                  <span className={styles.chatTitle}>{selectedConvo.title}</span>
                  {selectedAgent && (
                    <span className={styles.chatAgent}>
                      <Bot size={12} />
                      {selectedAgent.name}
                    </span>
                  )}
                </div>
              </div>

              {/* Messages */}
              <div className={styles.messages}>
                {messages.length === 0 && !streaming && (
                  <div className={styles.emptyChat}>
                    <div className={styles.emptyChatIcon}><Bot size={28} strokeWidth={1.2} /></div>
                    <p className={styles.emptyChatText}>
                      {selectedAgent
                        ? `${selectedAgent.name} is ready. Send a message to begin.`
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

                {/* Live streaming buffer */}
                {streaming && streamBuffer && (
                  <div className={`${styles.bubble} ${styles.bubbleAssistant}`}>
                    <div className={styles.bubbleMeta}>
                      <div className={styles.bubbleAvatar}><Bot size={12} /></div>
                      <span className={styles.bubbleAuthor}>{selectedAgent?.name ?? 'Assistant'}</span>
                    </div>
                    <div className={`${styles.bubbleContent} ${styles.bubbleContentAssistant}`}>
                      {streamBuffer}
                      <span className={styles.cursor} />
                    </div>
                  </div>
                )}

                {/* Thinking indicator (streaming but no buffer yet) */}
                {streaming && !streamBuffer && (
                  <div className={`${styles.bubble} ${styles.bubbleAssistant}`}>
                    <div className={styles.bubbleMeta}>
                      <div className={styles.bubbleAvatar}><Bot size={12} /></div>
                      <span className={styles.bubbleAuthor}>{selectedAgent?.name ?? 'Assistant'}</span>
                    </div>
                    <div className={`${styles.bubbleContent} ${styles.bubbleContentAssistant} ${styles.thinking}`}>
                      <span className={styles.dot} /><span className={styles.dot} /><span className={styles.dot} />
                    </div>
                  </div>
                )}

                <div ref={messagesEndRef} />
              </div>

              {/* Input bar */}
              <div className={styles.inputBar}>
                <div className={styles.inputWrap}>
                  <textarea
                    ref={inputRef}
                    className={styles.input}
                    value={input}
                    onChange={e => setInput(e.target.value)}
                    onKeyDown={handleKeyDown}
                    placeholder={streaming ? 'Waiting for response…' : 'Message… (Shift+Enter for new line)'}
                    rows={1}
                    disabled={streaming}
                  />
                  <button
                    className={styles.sendBtn}
                    onClick={sendMessage}
                    disabled={!input.trim() || streaming}
                    title="Send (Enter)"
                  >
                    {streaming ? <Loader2 size={16} className={styles.spinning} /> : <Send size={16} />}
                  </button>
                </div>
                <p className={styles.inputHint}>Enter to send · Shift+Enter for newline · @toolname to invoke tools</p>
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
