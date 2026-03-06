import {
  Activity,
  Bot,
  MessageCircle,
  Plug,
  TrendingUp,
  Clock,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { StatusBadge } from '../components/StatusBadge'
import styles from './Dashboard.module.css'

// Mock data — will be replaced with API calls
const MOCK_AGENTS = [
  { id: '1', name: 'Research Agent', status: 'online' as const, lastActive: '2 min ago' },
  { id: '2', name: 'Email Assistant', status: 'busy' as const, lastActive: 'now' },
  { id: '3', name: 'Code Reviewer', status: 'offline' as const, lastActive: '3h ago' },
]

const MOCK_ACTIVITY = [
  { id: '1', text: 'Research Agent completed task "Summarize Q4 report"', time: '5 min ago', type: 'success' },
  { id: '2', text: 'Email Assistant processing 3 new messages', time: '12 min ago', type: 'info' },
  { id: '3', text: 'New connector "Gmail" configured', time: '1h ago', type: 'info' },
  { id: '4', text: 'Knowledge base updated with 12 new entries', time: '2h ago', type: 'info' },
]

const STATS = [
  { label: 'Active Agents', value: '2', icon: Bot, change: '+1 today' },
  { label: 'Conversations', value: '24', icon: MessageCircle, change: '+5 today' },
  { label: 'Connectors', value: '3', icon: Plug, change: 'All healthy' },
  { label: 'Tasks Completed', value: '156', icon: TrendingUp, change: '+12 today' },
]

export function DashboardPage() {
  return (
    <div className="fade-in">
      <PageHeader
        title="Dashboard"
        description="Overview of your agents, their status, and recent activity."
      />

      {/* Stats grid */}
      <div className={styles.statsGrid}>
        {STATS.map(({ label, value, icon: Icon, change }) => (
          <Card key={label}>
            <div className={styles.statCard}>
              <div className={styles.statIcon}>
                <Icon size={20} strokeWidth={1.5} />
              </div>
              <div>
                <div className={styles.statValue}>{value}</div>
                <div className={styles.statLabel}>{label}</div>
                <div className={styles.statChange}>{change}</div>
              </div>
            </div>
          </Card>
        ))}
      </div>

      {/* Main content grid */}
      <div className={styles.mainGrid}>
        {/* Agents */}
        <Card>
          <div className={styles.sectionHeader}>
            <h2 className={styles.sectionTitle}>
              <Bot size={18} strokeWidth={1.5} />
              Agents
            </h2>
          </div>
          <div className={styles.agentList}>
            {MOCK_AGENTS.map(agent => (
              <div key={agent.id} className={styles.agentRow}>
                <div className={styles.agentInfo}>
                  <span className={styles.agentName}>{agent.name}</span>
                  <StatusBadge status={agent.status} />
                </div>
                <span className={styles.agentTime}>
                  <Clock size={12} />
                  {agent.lastActive}
                </span>
              </div>
            ))}
          </div>
        </Card>

        {/* Activity feed */}
        <Card>
          <div className={styles.sectionHeader}>
            <h2 className={styles.sectionTitle}>
              <Activity size={18} strokeWidth={1.5} />
              Recent Activity
            </h2>
          </div>
          <div className={styles.activityList}>
            {MOCK_ACTIVITY.map(item => (
              <div key={item.id} className={styles.activityRow}>
                <div
                  className={styles.activityDot}
                  data-type={item.type}
                />
                <div className={styles.activityContent}>
                  <span className={styles.activityText}>{item.text}</span>
                  <span className={styles.activityTime}>{item.time}</span>
                </div>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </div>
  )
}
