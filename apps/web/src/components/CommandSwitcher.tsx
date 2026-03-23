import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { useNavigate, useLocation } from 'react-router-dom'
import {
  Gauge, Bot, MessagesSquare, CalendarDays, Timer, Webhook,
  Library, Sparkles, Cog, Cable, KeyRound,
  Container, Activity,
  SlidersHorizontal, ScrollText,
  Search, Plus, ArrowUp, ArrowDown, CornerDownLeft,
  type LucideIcon,
} from 'lucide-react'
import styles from './CommandSwitcher.module.css'

interface CommandSwitcherProps {
  open: boolean
  onClose: () => void
}

interface CommandItem {
  id: string
  label: string
  group: string
  icon: LucideIcon
  action: 'navigate' | 'event'
  target: string // path for navigate, event name for event
  keywords?: string // extra searchable text
}

const COMMANDS: CommandItem[] = [
  // Navigation — Overview
  { id: 'nav-dashboard',     label: 'Go to Dashboard',        group: 'Navigation', icon: Gauge,             action: 'navigate', target: '/dashboard' },
  { id: 'nav-agents',        label: 'Go to Agents',           group: 'Navigation', icon: Bot,               action: 'navigate', target: '/agents' },
  { id: 'nav-conversations', label: 'Go to Conversations',    group: 'Navigation', icon: MessagesSquare,    action: 'navigate', target: '/conversations' },
  { id: 'nav-calendar',      label: 'Go to Calendar',         group: 'Navigation', icon: CalendarDays,      action: 'navigate', target: '/calendar' },
  { id: 'nav-scheduled',     label: 'Go to Scheduled Tasks',  group: 'Navigation', icon: Timer,             action: 'navigate', target: '/scheduled-tasks' },
  { id: 'nav-webhooks',      label: 'Go to Webhooks',         group: 'Navigation', icon: Webhook,           action: 'navigate', target: '/webhooks' },

  // Navigation — Resources
  { id: 'nav-knowledge',     label: 'Go to Knowledge Base',   group: 'Navigation', icon: Library,           action: 'navigate', target: '/knowledge' },
  { id: 'nav-skills',        label: 'Go to Skills',           group: 'Navigation', icon: Sparkles,          action: 'navigate', target: '/skills' },
  { id: 'nav-tools',         label: 'Go to Tools',            group: 'Navigation', icon: Cog,               action: 'navigate', target: '/tools' },
  { id: 'nav-connectors',    label: 'Go to Connectors',       group: 'Navigation', icon: Cable,             action: 'navigate', target: '/connectors' },
  { id: 'nav-credentials',   label: 'Go to Credentials',      group: 'Navigation', icon: KeyRound,          action: 'navigate', target: '/credentials' },

  // Navigation — Infrastructure
  { id: 'nav-containers',    label: 'Go to Containers',       group: 'Navigation', icon: Container,         action: 'navigate', target: '/containers' },
  { id: 'nav-activity',      label: 'Go to Activity Log',     group: 'Navigation', icon: Activity,          action: 'navigate', target: '/activity-log' },

  // Navigation — System
  { id: 'nav-settings',      label: 'Go to Settings',         group: 'Navigation', icon: SlidersHorizontal, action: 'navigate', target: '/settings', keywords: 'preferences config theme llm' },
  { id: 'nav-docs',          label: 'Go to Documentation',    group: 'Navigation', icon: ScrollText,        action: 'navigate', target: '/docs' },

  // Actions
  { id: 'act-search',        label: 'Search...',              group: 'Actions',    icon: Search,            action: 'event',    target: 'open-command-palette', keywords: 'find query' },
  { id: 'act-new-agent',     label: 'Create New Agent',       group: 'Actions',    icon: Plus,              action: 'navigate', target: '/agents?new=1', keywords: 'add agent bot' },
  { id: 'act-new-conv',      label: 'New Conversation',       group: 'Actions',    icon: Plus,              action: 'navigate', target: '/conversations?new=1', keywords: 'add chat message' },
  { id: 'act-new-kb',        label: 'Create Knowledge Base',  group: 'Actions',    icon: Plus,              action: 'navigate', target: '/knowledge?new=1', keywords: 'add knowledge documents' },
]

export function CommandSwitcher({ open, onClose }: CommandSwitcherProps) {
  const navigate = useNavigate()
  const location = useLocation()
  const inputRef = useRef<HTMLInputElement>(null)
  const [query, setQuery] = useState('')
  const [selectedIndex, setSelectedIndex] = useState(0)
  const resultsRef = useRef<HTMLDivElement>(null)

  // Filter commands by query
  const filtered = useMemo(() => {
    if (!query.trim()) return COMMANDS
    const q = query.toLowerCase()
    return COMMANDS.filter(cmd =>
      cmd.label.toLowerCase().includes(q) ||
      cmd.group.toLowerCase().includes(q) ||
      (cmd.keywords && cmd.keywords.toLowerCase().includes(q))
    )
  }, [query])

  // Group filtered items
  const grouped = useMemo(() => {
    const groups: { label: string; items: CommandItem[] }[] = []
    const map = new Map<string, CommandItem[]>()
    for (const cmd of filtered) {
      let arr = map.get(cmd.group)
      if (!arr) { arr = []; map.set(cmd.group, arr) }
      arr.push(cmd)
    }
    for (const [label, items] of map) {
      groups.push({ label, items })
    }
    return groups
  }, [filtered])

  // Reset on open
  useEffect(() => {
    if (open) {
      setQuery('')
      setSelectedIndex(0)
      setTimeout(() => inputRef.current?.focus(), 50)
    }
  }, [open])

  // Execute a command
  const execute = useCallback((cmd: CommandItem) => {
    onClose()
    if (cmd.action === 'navigate') {
      navigate(cmd.target)
    } else if (cmd.action === 'event') {
      window.dispatchEvent(new CustomEvent(cmd.target))
    }
  }, [navigate, onClose])

  // Keyboard navigation
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
      return
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setSelectedIndex(i => Math.min(i + 1, filtered.length - 1))
      return
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelectedIndex(i => Math.max(i - 1, 0))
      return
    }
    if (e.key === 'Enter' && filtered.length > 0) {
      e.preventDefault()
      const cmd = filtered[selectedIndex]
      if (cmd) execute(cmd)
      return
    }
  }, [filtered, selectedIndex, execute, onClose])

  // Clamp selection when filter changes
  useEffect(() => {
    setSelectedIndex(0)
  }, [query])

  // Scroll selected into view
  useEffect(() => {
    const container = resultsRef.current
    if (!container) return
    const el = container.querySelector(`[data-idx="${selectedIndex}"]`) as HTMLElement
    if (el) el.scrollIntoView({ block: 'nearest' })
  }, [selectedIndex])

  if (!open) return null

  // Track global index for flat selection
  let globalIdx = 0

  return (
    <div className={styles.overlay} onMouseDown={e => { if (e.target === e.currentTarget) onClose() }}>
      <div className={styles.palette} onKeyDown={handleKeyDown}>
        <div className={styles.inputWrap}>
          <span className={styles.inputPrefix}>&gt;</span>
          <input
            ref={inputRef}
            className={styles.input}
            value={query}
            onChange={e => setQuery(e.target.value)}
            placeholder="Type a command..."
            autoComplete="off"
            spellCheck={false}
          />
          <kbd className={styles.kbd}>ESC</kbd>
        </div>

        <div className={styles.results} ref={resultsRef}>
          {filtered.length === 0 && (
            <div className={styles.empty}>No matching commands</div>
          )}

          {grouped.map(group => (
            <div key={group.label} className={styles.section}>
              <div className={styles.sectionLabel}>{group.label}</div>
              {group.items.map(cmd => {
                const idx = globalIdx++
                const Icon = cmd.icon
                const isCurrentPage = cmd.action === 'navigate' &&
                  (location.pathname === cmd.target || location.pathname.startsWith(cmd.target.split('?')[0] + '/'))

                return (
                  <div
                    key={cmd.id}
                    data-idx={idx}
                    className={`${styles.item} ${selectedIndex === idx ? styles.itemSelected : ''}`}
                    onClick={() => execute(cmd)}
                    onMouseEnter={() => setSelectedIndex(idx)}
                  >
                    <Icon size={16} className={styles.itemIcon} />
                    <span className={styles.itemLabel}>{cmd.label}</span>
                    {isCurrentPage && <span className={styles.currentBadge}>current</span>}
                  </div>
                )
              })}
            </div>
          ))}
        </div>

        <div className={styles.footer}>
          <span className={styles.footerHint}>
            <kbd className={styles.footerKbd}><ArrowUp size={10} /></kbd>
            <kbd className={styles.footerKbd}><ArrowDown size={10} /></kbd>
            navigate
          </span>
          <span className={styles.footerHint}>
            <kbd className={styles.footerKbd}><CornerDownLeft size={10} /></kbd>
            run
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
