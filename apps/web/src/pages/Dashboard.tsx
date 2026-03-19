import { useState, useEffect, useCallback, useMemo } from 'react'
import {
  Bot,
  MessageCircle,
  Plus,
  ChevronRight,
  Loader2,
  Search,
  Star,
  Bell,
  BellOff,
  Shield,
  Zap,
  XCircle,
  X,
  Check,
  BookOpen,
} from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { useAuth } from '../lib/auth'
import { api, type Agent, type Conversation, type LlmConnector, type KnowledgeBase, type ScheduledTask, type TaskExecution, type ToolAuditEntry } from '../lib/api'
import styles from './Dashboard.module.css'

// ── Notification types ──────────────────────────────────────────

type NotificationKind = 'agent' | 'task' | 'security' | 'knowledge' | 'system'
type NotificationLevel = 'info' | 'success' | 'warning' | 'error'

interface Notification {
  id: string
  kind: NotificationKind
  level: NotificationLevel
  title: string
  subtitle: string
  timestamp: string
  read: boolean
  /** Navigate to this route on click */
  href?: string
}

function synthesizeNotifications(
  agents: Agent[],
  tasks: ScheduledTask[],
  executions: Map<string, TaskExecution[]>,
  auditEntries: ToolAuditEntry[],
  knowledgeBases: KnowledgeBase[],
): Notification[] {
  const items: Notification[] = []

  // Agent status notifications
  for (const agent of agents) {
    if (agent.status === 'error') {
      items.push({
        id: `agent-error-${agent.id}`,
        kind: 'agent',
        level: 'error',
        title: `${agent.name} has an error`,
        subtitle: 'Check agent configuration and connector settings',
        timestamp: agent.updated_at,
        read: false,
        href: '/agents',
      })
    } else if (agent.status === 'busy') {
      items.push({
        id: `agent-busy-${agent.id}`,
        kind: 'agent',
        level: 'info',
        title: `${agent.name} is processing`,
        subtitle: 'Currently handling a conversation',
        timestamp: agent.updated_at,
        read: true,
        href: '/conversations',
      })
    } else if (agent.status === 'online') {
      items.push({
        id: `agent-online-${agent.id}`,
        kind: 'agent',
        level: 'success',
        title: `${agent.name} is online`,
        subtitle: agent.description || 'Ready to process conversations',
        timestamp: agent.updated_at,
        read: true,
        href: '/agents',
      })
    }
  }

  // Scheduled task execution notifications
  for (const task of tasks) {
    const taskExecs = executions.get(task.id) || []
    for (const exec of taskExecs.slice(0, 2)) {
      if (exec.status === 'completed') {
        items.push({
          id: `task-done-${exec.id}`,
          kind: 'task',
          level: 'success',
          title: `"${task.name}" completed`,
          subtitle: exec.result_summary
            ? exec.result_summary.slice(0, 80)
            : `Ran in ${exec.duration_ms ? `${(exec.duration_ms / 1000).toFixed(1)}s` : 'unknown'}`,
          timestamp: exec.completed_at || exec.started_at,
          read: false,
          href: '/scheduled-tasks',
        })
      } else if (exec.status === 'failed') {
        items.push({
          id: `task-fail-${exec.id}`,
          kind: 'task',
          level: 'error',
          title: `"${task.name}" failed`,
          subtitle: exec.error_message?.slice(0, 80) || 'Execution error',
          timestamp: exec.completed_at || exec.started_at,
          read: false,
          href: '/scheduled-tasks',
        })
      }
    }
  }

  // Security / audit denial notifications
  for (const entry of auditEntries) {
    if (entry.decision === 'denied') {
      items.push({
        id: `audit-${entry.id}`,
        kind: 'security',
        level: 'warning',
        title: `${entry.tool_name} call denied`,
        subtitle: entry.denial_reason || `Policy violation on ${entry.agent_name}`,
        timestamp: entry.created_at,
        read: false,
        href: '/activity-log',
      })
    }
  }

  // Knowledge base activity
  for (const kb of knowledgeBases) {
    const updated = new Date(kb.updated_at)
    const hourAgo = Date.now() - 3600000
    if (updated.getTime() > hourAgo && kb.entry_count > 0) {
      items.push({
        id: `kb-${kb.id}`,
        kind: 'knowledge',
        level: 'info',
        title: `${kb.name} updated`,
        subtitle: `${kb.entry_count} entries`,
        timestamp: kb.updated_at,
        read: true,
        href: '/knowledge',
      })
    }
  }

  // Sort by timestamp descending
  items.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime())
  return items
}

// ── Notification filter chips ────────────────────────────────────

type NotificationFilter = 'all' | NotificationKind

const FILTER_OPTIONS: { key: NotificationFilter; label: string }[] = [
  { key: 'all', label: 'All' },
  { key: 'agent', label: 'Agents' },
  { key: 'task', label: 'Tasks' },
  { key: 'security', label: 'Security' },
  { key: 'knowledge', label: 'Knowledge' },
]

function kindIcon(kind: NotificationKind, level: NotificationLevel) {
  const size = 14
  const strokeWidth = 1.5
  switch (kind) {
    case 'agent': return <Bot size={size} strokeWidth={strokeWidth} />
    case 'task':
      return level === 'error'
        ? <XCircle size={size} strokeWidth={strokeWidth} />
        : <Zap size={size} strokeWidth={strokeWidth} />
    case 'security': return <Shield size={size} strokeWidth={strokeWidth} />
    case 'knowledge': return <BookOpen size={size} strokeWidth={strokeWidth} />
    case 'system': return <Bell size={size} strokeWidth={strokeWidth} />
  }
}

function levelColor(level: NotificationLevel): string {
  switch (level) {
    case 'success': return 'var(--success)'
    case 'warning': return 'var(--warning)'
    case 'error': return 'var(--error)'
    case 'info': return 'var(--accent)'
  }
}

// ── Page ──────────────────────────────────────────────────────────

export function DashboardPage() {
  const [agents, setAgents] = useState<Agent[]>([])
  const [conversations, setConversations] = useState<Conversation[]>([])
  const [connectors, setConnectors] = useState<LlmConnector[]>([])
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBase[]>([])
  const [scheduledTasks, setScheduledTasks] = useState<ScheduledTask[]>([])
  const [taskExecutions, setTaskExecutions] = useState<Map<string, TaskExecution[]>>(new Map())
  const [auditEntries, setAuditEntries] = useState<ToolAuditEntry[]>([])
  const [loading, setLoading] = useState(true)
  const navigate = useNavigate()
  const { user } = useAuth()

  // Notification state
  const [dismissedIds, setDismissedIds] = useState<Set<string>>(() => {
    try {
      const saved = localStorage.getItem('clawkson_dismissed_notifs')
      return saved ? new Set(JSON.parse(saved)) : new Set()
    } catch { return new Set() }
  })
  const [readIds, setReadIds] = useState<Set<string>>(() => {
    try {
      const saved = localStorage.getItem('clawkson_read_notifs')
      return saved ? new Set(JSON.parse(saved)) : new Set()
    } catch { return new Set() }
  })
  const [filter, setFilter] = useState<NotificationFilter>('all')

  useEffect(() => {
    Promise.all([
      api.agents.list(),
      api.conversations.list(),
      api.llmConnectors.list(),
      api.knowledge.listBases(),
      api.scheduledTasks.list().catch(() => [] as ScheduledTask[]),
      api.auditLog.list({ limit: 20, decision: 'denied' }).catch(() => [] as ToolAuditEntry[]),
    ])
      .then(([agts, convos, conns, kbs, tasks, audit]) => {
        setAgents(agts)
        setConversations(convos)
        setConnectors(conns)
        setKnowledgeBases(kbs)
        setScheduledTasks(tasks)
        setAuditEntries(audit)

        // Fetch recent executions for each task
        const execPromises = tasks.slice(0, 10).map(t =>
          api.scheduledTasks.history(t.id)
            .then(execs => ({ id: t.id, execs: execs.slice(0, 3) }))
            .catch(() => ({ id: t.id, execs: [] as TaskExecution[] }))
        )
        Promise.all(execPromises).then(results => {
          const map = new Map<string, TaskExecution[]>()
          for (const r of results) map.set(r.id, r.execs)
          setTaskExecutions(map)
        })
      })
      .finally(() => setLoading(false))
  }, [])

  // Synthesize and filter notifications
  const allNotifications = useMemo(() =>
    synthesizeNotifications(agents, scheduledTasks, taskExecutions, auditEntries, knowledgeBases),
    [agents, scheduledTasks, taskExecutions, auditEntries, knowledgeBases]
  )

  const visibleNotifications = useMemo(() => {
    let items = allNotifications.filter(n => !dismissedIds.has(n.id))
    if (filter !== 'all') items = items.filter(n => n.kind === filter)
    // Apply read state from localStorage
    return items.map(n => ({
      ...n,
      read: n.read || readIds.has(n.id),
    }))
  }, [allNotifications, dismissedIds, readIds, filter])

  const unreadCount = useMemo(() =>
    allNotifications.filter(n => !dismissedIds.has(n.id) && !n.read && !readIds.has(n.id)).length,
    [allNotifications, dismissedIds, readIds]
  )

  const persistDismissed = useCallback((ids: Set<string>) => {
    localStorage.setItem('clawkson_dismissed_notifs', JSON.stringify([...ids]))
  }, [])

  const persistRead = useCallback((ids: Set<string>) => {
    localStorage.setItem('clawkson_read_notifs', JSON.stringify([...ids]))
  }, [])

  const dismissNotification = useCallback((id: string) => {
    setDismissedIds(prev => {
      const next = new Set(prev)
      next.add(id)
      persistDismissed(next)
      return next
    })
  }, [persistDismissed])

  const markAllRead = useCallback(() => {
    const unreadIds = allNotifications
      .filter(n => !dismissedIds.has(n.id) && !n.read && !readIds.has(n.id))
      .map(n => n.id)
    setReadIds(prev => {
      const next = new Set(prev)
      for (const id of unreadIds) next.add(id)
      persistRead(next)
      return next
    })
  }, [allNotifications, dismissedIds, readIds, persistRead])

  const handleNotificationClick = useCallback((n: Notification) => {
    // Mark as read
    if (!n.read && !readIds.has(n.id)) {
      setReadIds(prev => {
        const next = new Set(prev)
        next.add(n.id)
        persistRead(next)
        return next
      })
    }
    if (n.href) navigate(n.href)
  }, [navigate, readIds, persistRead])

  const onlineCount = agents.filter(a => a.status === 'online').length
  const busyCount = agents.filter(a => a.status === 'busy').length
  const activePercent = agents.length > 0 ? Math.round((onlineCount + busyCount) / agents.length * 100) : 0
  const totalKbEntries = knowledgeBases.reduce((sum, kb) => sum + kb.entry_count, 0)

  // Top agents sorted by conversation count
  const agentActivity = agents.map(a => ({
    ...a,
    convos: conversations.filter(c => c.agent_id === a.id).length,
  })).sort((a, b) => b.convos - a.convos)

  const agentScore = (a: typeof agentActivity[0]) => {
    const bonus = a.status === 'online' ? 1 : a.status === 'busy' ? 0.5 : 0
    return Math.min(5, 3 + a.convos * 0.3 + bonus).toFixed(1)
  }

  // Decorative trend bars
  const trendBars = Array.from({ length: 28 }, (_, i) => ({
    height: Math.max(12, Math.min(95, 30 + ((i * 7 + 5) % 13) * 5 + Math.sin(i * 0.5) * 18)),
    accent: i % 6 === 0,
  }))

  const statusColor = (s: string) =>
    s === 'online' ? 'var(--success)' : s === 'busy' ? 'var(--warning)' : s === 'error' ? 'var(--error)' : 'var(--text-tertiary)'

  return (
    <div className="fade-in">
      {/* Header */}
      <div className={styles.header}>
        <h1 className={styles.title}>// Overview</h1>
        <div className={styles.headerRight}>
          <div className={styles.searchBox}>
            <Search size={14} />
            <input placeholder="Search..." className={styles.searchInput} />
          </div>
          {/* Notification bell */}
          <button className={styles.bellButton} onClick={() => {
            const el = document.getElementById('notif-box')
            el?.scrollIntoView({ behavior: 'smooth', block: 'center' })
          }}>
            <Bell size={16} />
            {unreadCount > 0 && (
              <span className={styles.bellBadge}>
                {unreadCount > 9 ? '9+' : unreadCount}
              </span>
            )}
          </button>
          {user && (
            <div className={styles.headerAvatar}>
              {user.display_name.charAt(0).toUpperCase()}
            </div>
          )}
        </div>
      </div>

      {/* ── Bento Grid ── */}
      <div className={styles.bento}>

        {/* Hero: 2×2 — Conversations overview */}
        <div className={`${styles.cell} ${styles.hero}`}>
          <span className={styles.cellLabel}>Conversations</span>
          <div className={styles.heroTop}>
            <div>
              <div className={styles.heroStat}>
                {loading ? '\u2014' : String(conversations.length).padStart(2, '0')}
              </div>
              <div className={styles.heroStatSub}>Total conversations across all agents</div>
            </div>
            <div className={styles.activeBadge}>
              <span className={styles.activeDot} />
              {activePercent}% Active
            </div>
          </div>
          <div className={styles.activityWrap}>
            <div className={styles.activityMeta}>
              <span className={styles.activityMetaLabel}>Agent Availability</span>
              <span className={styles.activityMetaValue}>
                {onlineCount + busyCount}/{agents.length}
              </span>
            </div>
            <div className={styles.activityBar}>
              <div
                className={styles.activityBarFill}
                style={{ width: `${Math.max(activePercent, 8)}%` }}
              >
                <span className={styles.activityBarText}>Active</span>
              </div>
            </div>
          </div>
        </div>

        {/* Top Agents: 1×2 */}
        <div className={`${styles.cell} ${styles.topAgents}`}>
          <span className={styles.cellLabel}>Top Agents</span>
          {loading ? (
            <div className={styles.topAgentsEmpty}><Loader2 size={16} className="spinning" /></div>
          ) : agents.length === 0 ? (
            <div className={styles.topAgentsEmpty}>
              <Bot size={20} strokeWidth={1} />
              <span>No agents yet</span>
            </div>
          ) : (
            <div className={styles.topAgentsList}>
              {agentActivity.slice(0, 5).map(agent => (
                <div key={agent.id} className={styles.topAgentRow} onClick={() => navigate('/agents')}>
                  <div className={styles.topAgentAvatar}>
                    <Bot size={14} strokeWidth={1.5} />
                    <div className={styles.topAgentDot} style={{ background: statusColor(agent.status) }} />
                  </div>
                  <div className={styles.topAgentInfo}>
                    <span className={styles.topAgentName}>{agent.name}</span>
                    <span className={styles.topAgentDesc}>{agent.description || 'No description'}</span>
                  </div>
                  <div className={styles.topAgentScore}>
                    <Star size={11} fill="currentColor" /> {agentScore(agent)}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Metric: Total Agents */}
        <div className={`${styles.cell} ${styles.metric}`}>
          <span className={styles.cellLabel}>Agents</span>
          <div className={styles.metricValue}>{loading ? '\u2014' : String(agents.length).padStart(3, '0')}</div>
          <div className={styles.metricSub}>{onlineCount} online, {busyCount} busy</div>
          <div className={styles.metricAccent} style={{ background: 'var(--accent)' }} />
        </div>

        {/* Metric: Active Now */}
        <div className={`${styles.cell} ${styles.metric}`}>
          <span className={styles.cellLabel}>Active Now</span>
          <div className={styles.metricValue}>{loading ? '\u2014' : String(onlineCount + busyCount).padStart(3, '0')}</div>
          <div className={styles.metricSub}>{activePercent}% of fleet</div>
          <div className={styles.metricAccent} style={{ background: 'var(--success)' }} />
        </div>

        {/* Trends: 2×1 */}
        <div className={`${styles.cell} ${styles.trends}`}>
          <div className={styles.trendsHeader}>
            <span className={styles.cellLabel} style={{ marginBottom: 0 }}>Trends Over Time</span>
            <div className={styles.trendsLegend}>
              <span><span className={styles.legendDot} style={{ background: '#34d399' }} />msgs</span>
              <span><span className={styles.legendDot} style={{ background: '#fbbf24' }} />tools</span>
            </div>
          </div>
          <div className={styles.barsChart}>
            {trendBars.map((bar, i) => (
              <div
                key={i}
                className={styles.bar}
                style={{
                  height: `${bar.height}%`,
                  background: bar.accent ? '#fbbf24' : '#34d399',
                  animationDelay: `${i * 25}ms`,
                }}
              />
            ))}
          </div>
        </div>

        {/* Metric: Knowledge */}
        <div className={`${styles.cell} ${styles.metric}`}>
          <span className={styles.cellLabel}>Knowledge</span>
          <div className={styles.metricValue}>{loading ? '\u2014' : String(totalKbEntries).padStart(3, '0')}</div>
          <div className={styles.metricSub}>{knowledgeBases.length} bases</div>
          <div className={styles.metricAccent} style={{ background: 'var(--info)' }} />
        </div>

        {/* Metric: Connectors */}
        <div className={`${styles.cell} ${styles.metric}`}>
          <span className={styles.cellLabel}>LLM Connectors</span>
          <div className={styles.metricValue}>{loading ? '\u2014' : String(connectors.length).padStart(3, '0')}</div>
          <div className={styles.metricSub}>Configured</div>
          <div className={styles.metricAccent} style={{ background: 'var(--warning)' }} />
        </div>

        {/* ── Notifications: 3×1 ── */}
        <div id="notif-box" className={`${styles.cell} ${styles.notifications}`}>
          <div className={styles.notifHeader}>
            <div className={styles.notifTitleRow}>
              <span className={styles.cellLabel} style={{ marginBottom: 0 }}>Notifications</span>
              {unreadCount > 0 && (
                <span className={styles.notifBadge}>{unreadCount}</span>
              )}
            </div>
            <div className={styles.notifActions}>
              {unreadCount > 0 && (
                <button className={styles.notifMarkRead} onClick={markAllRead}>
                  <Check size={11} /> Mark all read
                </button>
              )}
            </div>
          </div>

          {/* Filter chips */}
          <div className={styles.notifFilters}>
            {FILTER_OPTIONS.map(opt => (
              <button
                key={opt.key}
                className={`${styles.notifChip} ${filter === opt.key ? styles.notifChipActive : ''}`}
                onClick={() => setFilter(opt.key)}
              >
                {opt.label}
                {opt.key !== 'all' && (() => {
                  const count = allNotifications.filter(n =>
                    !dismissedIds.has(n.id) && n.kind === opt.key && !n.read && !readIds.has(n.id)
                  ).length
                  return count > 0 ? <span className={styles.notifChipCount}>{count}</span> : null
                })()}
              </button>
            ))}
          </div>

          {/* Notification list */}
          <div className={styles.notifList}>
            {loading ? (
              <div className={styles.notifEmpty}>
                <Loader2 size={16} className="spinning" />
              </div>
            ) : visibleNotifications.length === 0 ? (
              <div className={styles.notifEmpty}>
                <div className={styles.notifEmptyIcon}>
                  <BellOff size={24} strokeWidth={1} />
                </div>
                <span className={styles.notifEmptyTitle}>All clear</span>
                <span className={styles.notifEmptyDesc}>
                  {filter === 'all' ? 'No notifications right now' : `No ${filter} notifications`}
                </span>
              </div>
            ) : (
              <div className="stagger">
                {visibleNotifications.slice(0, 8).map(notif => (
                  <div
                    key={notif.id}
                    className={`${styles.notifItem} ${!notif.read ? styles.notifItemUnread : ''}`}
                    onClick={() => handleNotificationClick(notif)}
                  >
                    <div
                      className={styles.notifItemBar}
                      style={{ background: levelColor(notif.level) }}
                    />
                    <div
                      className={styles.notifItemIcon}
                      style={{ color: levelColor(notif.level) }}
                    >
                      {kindIcon(notif.kind, notif.level)}
                    </div>
                    <div className={styles.notifItemContent}>
                      <div className={styles.notifItemTitle}>
                        {!notif.read && <span className={styles.notifUnreadDot} />}
                        {notif.title}
                      </div>
                      <div className={styles.notifItemSub}>{notif.subtitle}</div>
                    </div>
                    <div className={styles.notifItemMeta}>
                      <span className={styles.notifItemTime}>{relativeTime(notif.timestamp)}</span>
                      <button
                        className={styles.notifDismiss}
                        onClick={e => { e.stopPropagation(); dismissNotification(notif.id) }}
                        title="Dismiss"
                      >
                        <X size={12} />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {visibleNotifications.length > 8 && (
            <button className={styles.viewAll} onClick={() => navigate('/activity-log')}>
              View all ({visibleNotifications.length}) <ChevronRight size={12} />
            </button>
          )}
        </div>

        {/* CTA: New Agent */}
        <div className={`${styles.cell} ${styles.cta}`} onClick={() => navigate('/agents')}>
          <div className={styles.ctaIcon}><Plus size={22} strokeWidth={2} /></div>
          <span className={styles.ctaText}>Manage Agents</span>
        </div>

        {/* Recent Conversations: 4×1 */}
        <div className={`${styles.cell} ${styles.convos}`}>
          <span className={styles.cellLabel}>Recent</span>
          {conversations.length === 0 && !loading ? (
            <div className={styles.convoEmpty}>No conversations yet</div>
          ) : (
            <div className={styles.convoList}>
              {conversations.slice(0, 4).map(convo => (
                <div key={convo.id} className={styles.convoRow} onClick={() => navigate(`/conversations/${convo.id}`)}>
                  <div className={styles.convoIcon}><MessageCircle size={13} /></div>
                  <span className={styles.convoText}>{convo.title}</span>
                  <span className={styles.convoTime}>{relativeTime(convo.updated_at)}</span>
                </div>
              ))}
            </div>
          )}
          <button className={styles.viewAll} onClick={() => navigate('/conversations')}>
            View all <ChevronRight size={12} />
          </button>
        </div>
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
