import { NavLink, useLocation, useNavigate } from 'react-router-dom'
import {
  Gauge,
  MessagesSquare,
  Library,
  Cable,
  Cog,
  Sparkles,
  Bot,
  Container,
  CalendarDays,
  Timer,
  SlidersHorizontal,
  ScrollText,
  ChevronsLeft,
  ChevronsRight,
  Disc3,
  LogOut,
  ShieldCheck,
  KeyRound,
  Activity,
  Webhook,
} from 'lucide-react'
import { useAuth } from '../lib/auth'
import styles from './Sidebar.module.css'

interface SidebarProps {
  collapsed: boolean
  onToggle: () => void
}

interface NavItem {
  path: string
  label: string
  icon: typeof Gauge
}

interface NavGroup {
  label: string
  items: NavItem[]
}

const NAV_GROUPS: NavGroup[] = [
  {
    label: 'Overview',
    items: [
      { path: '/dashboard', label: 'Dashboard', icon: Gauge },
      { path: '/agents', label: 'Agents', icon: Bot },
      { path: '/conversations', label: 'Conversations', icon: MessagesSquare },
      { path: '/calendar', label: 'Calendar', icon: CalendarDays },
      { path: '/scheduled-tasks', label: 'Scheduled Tasks', icon: Timer },
      { path: '/webhooks', label: 'Webhooks', icon: Webhook },
    ],
  },
  {
    label: 'Resources',
    items: [
      { path: '/knowledge', label: 'Knowledge Base', icon: Library },
      { path: '/skills', label: 'Skills', icon: Sparkles },
      { path: '/tools', label: 'Tools', icon: Cog },
      { path: '/connectors', label: 'Connectors', icon: Cable },
      { path: '/credentials', label: 'Credentials', icon: KeyRound },
    ],
  },
  {
    label: 'Infrastructure',
    items: [
      { path: '/containers', label: 'Containers', icon: Container },
      { path: '/activity-log', label: 'Activity Log', icon: Activity },
    ],
  },
]

const NAV_BOTTOM_ITEMS = [
  { path: '/settings', label: 'Settings', icon: SlidersHorizontal },
  { path: '/docs', label: 'Documentation', icon: ScrollText },
]

export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  const location = useLocation()
  const navigate = useNavigate()
  const { user, logout } = useAuth()

  return (
    <aside className={`${styles.sidebar} ${collapsed ? styles.collapsed : ''}`}>
      {/* Brand */}
      <div className={styles.brand}>
        <div className={styles.logo}>
          <Disc3 size={17} strokeWidth={1.5} />
        </div>
        {!collapsed && <span className={styles.brandText}>clawkson</span>}
      </div>

      {/* Navigation */}
      <nav className={styles.nav}>
        {NAV_GROUPS.map((group) => (
          <div key={group.label} className={styles.navGroup}>
            {!collapsed && (
              <span className={styles.navGroupLabel}>{group.label}</span>
            )}
            {collapsed && <div className={styles.navGroupDivider} />}
            {group.items.map(({ path, label, icon: Icon }) => {
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
          </div>
        ))}
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
          <button
            className={`${styles.userInfo} ${styles.userInfoBtn}`}
            onClick={() => navigate('/profile')}
            title={collapsed ? `${user.display_name} — Edit profile` : 'Edit profile'}
          >
            <div className={styles.avatar}>
              {user.avatar_url
                ? <img src={user.avatar_url} alt={user.display_name} className={styles.avatarImg} />
                : user.display_name.charAt(0).toUpperCase()}
            </div>
            {!collapsed && (
              <div className={styles.userDetails}>
                <span className={styles.userName}>
                  {user.display_name}
                  {user.role === 'admin' && <ShieldCheck size={12} className={styles.adminBadge} />}
                </span>
                <span className={styles.userEmail}>{user.email}</span>
              </div>
            )}
          </button>
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
        {collapsed ? <ChevronsRight size={16} /> : <ChevronsLeft size={16} />}
      </button>
    </aside>
  )
}
