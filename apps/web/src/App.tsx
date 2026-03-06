import { Routes, Route, Navigate } from 'react-router-dom'
import { Layout } from './components/Layout'
import { DashboardPage } from './pages/Dashboard'
import { ConversationsPage } from './pages/Conversations'
import { KnowledgeBasePage } from './pages/KnowledgeBase'
import { ConnectorsPage } from './pages/Connectors'
import { ToolsPage } from './pages/Tools'
import { SettingsPage } from './pages/Settings'
import { DocsPage } from './pages/Docs'
import { AgentsPage } from './pages/Agents'

export default function App() {
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
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/docs" element={<DocsPage />} />
      </Route>
    </Routes>
  )
}
