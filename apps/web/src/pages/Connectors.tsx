import { Plus, Mail, MessageSquare, ToggleLeft, ToggleRight } from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import styles from './Connectors.module.css'

const MOCK_CONNECTORS = [
  {
    id: '1',
    name: 'Gmail',
    type: 'gmail',
    icon: Mail,
    enabled: true,
    status: 'Connected',
    description: 'Read and send emails via Gmail API.',
  },
  {
    id: '2',
    name: 'Telegram Bot',
    type: 'telegram',
    icon: MessageSquare,
    enabled: true,
    status: 'Connected',
    description: 'Receive and send messages through Telegram Bot API.',
  },
  {
    id: '3',
    name: 'Slack Workspace',
    type: 'slack',
    icon: MessageSquare,
    enabled: false,
    status: 'Disconnected',
    description: 'Integrate with Slack for team notifications.',
  },
]

export function ConnectorsPage() {
  return (
    <div className="fade-in">
      <PageHeader
        title="Connectors"
        description="Add platforms like Telegram, Gmail, Slack and more."
        actions={
          <Button>
            <Plus size={16} /> Add Connector
          </Button>
        }
      />

      <div className={styles.grid}>
        {MOCK_CONNECTORS.map(conn => (
          <Card key={conn.id} interactive>
            <div className={styles.connHeader}>
              <div className={styles.connIcon}>
                <conn.icon size={20} strokeWidth={1.5} />
              </div>
              <div className={styles.connToggle}>
                {conn.enabled ? (
                  <ToggleRight size={24} className={styles.toggleOn} />
                ) : (
                  <ToggleLeft size={24} className={styles.toggleOff} />
                )}
              </div>
            </div>
            <h3 className={styles.connName}>{conn.name}</h3>
            <p className={styles.connDesc}>{conn.description}</p>
            <div className={styles.connStatus} data-connected={conn.enabled}>
              <span className={styles.connDot} />
              {conn.status}
            </div>
          </Card>
        ))}
      </div>
    </div>
  )
}
