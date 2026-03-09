import { useState, useEffect } from 'react'
import { Wrench, Plug, Search, ToggleRight, ToggleLeft, AtSign, Loader2 } from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { api, type Tool, type Connector } from '../lib/api'
import styles from './Tools.module.css'

export function ToolsPage() {
  const [tools, setTools] = useState<Tool[]>([])
  const [connectors, setConnectors] = useState<Connector[]>([])
  const [search, setSearch] = useState('')
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    Promise.all([
      api.connectors.list().catch(() => [] as Connector[]),
    ])
      .then(([conns]) => {
        setConnectors(conns)
        // Tools are provided by connectors - for now use mock data
        // until the tools API returns real data
        setTools([
          { id: '1', name: 'send_email', description: 'Send an email through a connected Gmail account.', connector_id: '', schema: {}, enabled: true },
          { id: '2', name: 'read_inbox', description: 'Read recent emails from the Gmail inbox.', connector_id: '', schema: {}, enabled: true },
          { id: '3', name: 'send_telegram', description: 'Send a message via Telegram bot.', connector_id: '', schema: {}, enabled: true },
          { id: '4', name: 'web_search', description: 'Search the web for information.', connector_id: '', schema: {}, enabled: false },
        ])
      })
      .finally(() => setLoading(false))
  }, [])

  const filteredTools = tools.filter(t =>
    t.name.toLowerCase().includes(search.toLowerCase()) ||
    t.description.toLowerCase().includes(search.toLowerCase())
  )

  return (
    <div className="fade-in">
      <PageHeader
        title="Tools"
        description="Tools are provided by your connectors. Use @toolname in conversations to invoke them."
      />

      {/* Syntax hint */}
      <div className={styles.syntaxHint}>
        <AtSign size={14} />
        <span>
          Type <code className={styles.syntaxCode}>@toolname</code> in any conversation to invoke a tool.
          Tools are automatically discovered from your active connectors.
        </span>
      </div>

      {/* Search */}
      <div className={styles.searchBar}>
        <Search size={15} />
        <input
          type="text"
          placeholder="Search tools..."
          className={styles.searchInput}
          value={search}
          onChange={e => setSearch(e.target.value)}
        />
      </div>

      {loading ? (
        <div className={styles.loadingRow}><Loader2 size={15} className="spinning" /> Loading tools...</div>
      ) : filteredTools.length === 0 ? (
        <div className={styles.emptyState}>
          <Wrench size={36} strokeWidth={1} />
          <p className={styles.emptyTitle}>{search ? 'No tools match your search' : 'No tools available'}</p>
          <p className={styles.emptyDesc}>Tools are automatically provided by your active connectors.</p>
        </div>
      ) : (
        <div className={`${styles.list} stagger`}>
          {filteredTools.map(tool => (
            <Card key={tool.id}>
              <div className={styles.toolRow}>
                <div className={styles.toolIcon} data-active={tool.enabled}>
                  <Wrench size={16} strokeWidth={1.5} />
                </div>
                <div className={styles.toolInfo}>
                  <div className={styles.toolNameRow}>
                    <code className={styles.toolName}>@{tool.name}</code>
                    <div className={styles.toolStatus} data-enabled={tool.enabled}>
                      {tool.enabled
                        ? <ToggleRight size={18} />
                        : <ToggleLeft size={18} />}
                      <span>{tool.enabled ? 'Active' : 'Inactive'}</span>
                    </div>
                  </div>
                  <p className={styles.toolDesc}>{tool.description}</p>
                  <div className={styles.toolMeta}>
                    <span className={styles.toolConnector}>
                      <Plug size={10} />
                      {tool.connector_id
                        ? connectors.find(c => c.id === tool.connector_id)?.name ?? 'Unknown'
                        : 'Built-in'}
                    </span>
                  </div>
                </div>
              </div>
            </Card>
          ))}
        </div>
      )}
    </div>
  )
}
