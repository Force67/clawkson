import { useState, useEffect, useCallback, useRef } from 'react'
import { Outlet } from 'react-router-dom'
import { Sidebar } from './Sidebar'
import { CommandPalette } from './CommandPalette'
import styles from './Layout.module.css'

export function Layout() {
  const [collapsed, setCollapsed] = useState(false)
  const [spotlightOpen, setSpotlightOpen] = useState(false)
  const layoutRef = useRef<HTMLDivElement>(null)

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    const el = layoutRef.current
    if (!el) return
    const rect = el.getBoundingClientRect()
    const x = ((e.clientX - rect.left) / rect.width) * 100
    const y = ((e.clientY - rect.top) / rect.height) * 100
    el.style.setProperty('--mouse-x', `${x}%`)
    el.style.setProperty('--mouse-y', `${y}%`)
  }, [])

  // Global keyboard shortcuts — Cmd+K, Ctrl+K, Cmd+Shift+P, Ctrl+Shift+P
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey
      if (mod && e.key === 'k') {
        e.preventDefault()
        setSpotlightOpen(prev => !prev)
        return
      }
      if (mod && e.shiftKey && e.key === 'P') {
        e.preventDefault()
        setSpotlightOpen(prev => !prev)
        return
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [])

  // Custom event: other components can open the spotlight
  useEffect(() => {
    const handler = () => setSpotlightOpen(true)
    window.addEventListener('open-command-palette', handler)
    return () => window.removeEventListener('open-command-palette', handler)
  }, [])

  return (
    <div
      ref={layoutRef}
      className={styles.layout}
      onMouseMove={handleMouseMove}
    >
      <Sidebar collapsed={collapsed} onToggle={() => setCollapsed(c => !c)} />
      <main className={styles.main}>
        <div className={styles.content}>
          <Outlet />
        </div>
      </main>
      <CommandPalette open={spotlightOpen} onClose={() => setSpotlightOpen(false)} />
    </div>
  )
}
