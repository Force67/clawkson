import { NavLink, useLocation } from 'react-router-dom'
import {
  LayoutDashboard,
  MessageCircle,
  BookOpen,
  Plug,
  Wrench,
  Settings,
  FileText,
  PanelLeftClose,
  PanelLeft,
  Cat,
  Bot,
} from 'lucide-react'
import styles from './Sidebar.module.css'

interface SidebarProps {
  collapsed: boolean
  onToggle: () => void
}

const NAV_ITEMS = [
  { path: '/dashboard', label: 'Dashboard', icon: LayoutDashboard },
  { path: '/agents', label: 'Agents', icon: Bot },
  { path: '/conversations', label: 'Conversations', icon: MessageCircle },
  { path: '/knowledge', label: 'Knowledge Base', icon: BookOpen },
  { path: '/connectors', label: 'Connectors', icon: Plug },
  { path: '/tools', label: 'Tools', icon: Wrench },
  { path: '/settings', label: 'Settings', icon: Settings },
  { path: '/docs', label: 'Documentation', icon: FileText },
]

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  const location = useLocation()

  return (
    <aside className={`${styles.sidebar} ${collapsed ? styles.collapsed : ''}`}>
      {/* Brand */}
      <div className={styles.brand}>
        <div className={styles.logo}>
          <Cat size={28} strokeWidth={1.8} />
        </div>
        {!collapsed && <span className={styles.brandText}>Clawkson</span>}
      </div>

      {/* Navigation */}
      <nav className={styles.nav}>
        {NAV_ITEMS.map(({ path, label, icon: Icon }) => {
          const isActive =
            location.pathname === path ||
            location.pathname.startsWith(path + '/')

          return (
            <NavLink
              key={path}
              to={path}
              className={`${styles.navItem} ${isActive ? styles.navItemActive : ''}`}
              title={collapsed ? label : undefined}
            >
              <Icon size={20} strokeWidth={isActive ? 2 : 1.5} />
              {!collapsed && <span className={styles.navLabel}>{label}</span>}
              {isActive && <div className={styles.activeIndicator} />}
            </NavLink>
          )
        })}
      </nav>

      {/* Collapse toggle */}
      <button className={styles.toggle} onClick={onToggle} title="Toggle sidebar">
        {collapsed ? <PanelLeft size={18} /> : <PanelLeftClose size={18} />}
      </button>
    </aside>
  )
}
