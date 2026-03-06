import { useState, useEffect } from 'react'
import {
  Activity,
  Bot,
  MessageCircle,
  Plug,
  TrendingUp,
  Clock,
  Plus,
} from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { StatusBadge } from '../components/StatusBadge'
import { Button } from '../components/Button'
import { api, type Agent, type Conversation } from '../lib/api'
import styles from './Dashboard.module.css'

export function DashboardPage() {
  const [agents, setAgents] = useState<Agent[]>([])
  const [conversations, setConversations] = useState<Conversation[]>([])
  const [loading, setLoading] = useState(true)
  const navigate = useNavigate()

  useEffect(() => {
    Promise.all([api.agents.list(), api.conversations.list()])
      .then(([agts, convos]) => { setAgents(agts); setConversations(convos) })
      .finally(() => setLoading(false))
  }, [])

  const onlineCount = agents.filter(a => a.status === 'online').length
  const busyCount = agents.filter(a => a.status === 'busy').length

  const stats = [
    { label: 'Total Agents', value: String(agents.length), icon: Bot, change: `${onlineCount} online` },
    { label: 'Conversations', value: String(conversations.length), icon: MessageCircle, change: 'All time' },
    { label: 'Active Now', value: String(onlineCount + busyCount), icon: TrendingUp, change: `${onlineCount} online · ${busyCount} busy` },
    { label: 'Connectors', value: '—', icon: Plug, change: 'See connectors page' },
  ]

  return (
    <div className="fade-in">
      <PageHeader
        title="Dashboard"
        description="Overview of your agents, their status, and recent activity."
      />

      {/* Stats */}
      <div className={styles.statsGrid}>
        {stats.map(({ label, value, icon: Icon, change }) => (
          <Card key={label}>
            <div className={styles.statCard}>
              <div className={styles.statIcon}><Icon size={20} strokeWidth={1.5} /></div>
              <div>
                <div className={styles.statValue}>{loading ? '—' : value}</div>
                <div className={styles.statLabel}>{label}</div>
                <div className={styles.statChange}>{change}</div>
              </div>
            </div>
          </Card>
        ))}
      </div>

      <div className={styles.mainGrid}>
        {/* Agents */}
        <Card>
          <div className={styles.sectionHeader}>
            <h2 className={styles.sectionTitle}><Bot size={18} strokeWidth={1.5} /> Agents</h2>
            <Button size="sm" variant="secondary" onClick={() => navigate('/agents')}>
              <Plus size={13} /> Manage
            </Button>
          </div>
          {agents.length === 0 && !loading ? (
            <p className={styles.emptyMsg}>No agents yet. <button className={styles.emptyLink} onClick={() => navigate('/agents')}>Create one →</button></p>
          ) : (
            <div className={styles.agentList}>
              {agents.slice(0, 6).map(agent => (
                <div key={agent.id} className={styles.agentRow}>
                  <div className={styles.agentInfo}>
                    <span className={styles.agentName}>{agent.name}</span>
                    <StatusBadge status={agent.status} />
                  </div>
                  <span className={styles.agentTime}>
                    <Clock size={12} />
                    {relativeTime(agent.updated_at)}
                  </span>
                </div>
              ))}
            </div>
          )}
        </Card>

        {/* Recent conversations */}
        <Card>
          <div className={styles.sectionHeader}>
            <h2 className={styles.sectionTitle}><Activity size={18} strokeWidth={1.5} /> Recent Conversations</h2>
            <Button size="sm" variant="secondary" onClick={() => navigate('/conversations')}>
              View all
            </Button>
          </div>
          {conversations.length === 0 && !loading ? (
            <p className={styles.emptyMsg}>No conversations yet. <button className={styles.emptyLink} onClick={() => navigate('/conversations')}>Start one →</button></p>
          ) : (
            <div className={styles.activityList}>
              {conversations.slice(0, 5).map(convo => (
                <div
                  key={convo.id}
                  className={styles.activityRow}
                  onClick={() => navigate(`/conversations`)}
                  style={{ cursor: 'pointer' }}
                >
                  <div className={styles.activityDot} data-type="info" />
                  <div className={styles.activityContent}>
                    <span className={styles.activityText}>{convo.title}</span>
                    <span className={styles.activityTime}>{relativeTime(convo.updated_at)}</span>
                  </div>
                </div>
              ))}
            </div>
          )}
        </Card>
      </div>
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

