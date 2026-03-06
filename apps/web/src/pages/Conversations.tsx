import { useState } from 'react'
import { Plus, Search, MessageCircle, Send } from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Button } from '../components/Button'
import { EmptyState } from '../components/EmptyState'
import styles from './Conversations.module.css'

interface MockConvo {
  id: string
  title: string
  agent: string
  lastMessage: string
  time: string
  unread: boolean
}

const MOCK_CONVOS: MockConvo[] = [
  { id: '1', title: 'Research Q4 earnings', agent: 'Research Agent', lastMessage: 'I found 3 relevant reports...', time: '5m', unread: true },
  { id: '2', title: 'Draft weekly update', agent: 'Email Assistant', lastMessage: 'Here is the draft email...', time: '1h', unread: false },
  { id: '3', title: 'Review PR #142', agent: 'Code Reviewer', lastMessage: 'The changes look good...', time: '3h', unread: false },
]

export function ConversationsPage() {
  const [selected, setSelected] = useState<string | null>(MOCK_CONVOS[0]?.id ?? null)
  const [message, setMessage] = useState('')

  const selectedConvo = MOCK_CONVOS.find(c => c.id === selected)

  return (
    <div className={`fade-in ${styles.container}`}>
      <PageHeader
        title="Conversations"
        description="Chat with your agents. Use @toolname to invoke tools."
      />

      <div className={styles.chatLayout}>
        {/* Sidebar list */}
        <div className={styles.convoList}>
          <div className={styles.listHeader}>
            <div className={styles.searchBar}>
              <Search size={14} />
              <input type="text" placeholder="Search conversations..." className={styles.searchInput} />
            </div>
            <Button size="sm" variant="primary">
              <Plus size={14} /> New
            </Button>
          </div>

          <div className={styles.listItems}>
            {MOCK_CONVOS.map(convo => (
              <div
                key={convo.id}
                className={`${styles.convoItem} ${selected === convo.id ? styles.convoItemActive : ''}`}
                onClick={() => setSelected(convo.id)}
              >
                <div className={styles.convoItemTop}>
                  <span className={styles.convoTitle}>{convo.title}</span>
                  <span className={styles.convoTime}>{convo.time}</span>
                </div>
                <div className={styles.convoMeta}>
                  <span className={styles.convoAgent}>{convo.agent}</span>
                  {convo.unread && <span className={styles.unreadDot} />}
                </div>
                <p className={styles.convoPreview}>{convo.lastMessage}</p>
              </div>
            ))}
          </div>
        </div>

        {/* Chat area */}
        <div className={styles.chatArea}>
          {selectedConvo ? (
            <>
              <div className={styles.chatHeader}>
                <div>
                  <h3 className={styles.chatTitle}>{selectedConvo.title}</h3>
                  <span className={styles.chatAgent}>{selectedConvo.agent}</span>
                </div>
              </div>

              <div className={styles.messages}>
                <div className={`${styles.messageBubble} ${styles.messageAssistant}`}>
                  <span className={styles.messageRole}>Assistant</span>
                  <p>{selectedConvo.lastMessage}</p>
                </div>
              </div>

              <div className={styles.inputBar}>
                <input
                  className={styles.messageInput}
                  type="text"
                  placeholder="Type a message... (use @tool to invoke tools)"
                  value={message}
                  onChange={e => setMessage(e.target.value)}
                  onKeyDown={e => {
                    if (e.key === 'Enter' && message.trim()) {
                      setMessage('')
                    }
                  }}
                />
                <button
                  className={styles.sendButton}
                  disabled={!message.trim()}
                  onClick={() => setMessage('')}
                >
                  <Send size={16} />
                </button>
              </div>
            </>
          ) : (
            <EmptyState
              icon={MessageCircle}
              title="No conversation selected"
              description="Select a conversation from the list or start a new one."
            />
          )}
        </div>
      </div>
    </div>
  )
}
