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
  LogOut,
  Shield,
} from 'lucide-react'
import { useAuth } from '../lib/auth'
import styles from './Sidebar.module.css'

interface SidebarProps {
  collapsed: boolean
  onToggle: () => void
}

const NAV_ITEMS = [
  { path: '/dashboard', label: 'Dashboard', icon: LayoutDashboard },
  { path: '/conversations', label: 'Conversations', icon: MessageCircle },
  { path: '/knowledge', label: 'Knowledge Base', icon: BookOpen },
  { path: '/connectors', label: 'Connectors', icon: Plug },
  { path: '/tools', label: 'Tools', icon: Wrench },
]

const NAV_BOTTOM_ITEMS = [
  { path: '/settings', label: 'Settings', icon: Settings },
  { path: '/docs', label: 'Documentation', icon: FileText },
]

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  const location = useLocation()
  const { user, logout } = useAuth()

  return (
    <aside className={`${styles.sidebar} ${collapsed ? styles.collapsed : ''}`}>
      {/* Brand */}
      <div className={styles.brand}>
        <div className={styles.logo}>
          <Cat size={24} strokeWidth={2} />
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
              {isActive && <div className={styles.activeGlow} />}
              <Icon size={18} strokeWidth={isActive ? 2 : 1.5} />
              {!collapsed && <span className={styles.navLabel}>{label}</span>}
            </NavLink>
          )
        })}
      </nav>

      {/* Bottom navigation */}
      <nav className={styles.navBottom}>
        {NAV_BOTTOM_ITEMS.map(({ path, label, icon: Icon }) => {
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
              {isActive && <div className={styles.activeGlow} />}
              <Icon size={18} strokeWidth={isActive ? 2 : 1.5} />
              {!collapsed && <span className={styles.navLabel}>{label}</span>}
            </NavLink>
          )
        })}
      </nav>

      {/* User section */}
      {user && (
        <div className={styles.userSection}>
          <div className={styles.userInfo} title={collapsed ? `${user.display_name} (${user.email})` : undefined}>
            <div className={styles.avatar}>
              {user.display_name.charAt(0).toUpperCase()}
            </div>
            {!collapsed && (
              <div className={styles.userDetails}>
                <span className={styles.userName}>
                  {user.display_name}
                  {user.role === 'admin' && <Shield size={12} className={styles.adminBadge} />}
                </span>
                <span className={styles.userEmail}>{user.email}</span>
              </div>
            )}
          </div>
          <button
            className={styles.logoutBtn}
            onClick={logout}
            title="Sign out"
          >
            <LogOut size={15} />
          </button>
        </div>
      )}

      {/* Collapse toggle */}
      <button className={styles.toggle} onClick={onToggle} title="Toggle sidebar">
        {collapsed ? <PanelLeft size={16} /> : <PanelLeftClose size={16} />}
      </button>
    </aside>
  )
}
