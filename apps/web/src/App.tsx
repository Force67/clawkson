import { Routes, Route, Navigate } from 'react-router-dom'
import { useEffect } from 'react'
import { AuthProvider, useAuth } from './lib/auth'
import { api } from './lib/api'
import { Layout } from './components/Layout'
import { LoginPage } from './pages/Login'
import { DashboardPage } from './pages/Dashboard'
import { ConversationsPage } from './pages/Conversations'
import { KnowledgeBasePage } from './pages/KnowledgeBase'
import { ConnectorsPage } from './pages/Connectors'
import { ToolsPage } from './pages/Tools'
import { SkillsPage } from './pages/Skills'
import { SettingsPage } from './pages/Settings'
import { AgentsPage } from './pages/Agents'
import { ContainersPage } from './pages/Containers'
import { DocsPage } from './pages/Docs'
import { CalendarPage } from './pages/Calendar'

function ThemeLoader() {
  useEffect(() => {
    api.settings.get().then(s => {
      document.documentElement.setAttribute('data-theme', s.theme)
    }).catch(() => {
      // fallback: honour system preference if settings can't be loaded
      document.documentElement.setAttribute('data-theme', 'system')
    })
  }, [])
  return null
}

function AppRoutes() {
  const { user, loading } = useAuth()

  if (loading) {
    return (
      <div style={{
        height: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'var(--bg-primary)',
        color: 'var(--text-secondary)',
        fontSize: '0.9rem',
      }}>
        Loading...
      </div>
    )
  }

  if (!user) {
    return (
      <Routes>
        <Route path="*" element={<LoginPage />} />
      </Routes>
    )
  }

  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<Navigate to="/dashboard" replace />} />
        <Route path="/dashboard" element={<DashboardPage />} />
        <Route path="/agents" element={<AgentsPage />} />
        <Route path="/conversations" element={<ConversationsPage />} />
        <Route path="/conversations/:id" element={<ConversationsPage />} />
        <Route path="/knowledge" element={<KnowledgeBasePage />} />
        <Route path="/connectors" element={<ConnectorsPage />} />
        <Route path="/tools" element={<ToolsPage />} />
        <Route path="/skills" element={<SkillsPage />} />
        <Route path="/containers" element={<ContainersPage />} />
        <Route path="/calendar" element={<CalendarPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/docs" element={<DocsPage />} />
      </Route>
    </Routes>
  )
}

export default function App() {
  return (
    <AuthProvider>
      <ThemeLoader />
      <AppRoutes />
    </AuthProvider>
  )
}
