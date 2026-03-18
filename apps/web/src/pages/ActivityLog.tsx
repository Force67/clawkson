import { useState, useEffect, useCallback, useMemo } from 'react'
import {
  Loader2, ChevronLeft, ChevronRight,
  Terminal, Globe, Search, BookOpen, FolderOpen,
  FileEdit, List, Clock, Shield, ShieldAlert,
  Filter, X, Calendar, Bot,
} from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Button } from '../components/Button'
import {
  api,
  type ToolAuditEntry,
  type UserAuditStats,
  type Agent,
} from '../lib/api'
import styles from './ActivityLog.module.css'

// ── Helpers ────────────────────────────────────────────────────────

const TOOL_META: Record<string, { label: string; icon: typeof Terminal; category: string }> = {
  code_execution:    { label: 'Code Execution',    icon: Terminal,   category: 'compute' },
  workspace_read:    { label: 'Workspace Read',     icon: FolderOpen, category: 'workspace' },
  workspace_write:   { label: 'Workspace Write',    icon: FileEdit,   category: 'workspace' },
  workspace_list:    { label: 'Workspace List',     icon: List,       category: 'workspace' },
  knowledge_search:  { label: 'Knowledge Search',   icon: Search,     category: 'knowledge' },
  knowledge_list:    { label: 'Knowledge List',      icon: BookOpen,   category: 'knowledge' },
  web_search:        { label: 'Web Search',          icon: Globe,      category: 'network' },
  authenticated_http:{ label: 'HTTP Request',        icon: Globe,      category: 'network' },
  browser:           { label: 'Browser',             icon: Globe,      category: 'network' },
  start_preview:     { label: 'Start Preview',       icon: Globe,      category: 'network' },
  manage_scheduled_tasks: { label: 'Scheduled Tasks', icon: Clock,    category: 'system' },
  manage_calendar:   { label: 'Calendar',            icon: Calendar,   category: 'system' },
  create_skill:      { label: 'Create Skill',        icon: BookOpen,   category: 'system' },
}

function getToolMeta(name: string) {
  return TOOL_META[name] ?? { label: name, icon: Terminal, category: 'other' }
}

function formatDuration(ms: number | null): string {
  if (ms === null) return '-'
  if (ms < 1000) return `${ms}ms`
  const secs = Math.round(ms / 1000)
  if (secs < 60) return `${secs}s`
  return `${Math.floor(secs / 60)}m ${secs % 60}s`
}

function formatTimestamp(iso: string): string {
  const d = new Date(iso)
  const now = new Date()
  const diffMs = now.getTime() - d.getTime()
  const diffMins = Math.floor(diffMs / 60000)

  if (diffMins < 1) return 'Just now'
  if (diffMins < 60) return `${diffMins}m ago`

  const diffHours = Math.floor(diffMins / 60)
  if (diffHours < 24) return `${diffHours}h ago`

  const diffDays = Math.floor(diffHours / 24)
  if (diffDays < 7) return `${diffDays}d ago`

  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
}

function formatFullTimestamp(iso: string): string {
  const d = new Date(iso)
  return d.toLocaleString(undefined, {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', second: '2-digit',
  })
}

const TIME_RANGES = [
  { label: '1h', value: 1 },
  { label: '24h', value: 24 },
  { label: '7d', value: 168 },
  { label: '30d', value: 720 },
  { label: 'All', value: 0 },
]

const PAGE_SIZE = 50

// ── Stats Bar ──────────────────────────────────────────────────────

function StatsBar({ stats, loading }: { stats: UserAuditStats | null; loading: boolean }) {
  if (loading || !stats) {
    return (
      <div className={styles.statsBar}>
        <div className={styles.statsSkeleton} />
      </div>
    )
  }

  const deniedPct = stats.total > 0 ? Math.round((stats.denied / stats.total) * 100) : 0

  return (
    <div className={styles.statsBar}>
      <div className={styles.statsGroup}>
        <div className={styles.stat}>
          <span className={styles.statValue}>{stats.total.toLocaleString()}</span>
          <span className={styles.statLabel}>Total</span>
        </div>
        <div className={styles.statDivider} />
        <div className={styles.stat}>
          <span className={`${styles.statValue} ${styles.statAllowed}`}>{stats.allowed.toLocaleString()}</span>
          <span className={styles.statLabel}>Allowed</span>
        </div>
        <div className={styles.statDivider} />
        <div className={styles.stat}>
          <span className={`${styles.statValue} ${stats.denied > 0 ? styles.statDenied : ''}`}>{stats.denied.toLocaleString()}</span>
          <span className={styles.statLabel}>Denied{deniedPct > 0 ? ` (${deniedPct}%)` : ''}</span>
        </div>
      </div>

      {stats.by_tool.length > 0 && (
        <div className={styles.breakdownGroup}>
          {stats.by_tool.slice(0, 4).map(t => (
            <div key={t.key} className={styles.breakdownChip}>
              <span className={styles.breakdownKey}>{getToolMeta(t.key).label}</span>
              <span className={styles.breakdownCount}>{t.count}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

// ── Audit Entry Row ─────────────────────────────────────────────────

function AuditRow({ entry }: { entry: ToolAuditEntry }) {
  const meta = getToolMeta(entry.tool_name)
  const Icon = meta.icon
  const isDenied = entry.decision === 'denied'

  return (
    <div className={`${styles.auditRow} ${isDenied ? styles.auditRowDenied : ''}`}>
      {/* Timeline dot */}
      <div className={styles.timelineDot}>
        <div className={`${styles.dot} ${isDenied ? styles.dotDenied : styles.dotAllowed}`} />
      </div>

      {/* Tool icon */}
      <div className={`${styles.toolIcon} ${styles[`toolIcon_${meta.category}`] ?? ''}`}>
        <Icon size={14} />
      </div>

      {/* Main content */}
      <div className={styles.rowContent}>
        <div className={styles.rowTop}>
          <span className={styles.toolName}>{meta.label}</span>

          {entry.http_method && (
            <span className={styles.httpBadge}>
              {entry.http_method}
            </span>
          )}

          {entry.target_path && (
            <span className={styles.targetPath} title={entry.target_path}>
              {entry.target_path.length > 60
                ? entry.target_path.slice(0, 57) + '...'
                : entry.target_path}
            </span>
          )}
        </div>

        <div className={styles.rowBottom}>
          <span className={styles.agentTag}>
            <Bot size={10} />
            {entry.agent_name}
          </span>

          {entry.conversation_title && (
            <a
              href={`/conversations/${entry.conversation_id}`}
              className={styles.convLink}
              title={entry.conversation_title}
            >
              {entry.conversation_title.length > 30
                ? entry.conversation_title.slice(0, 27) + '...'
                : entry.conversation_title}
            </a>
          )}

          {isDenied && entry.denial_reason && (
            <span className={styles.denialReason} title={entry.denial_reason}>
              {entry.denial_reason.length > 50
                ? entry.denial_reason.slice(0, 47) + '...'
                : entry.denial_reason}
            </span>
          )}
        </div>
      </div>

      {/* Right side: decision + duration + timestamp */}
      <div className={styles.rowMeta}>
        <span className={`${styles.decisionBadge} ${isDenied ? styles.decisionDenied : styles.decisionAllowed}`}>
          {isDenied ? <ShieldAlert size={10} /> : <Shield size={10} />}
          {entry.decision}
        </span>

        <span className={styles.duration}>
          {formatDuration(entry.duration_ms)}
        </span>

        <span className={styles.timestamp} title={formatFullTimestamp(entry.created_at)}>
          {formatTimestamp(entry.created_at)}
        </span>
      </div>
    </div>
  )
}

// ── Main Page ───────────────────────────────────────────────────────

export function ActivityLogPage() {
  const [entries, setEntries] = useState<ToolAuditEntry[]>([])
  const [stats, setStats] = useState<UserAuditStats | null>(null)
  const [agents, setAgents] = useState<Agent[]>([])
  const [loading, setLoading] = useState(true)
  const [statsLoading, setStatsLoading] = useState(true)
  const [offset, setOffset] = useState(0)
  const [hasMore, setHasMore] = useState(true)

  // Filters
  const [timeRange, setTimeRange] = useState(24) // hours, 0 = all
  const [agentFilter, setAgentFilter] = useState<string>('')
  const [toolFilter, setToolFilter] = useState<string>('')
  const [decisionFilter, setDecisionFilter] = useState<string>('')
  const [showFilters, setShowFilters] = useState(false)

  const sinceISO = useMemo(() => {
    if (timeRange === 0) return undefined
    const d = new Date(Date.now() - timeRange * 3600_000)
    return d.toISOString()
  }, [timeRange])

  const hasActiveFilters = agentFilter || toolFilter || decisionFilter

  // Unique tool names from the loaded entries (for filter dropdown)
  const uniqueTools = useMemo(() => {
    const set = new Set<string>()
    entries.forEach(e => set.add(e.tool_name))
    return Array.from(set).sort()
  }, [entries])

  const loadEntries = useCallback(async (newOffset: number) => {
    setLoading(true)
    try {
      const rows = await api.auditLog.list({
        limit: PAGE_SIZE,
        offset: newOffset,
        agent_id: agentFilter || undefined,
        tool_name: toolFilter || undefined,
        decision: decisionFilter || undefined,
        since: sinceISO,
      })
      setEntries(rows)
      setHasMore(rows.length === PAGE_SIZE)
      setOffset(newOffset)
    } catch (err) {
      console.error('Failed to load audit log:', err)
    } finally {
      setLoading(false)
    }
  }, [agentFilter, toolFilter, decisionFilter, sinceISO])

  const loadStats = useCallback(async () => {
    setStatsLoading(true)
    try {
      const s = await api.auditLog.stats(sinceISO)
      setStats(s)
    } catch (err) {
      console.error('Failed to load audit stats:', err)
    } finally {
      setStatsLoading(false)
    }
  }, [sinceISO])

  // Initial load
  useEffect(() => {
    api.agents.list().then(setAgents).catch(() => {})
  }, [])

  // Reload when filters change
  useEffect(() => {
    loadEntries(0)
    loadStats()
  }, [loadEntries, loadStats])

  function clearFilters() {
    setAgentFilter('')
    setToolFilter('')
    setDecisionFilter('')
  }

  return (
    <>
      <PageHeader
        title="Activity Log"
        description="Audit trail of every tool call made by your agents."
        actions={
          <div className={styles.headerActions}>
            <Button
              variant={showFilters ? 'secondary' : 'ghost'}
              size="sm"
              onClick={() => setShowFilters(f => !f)}
            >
              <Filter size={13} />
              Filters
              {hasActiveFilters && <span className={styles.filterDot} />}
            </Button>
          </div>
        }
      />

      {/* Time range selector */}
      <div className={styles.controls}>
        <div className={styles.timeRange}>
          {TIME_RANGES.map(r => (
            <button
              key={r.value}
              className={`${styles.timeBtn} ${timeRange === r.value ? styles.timeBtnActive : ''}`}
              onClick={() => setTimeRange(r.value)}
            >
              {r.label}
            </button>
          ))}
        </div>

        <StatsBar stats={stats} loading={statsLoading} />
      </div>

      {/* Filter bar */}
      {showFilters && (
        <div className={styles.filterBar}>
          <div className={styles.filterGroup}>
            <label className={styles.filterLabel}>Agent</label>
            <select
              className={styles.filterSelect}
              value={agentFilter}
              onChange={e => setAgentFilter(e.target.value)}
            >
              <option value="">All agents</option>
              {agents.map(a => (
                <option key={a.id} value={a.id}>{a.name}</option>
              ))}
            </select>
          </div>

          <div className={styles.filterGroup}>
            <label className={styles.filterLabel}>Tool</label>
            <select
              className={styles.filterSelect}
              value={toolFilter}
              onChange={e => setToolFilter(e.target.value)}
            >
              <option value="">All tools</option>
              {Object.entries(TOOL_META).map(([key, meta]) => (
                <option key={key} value={key}>{meta.label}</option>
              ))}
              {uniqueTools
                .filter(t => !TOOL_META[t])
                .map(t => <option key={t} value={t}>{t}</option>)}
            </select>
          </div>

          <div className={styles.filterGroup}>
            <label className={styles.filterLabel}>Decision</label>
            <select
              className={styles.filterSelect}
              value={decisionFilter}
              onChange={e => setDecisionFilter(e.target.value)}
            >
              <option value="">All</option>
              <option value="allowed">Allowed</option>
              <option value="denied">Denied</option>
            </select>
          </div>

          {hasActiveFilters && (
            <button className={styles.clearFilters} onClick={clearFilters}>
              <X size={12} /> Clear
            </button>
          )}
        </div>
      )}

      {/* Event stream */}
      <div className={styles.streamContainer}>
        {loading && entries.length === 0 ? (
          <div className={styles.loadingState}>
            <Loader2 size={16} className="spinning" />
            <span>Loading activity...</span>
          </div>
        ) : entries.length === 0 ? (
          <div className={styles.emptyState}>
            <Shield size={32} className={styles.emptyIcon} />
            <div className={styles.emptyTitle}>No activity recorded</div>
            <div className={styles.emptyDesc}>
              {hasActiveFilters
                ? 'No entries match the current filters. Try broadening your search.'
                : 'Tool calls will appear here as your agents work.'}
            </div>
          </div>
        ) : (
          <>
            <div className={styles.timeline}>
              {entries.map(entry => (
                <AuditRow key={entry.id} entry={entry} />
              ))}
            </div>

            {/* Pagination */}
            <div className={styles.pagination}>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => loadEntries(Math.max(0, offset - PAGE_SIZE))}
                disabled={offset === 0}
              >
                <ChevronLeft size={14} /> Newer
              </Button>

              <span className={styles.pageInfo}>
                {offset + 1}–{offset + entries.length}
              </span>

              <Button
                variant="ghost"
                size="sm"
                onClick={() => loadEntries(offset + PAGE_SIZE)}
                disabled={!hasMore}
              >
                Older <ChevronRight size={14} />
              </Button>
            </div>
          </>
        )}
      </div>
    </>
  )
}
