import { Wrench, Plug, Search } from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { StatusBadge } from '../components/StatusBadge'
import styles from './Tools.module.css'

const MOCK_TOOLS = [
  {
    id: '1',
    name: 'send_email',
    description: 'Send an email through a connected Gmail account.',
    connector: 'Gmail',
    enabled: true,
  },
  {
    id: '2',
    name: 'read_inbox',
    description: 'Read recent emails from the Gmail inbox.',
    connector: 'Gmail',
    enabled: true,
  },
  {
    id: '3',
    name: 'send_telegram',
    description: 'Send a message via Telegram bot.',
    connector: 'Telegram Bot',
    enabled: true,
  },
  {
    id: '4',
    name: 'web_search',
    description: 'Search the web for information.',
    connector: 'Built-in',
    enabled: false,
  },
]

export function ToolsPage() {
  return (
    <div className="fade-in">
      <PageHeader
        title="Tools"
        description="Tools are provided by connectors. Mention them with @toolname in conversations."
      />

      <div className={styles.searchBar}>
        <Search size={16} />
        <input type="text" placeholder="Search tools..." className={styles.searchInput} />
      </div>

      <div className={styles.list}>
        {MOCK_TOOLS.map(tool => (
          <Card key={tool.id}>
            <div className={styles.toolRow}>
              <div className={styles.toolIcon}>
                <Wrench size={18} strokeWidth={1.5} />
              </div>
              <div className={styles.toolInfo}>
                <div className={styles.toolNameRow}>
                  <code className={styles.toolName}>@{tool.name}</code>
                  <StatusBadge status={tool.enabled ? 'online' : 'offline'} />
                </div>
                <p className={styles.toolDesc}>{tool.description}</p>
                <span className={styles.toolConnector}>
                  <Plug size={11} />
                  {tool.connector}
                </span>
              </div>
            </div>
          </Card>
        ))}
      </div>
    </div>
  )
}
