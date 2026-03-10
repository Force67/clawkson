import { useState, useEffect } from 'react'
import { Wrench, Plug, Search, AtSign, Loader2, Cpu } from 'lucide-react'
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
      api.tools.list().catch(() => [] as Tool[]),
      api.connectors.list().catch(() => [] as Connector[]),
    ])
      .then(([fetchedTools, conns]) => {
        setTools(fetchedTools)
        setConnectors(conns)
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
          {filteredTools.map(tool => {
            const connectorName = tool.connector_id
              ? connectors.find(c => c.id === tool.connector_id)?.name ?? 'Unknown'
              : null
            return (
              <Card key={tool.id}>
                <div className={styles.toolRow}>
                  <div className={styles.toolIcon} data-active={tool.enabled}>
                    {tool.tool_type === 'builtin'
                      ? <Cpu size={16} strokeWidth={1.5} />
                      : <Wrench size={16} strokeWidth={1.5} />}
                  </div>
                  <div className={styles.toolInfo}>
                    <div className={styles.toolNameRow}>
                      <code className={styles.toolName}>@{tool.name}</code>
                      <div className={styles.toolStatus} data-enabled={tool.enabled}>
                        <span>{tool.tool_type === 'builtin' ? 'Built-in' : 'Connector'}</span>
                      </div>
                    </div>
                    <p className={styles.toolDesc}>{tool.description}</p>
                    <div className={styles.toolMeta}>
                      <span className={styles.toolConnector}>
                        <Plug size={10} />
                        {connectorName ?? 'Built-in'}
                      </span>
                    </div>
                  </div>
                </div>
              </Card>
            )
          })}
        </div>
      )}
    </div>
  )
}
