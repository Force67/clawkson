import { useState } from 'react'
import { Outlet } from 'react-router-dom'
import { Sidebar } from './Sidebar'
import styles from './Layout.module.css'

export function Layout() {
  const [collapsed, setCollapsed] = useState(false)

  return (
    <div className={styles.layout}>
      <Sidebar collapsed={collapsed} onToggle={() => setCollapsed(c => !c)} />
      <main className={`${styles.main} ${collapsed ? styles.mainCollapsed : ''}`}>
        <div className={styles.content}>
          <Outlet />
        </div>
      </main>
    </div>
  )
}
