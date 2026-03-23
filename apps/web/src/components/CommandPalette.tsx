import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { Search, MessageSquare, Bot, Library, Loader2, ArrowUp, ArrowDown, CornerDownLeft } from 'lucide-react'
import { api, type GlobalSearchResponse } from '../lib/api'
import styles from './CommandPalette.module.css'

interface CommandPaletteProps {
  open: boolean
  onClose: () => void
}

export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  const navigate = useNavigate()
  const inputRef = useRef<HTMLInputElement>(null)
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<GlobalSearchResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [selectedIndex, setSelectedIndex] = useState(0)
  const resultsRef = useRef<HTMLDivElement>(null)

  // Debounce timer
  const debounceRef = useRef<ReturnType<typeof setTimeout>>()

  // Build flat list of navigable items
  const flatItems = useMemo(() => {
    if (!results) return []
    const items: { type: 'conversation' | 'agent' | 'knowledge'; id: string; index: number }[] = []
    let idx = 0
    for (const c of results.conversations) {
      items.push({ type: 'conversation', id: c.id, index: idx++ })
    }
    for (const a of results.agents) {
      items.push({ type: 'agent', id: a.id, index: idx++ })
    }
    for (const kb of results.knowledge_bases) {
      items.push({ type: 'knowledge', id: kb.id, index: idx++ })
    }
    return items
  }, [results])

  // Reset on open/close
  useEffect(() => {
    if (open) {
      setQuery('')
      setResults(null)
      setSelectedIndex(0)
      setTimeout(() => inputRef.current?.focus(), 50)
    }
  }, [open])

  // Debounced search
  const doSearch = useCallback(async (q: string) => {
    if (q.trim().length < 2) {
      setResults(null)
      setLoading(false)
      return
    }
    setLoading(true)
    try {
      const data = await api.search.global(q, 8)
      setResults(data)
      setSelectedIndex(0)
    } catch {
      setResults(null)
    }
    setLoading(false)
  }, [])

  const handleInputChange = useCallback((value: string) => {
    setQuery(value)
    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = setTimeout(() => doSearch(value), 250)
  }, [doSearch])

  // Navigate to a result
  const navigateTo = useCallback((type: string, id: string) => {
    onClose()
    switch (type) {
      case 'conversation':
        navigate(`/conversations/${id}`)
        break
      case 'agent':
        navigate('/agents')
        break
      case 'knowledge':
        navigate('/knowledge')
        break
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
      setSelectedIndex(i => Math.min(i + 1, flatItems.length - 1))
      return
    }

    if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelectedIndex(i => Math.max(i - 1, 0))
      return
    }

    if (e.key === 'Enter' && flatItems.length > 0) {
      e.preventDefault()
      const item = flatItems[selectedIndex]
      if (item) navigateTo(item.type, item.id)
      return
    }
  }, [flatItems, selectedIndex, navigateTo, onClose])

  // Scroll selected item into view
  useEffect(() => {
    const container = resultsRef.current
    if (!container) return
    const selectedEl = container.querySelector(`[data-idx="${selectedIndex}"]`) as HTMLElement
    if (selectedEl) {
      selectedEl.scrollIntoView({ block: 'nearest' })
    }
  }, [selectedIndex])

  if (!open) return null

  const totalResults = results
    ? results.conversations.length + results.agents.length + results.knowledge_bases.length
    : 0

  let globalIdx = 0

  return (
    <div className={styles.overlay} onMouseDown={(e) => { if (e.target === e.currentTarget) onClose() }}>
      <div className={styles.palette} onKeyDown={handleKeyDown}>
        {/* Input */}
        <div className={styles.inputWrap}>
          <Search size={18} className={styles.inputIcon} />
          <input
            ref={inputRef}
            className={styles.input}
            value={query}
            onChange={e => handleInputChange(e.target.value)}
            placeholder="Search conversations, agents, knowledge..."
            autoComplete="off"
            spellCheck={false}
          />
          {loading && <Loader2 size={16} className={styles.inputSpinner} />}
          <kbd className={styles.kbd}>ESC</kbd>
        </div>

        {/* Results */}
        <div className={styles.results} ref={resultsRef}>
          {query.trim().length >= 2 && !loading && totalResults === 0 && (
            <div className={styles.empty}>No results for &ldquo;{query}&rdquo;</div>
          )}

          {query.trim().length < 2 && !loading && (
            <div className={styles.empty}>Type at least 2 characters to search</div>
          )}

          {/* Conversations */}
          {results && results.conversations.length > 0 && (
            <div className={styles.section}>
              <div className={styles.sectionLabel}>
                <MessageSquare size={12} />
                Conversations
                <span className={styles.sectionCount}>({results.conversations.length})</span>
              </div>
              {results.conversations.map(c => {
                const idx = globalIdx++
                return (
                  <div
                    key={c.id}
                    data-idx={idx}
                    className={`${styles.item} ${selectedIndex === idx ? styles.itemSelected : ''}`}
                    onClick={() => navigateTo('conversation', c.id)}
                    onMouseEnter={() => setSelectedIndex(idx)}
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
                <span className={styles.sectionCount}>({results.agents.length})</span>
              </div>
              {results.agents.map(a => {
                const idx = globalIdx++
                return (
                  <div
                    key={a.id}
                    data-idx={idx}
                    className={`${styles.item} ${selectedIndex === idx ? styles.itemSelected : ''}`}
                    onClick={() => navigateTo('agent', a.id)}
                    onMouseEnter={() => setSelectedIndex(idx)}
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
                <span className={styles.sectionCount}>({results.knowledge_bases.length})</span>
              </div>
              {results.knowledge_bases.map(kb => {
                const idx = globalIdx++
                return (
                  <div
                    key={kb.id}
                    data-idx={idx}
                    className={`${styles.item} ${selectedIndex === idx ? styles.itemSelected : ''}`}
                    onClick={() => navigateTo('knowledge', kb.id)}
                    onMouseEnter={() => setSelectedIndex(idx)}
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
        </div>

        {/* Footer hints */}
        {totalResults > 0 && (
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
        )}
      </div>
    </div>
  )
}
