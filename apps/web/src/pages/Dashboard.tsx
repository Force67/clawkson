import { useState, useEffect } from 'react'
import {
  Bot,
  MessageCircle,
  Plus,
  ChevronRight,
  Loader2,
  Cpu,
  Thermometer,
  Hash,
  Container,
  Search,
  Star,
} from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { useAuth } from '../lib/auth'
import { api, type Agent, type Conversation, type LlmConnector, type KnowledgeBase } from '../lib/api'
import styles from './Dashboard.module.css'

// ── Page ──────────────────────────────────────────────────────────

export function DashboardPage() {
  const [agents, setAgents] = useState<Agent[]>([])
  const [conversations, setConversations] = useState<Conversation[]>([])
  const [connectors, setConnectors] = useState<LlmConnector[]>([])
  const [knowledgeBases, setKnowledgeBases] = useState<KnowledgeBase[]>([])
  const [loading, setLoading] = useState(true)
  const navigate = useNavigate()
  const { user } = useAuth()

  useEffect(() => {
    Promise.all([api.agents.list(), api.conversations.list(), api.llmConnectors.list(), api.knowledge.listBases()])
      .then(([agts, convos, conns, kbs]) => {
        setAgents(agts)
        setConversations(convos)
        setConnectors(conns)
        setKnowledgeBases(kbs)
      })
      .finally(() => setLoading(false))
  }, [])

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

        {/* CTA: New Agent */}
        <div className={`${styles.cell} ${styles.cta}`} onClick={() => navigate('/agents')}>
          <div className={styles.ctaIcon}><Plus size={22} strokeWidth={2} /></div>
          <span className={styles.ctaText}>Manage Agents</span>
        </div>

        {/* Recent Conversations: 3×1 */}
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
