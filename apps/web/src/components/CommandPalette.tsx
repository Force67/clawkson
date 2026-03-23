import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { useNavigate, useLocation } from 'react-router-dom'
import {
  Search, MessageSquare, Bot, Library, Loader2,
  ArrowUp, ArrowDown, CornerDownLeft, Brain, FileText,
  Gauge, MessagesSquare, CalendarDays, Timer, Webhook,
  Sparkles, Cog, Cable, KeyRound, Container, Activity,
  SlidersHorizontal, ScrollText, Plus,
  type LucideIcon,
} from 'lucide-react'
import { api, type GlobalSearchResponse } from '../lib/api'
import styles from './CommandPalette.module.css'

// ── Quick actions (always available, client-side filtered) ────────

interface QuickAction {
  id: string
  label: string
  group: 'Actions' | 'Navigation'
  icon: LucideIcon
  path: string
  keywords?: string
}

const QUICK_ACTIONS: QuickAction[] = [
  // Actions
  { id: 'act-new-agent', label: 'Create New Agent',       group: 'Actions',    icon: Plus,              path: '/agents?new=1',           keywords: 'add agent bot' },
  { id: 'act-new-conv',  label: 'New Conversation',       group: 'Actions',    icon: Plus,              path: '/conversations?new=1',    keywords: 'add chat message' },
  { id: 'act-new-kb',    label: 'Create Knowledge Base',  group: 'Actions',    icon: Plus,              path: '/knowledge?new=1',        keywords: 'add knowledge documents' },
  // Navigation
  { id: 'nav-dashboard',     label: 'Dashboard',         group: 'Navigation', icon: Gauge,             path: '/dashboard' },
  { id: 'nav-agents',        label: 'Agents',            group: 'Navigation', icon: Bot,               path: '/agents' },
  { id: 'nav-conversations', label: 'Conversations',     group: 'Navigation', icon: MessagesSquare,    path: '/conversations' },
  { id: 'nav-calendar',      label: 'Calendar',          group: 'Navigation', icon: CalendarDays,      path: '/calendar' },
  { id: 'nav-scheduled',     label: 'Scheduled Tasks',   group: 'Navigation', icon: Timer,             path: '/scheduled-tasks' },
  { id: 'nav-webhooks',      label: 'Webhooks',          group: 'Navigation', icon: Webhook,           path: '/webhooks' },
  { id: 'nav-knowledge',     label: 'Knowledge Base',    group: 'Navigation', icon: Library,           path: '/knowledge' },
  { id: 'nav-skills',        label: 'Skills',            group: 'Navigation', icon: Sparkles,          path: '/skills' },
  { id: 'nav-tools',         label: 'Tools',             group: 'Navigation', icon: Cog,               path: '/tools' },
  { id: 'nav-connectors',    label: 'Connectors',        group: 'Navigation', icon: Cable,             path: '/connectors' },
  { id: 'nav-credentials',   label: 'Credentials',       group: 'Navigation', icon: KeyRound,          path: '/credentials' },
  { id: 'nav-containers',    label: 'Containers',        group: 'Navigation', icon: Container,         path: '/containers' },
  { id: 'nav-activity',      label: 'Activity Log',      group: 'Navigation', icon: Activity,          path: '/activity-log' },
  { id: 'nav-settings',      label: 'Settings',          group: 'Navigation', icon: SlidersHorizontal, path: '/settings',   keywords: 'preferences config theme llm' },
  { id: 'nav-docs',          label: 'Documentation',     group: 'Navigation', icon: ScrollText,        path: '/docs' },
]

// ── Unified item type for keyboard navigation ────────────────────

type SpotlightItem =
  | { kind: 'action'; action: QuickAction }
  | { kind: 'conversation'; data: { id: string; title: string; agent_name: string | null; message_snippet: string | null; updated_at: string } }
  | { kind: 'agent'; data: { id: string; name: string; description: string; status: string } }
  | { kind: 'kb'; data: { id: string; name: string; description: string | null; entry_count: number } }
  | { kind: 'entry'; data: { id: string; knowledge_base_id: string; kb_name: string; title: string; content: string; score: number; source: 'memory' | 'standard' } }

// ── Component ────────────────────────────────────────────────────

interface Props {
  open: boolean
  onClose: () => void
}

export function CommandPalette({ open, onClose }: Props) {
  const navigate = useNavigate()
  const location = useLocation()
  const inputRef = useRef<HTMLInputElement>(null)
  const resultsRef = useRef<HTMLDivElement>(null)
  const debounceRef = useRef<ReturnType<typeof setTimeout>>()

  const [query, setQuery] = useState('')
  const [results, setResults] = useState<GlobalSearchResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [selectedIndex, setSelectedIndex] = useState(0)

  // ── Client-side filtered quick actions ──────────────────────────

  const filteredActions = useMemo(() => {
    if (!query.trim()) return QUICK_ACTIONS
    const q = query.toLowerCase()
    return QUICK_ACTIONS.filter(a =>
      a.label.toLowerCase().includes(q) ||
      (a.keywords && a.keywords.toLowerCase().includes(q))
    )
  }, [query])

  // When there's no query, only show actions (no nav clutter)
  // When searching, show matching nav items too
  const visibleActions = useMemo(() => {
    if (!query.trim()) return filteredActions.filter(a => a.group === 'Actions')
    return filteredActions
  }, [query, filteredActions])

  // ── Build flat item list for keyboard nav ──────────────────────

  const flatItems = useMemo(() => {
    const items: SpotlightItem[] = []

    // Actions/nav first
    for (const a of visibleActions) {
      items.push({ kind: 'action', action: a })
    }

    if (results) {
      for (const c of results.conversations) {
        items.push({ kind: 'conversation', data: c })
      }
      for (const a of results.agents) {
        items.push({ kind: 'agent', data: a })
      }
      for (const kb of results.knowledge_bases) {
        items.push({ kind: 'kb', data: kb })
      }
      for (const e of results.knowledge_entries) {
        items.push({ kind: 'entry', data: e })
      }
    }

    return items
  }, [visibleActions, results])

  // ── Reset on open ──────────────────────────────────────────────

  useEffect(() => {
    if (open) {
      setQuery('')
      setResults(null)
      setSelectedIndex(0)
      setLoading(false)
      setTimeout(() => inputRef.current?.focus(), 50)
    }
  }, [open])

  // ── Debounced API search ───────────────────────────────────────

  const doSearch = useCallback(async (q: string) => {
    if (q.trim().length < 2) {
      setResults(null)
      setLoading(false)
      return
    }
    setLoading(true)
    try {
      const data = await api.search.global(q, 6)
      setResults(data)
    } catch {
      setResults(null)
    }
    setLoading(false)
  }, [])

  const handleInput = useCallback((value: string) => {
    setQuery(value)
    setSelectedIndex(0)
    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = setTimeout(() => doSearch(value), 200)
  }, [doSearch])

  // ── Execute an item ────────────────────────────────────────────

  const execute = useCallback((item: SpotlightItem) => {
    onClose()
    switch (item.kind) {
      case 'action':
        navigate(item.action.path)
        break
      case 'conversation':
        navigate(`/conversations/${item.data.id}`)
        break
      case 'agent':
        navigate('/agents')
        break
      case 'kb':
        navigate('/knowledge')
        break
      case 'entry':
        navigate('/knowledge')
        break
    }
  }, [navigate, onClose])

  // ── Keyboard ───────────────────────────────────────────────────

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') { e.preventDefault(); onClose(); return }
    if (e.key === 'ArrowDown') { e.preventDefault(); setSelectedIndex(i => Math.min(i + 1, flatItems.length - 1)); return }
    if (e.key === 'ArrowUp') { e.preventDefault(); setSelectedIndex(i => Math.max(i - 1, 0)); return }
    if (e.key === 'Enter' && flatItems.length > 0) {
      e.preventDefault()
      const item = flatItems[selectedIndex]
      if (item) execute(item)
    }
  }, [flatItems, selectedIndex, execute, onClose])

  // ── Scroll selected into view ──────────────────────────────────

  useEffect(() => {
    const el = resultsRef.current?.querySelector(`[data-idx="${selectedIndex}"]`) as HTMLElement
    if (el) el.scrollIntoView({ block: 'nearest' })
  }, [selectedIndex])

  if (!open) return null

  // ── Group visible actions ──────────────────────────────────────

  const actionGroups = new Map<string, QuickAction[]>()
  for (const a of visibleActions) {
    let arr = actionGroups.get(a.group)
    if (!arr) { arr = []; actionGroups.set(a.group, arr) }
    arr.push(a)
  }

  // Track index across all sections
  let idx = 0

  const hasSearchResults = results &&
    (results.conversations.length > 0 || results.agents.length > 0 || results.knowledge_bases.length > 0 || results.knowledge_entries.length > 0)

  const showEmptySearch = query.trim().length >= 2 && !loading && !hasSearchResults && visibleActions.length === 0

  return (
    <div className={styles.overlay} onMouseDown={e => { if (e.target === e.currentTarget) onClose() }}>
      <div className={styles.palette} onKeyDown={handleKeyDown}>
        {/* Input */}
        <div className={styles.inputWrap}>
          <Search size={18} className={styles.inputIcon} />
          <input
            ref={inputRef}
            className={styles.input}
            value={query}
            onChange={e => handleInput(e.target.value)}
            placeholder="Search or jump to..."
            autoComplete="off"
            spellCheck={false}
          />
          {loading && <Loader2 size={16} className={styles.inputSpinner} />}
          <kbd className={styles.kbd}>ESC</kbd>
        </div>

        {/* Results */}
        <div className={styles.results} ref={resultsRef}>
          {showEmptySearch && (
            <div className={styles.empty}>No results for &ldquo;{query}&rdquo;</div>
          )}

          {/* Quick actions / navigation */}
          {Array.from(actionGroups).map(([group, actions]) => (
            <div key={group} className={styles.section}>
              <div className={styles.sectionLabel}>{group}</div>
              {actions.map(a => {
                const i = idx++
                const Icon = a.icon
                const isCurrent = location.pathname === a.path ||
                  location.pathname.startsWith(a.path.split('?')[0] + '/')
                return (
                  <div
                    key={a.id}
                    data-idx={i}
                    className={`${styles.item} ${selectedIndex === i ? styles.itemSelected : ''}`}
                    onClick={() => execute({ kind: 'action', action: a })}
                    onMouseEnter={() => setSelectedIndex(i)}
                  >
                    <div className={styles.itemIconSmall}>
                      <Icon size={16} />
                    </div>
                    <span className={styles.itemLabel}>{a.label}</span>
                    {isCurrent && <span className={styles.currentBadge}>current</span>}
                  </div>
                )
              })}
            </div>
          ))}

          {/* Conversations */}
          {results && results.conversations.length > 0 && (
            <div className={styles.section}>
              <div className={styles.sectionLabel}>
                <MessageSquare size={12} />
                Conversations
              </div>
              {results.conversations.map(c => {
                const i = idx++
                return (
                  <div
                    key={c.id}
                    data-idx={i}
                    className={`${styles.item} ${selectedIndex === i ? styles.itemSelected : ''}`}
                    onClick={() => execute({ kind: 'conversation', data: c })}
                    onMouseEnter={() => setSelectedIndex(i)}
                  >
                    <div className={`${styles.itemIcon} ${styles.itemIconConv}`}>
                      <MessageSquare size={16} />
                    </div>
                    <div className={styles.itemBody}>
                      <div className={styles.itemName}>{c.title}</div>
                      {c.agent_name && <div className={styles.itemSub}>{c.agent_name}</div>}
                      {c.message_snippet && (
                        <div className={styles.itemSnippet}>
                          &ldquo;{c.message_snippet.slice(0, 120)}{c.message_snippet.length > 120 ? '...' : ''}&rdquo;
                        </div>
                      )}
                    </div>
                    <div className={styles.itemMeta}>
                      {new Date(c.updated_at).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}
                    </div>
                  </div>
                )
              })}
            </div>
          )}

          {/* Agents */}
          {results && results.agents.length > 0 && (
            <div className={styles.section}>
              <div className={styles.sectionLabel}>
                <Bot size={12} />
                Agents
              </div>
              {results.agents.map(a => {
                const i = idx++
                return (
                  <div
                    key={a.id}
                    data-idx={i}
                    className={`${styles.item} ${selectedIndex === i ? styles.itemSelected : ''}`}
                    onClick={() => execute({ kind: 'agent', data: a })}
                    onMouseEnter={() => setSelectedIndex(i)}
                  >
                    <div className={`${styles.itemIcon} ${styles.itemIconAgent}`}>
                      <Bot size={16} />
                    </div>
                    <div className={styles.itemBody}>
                      <div className={styles.itemName}>{a.name}</div>
                      {a.description && <div className={styles.itemSub}>{a.description.slice(0, 80)}</div>}
                    </div>
                    <div className={styles.itemMeta}>{a.status}</div>
                  </div>
                )
              })}
            </div>
          )}

          {/* Knowledge Bases */}
          {results && results.knowledge_bases.length > 0 && (
            <div className={styles.section}>
              <div className={styles.sectionLabel}>
                <Library size={12} />
                Knowledge
              </div>
              {results.knowledge_bases.map(kb => {
                const i = idx++
                return (
                  <div
                    key={kb.id}
                    data-idx={i}
                    className={`${styles.item} ${selectedIndex === i ? styles.itemSelected : ''}`}
                    onClick={() => execute({ kind: 'kb', data: kb })}
                    onMouseEnter={() => setSelectedIndex(i)}
                  >
                    <div className={`${styles.itemIcon} ${styles.itemIconKb}`}>
                      <Library size={16} />
                    </div>
                    <div className={styles.itemBody}>
                      <div className={styles.itemName}>{kb.name}</div>
                      {kb.description && <div className={styles.itemSub}>{kb.description.slice(0, 80)}</div>}
                    </div>
                    <div className={styles.itemMeta}>{kb.entry_count} entries</div>
                  </div>
                )
              })}
            </div>
          )}

          {/* Semantic matches (memory + knowledge entries) */}
          {results && results.knowledge_entries.length > 0 && (
            <div className={styles.section}>
              <div className={styles.sectionLabel}>
                <Brain size={12} />
                Semantic Matches
              </div>
              {results.knowledge_entries.map(e => {
                const i = idx++
                const isMemory = e.source === 'memory'
                return (
                  <div
                    key={e.id}
                    data-idx={i}
                    className={`${styles.item} ${selectedIndex === i ? styles.itemSelected : ''}`}
                    onClick={() => execute({ kind: 'entry', data: e })}
                    onMouseEnter={() => setSelectedIndex(i)}
                  >
                    <div className={`${styles.itemIcon} ${isMemory ? styles.itemIconMemory : styles.itemIconKb}`}>
                      {isMemory ? <Brain size={16} /> : <FileText size={16} />}
                    </div>
                    <div className={styles.itemBody}>
                      <div className={styles.itemName}>{e.title}</div>
                      <div className={styles.itemSnippet}>{e.content.slice(0, 120)}</div>
                      <div className={styles.itemSub}>{e.kb_name}</div>
                    </div>
                    <div className={styles.itemScore}>{(e.score * 100).toFixed(0)}%</div>
                  </div>
                )
              })}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className={styles.footer}>
          <span className={styles.footerHint}>
            <kbd className={styles.footerKbd}><ArrowUp size={10} /></kbd>
            <kbd className={styles.footerKbd}><ArrowDown size={10} /></kbd>
            navigate
          </span>
          <span className={styles.footerHint}>
            <kbd className={styles.footerKbd}><CornerDownLeft size={10} /></kbd>
            open
          </span>
          <span className={styles.footerHint}>
            <kbd className={styles.footerKbd}>esc</kbd>
            close
          </span>
        </div>
      </div>
    </div>
  )
}
