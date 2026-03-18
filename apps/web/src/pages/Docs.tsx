import { useState } from 'react'
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import rehypeHighlight from 'rehype-highlight'
import {
  BookOpen,
  Rocket,
  Layers,
  Bot,
  Code2,
  Plug,
  Shield,
  Database,
} from 'lucide-react'
import styles from './Docs.module.css'

import readmeRaw from '../../../../docs/README.md?raw'
import gettingStartedRaw from '../../../../docs/getting-started.md?raw'
import architectureRaw from '../../../../docs/architecture.md?raw'
import agentsRaw from '../../../../docs/agents.md?raw'
import apiRaw from '../../../../docs/api.md?raw'
import connectorsRaw from '../../../../docs/connectors.md?raw'
import permissionsRaw from '../../../../docs/permissions.md?raw'
import vectorchordRaw from '../../../../docs/vectorchord.md?raw'

interface Chapter {
  id: string
  title: string
  icon: React.ReactNode
  content: string
}

const chapters: Chapter[] = [
  { id: 'welcome', title: 'Welcome', icon: <BookOpen size={16} />, content: readmeRaw },
  { id: 'getting-started', title: 'Getting Started', icon: <Rocket size={16} />, content: gettingStartedRaw },
  { id: 'architecture', title: 'Architecture', icon: <Layers size={16} />, content: architectureRaw },
  { id: 'agents', title: 'Agents', icon: <Bot size={16} />, content: agentsRaw },
  { id: 'api', title: 'API Reference', icon: <Code2 size={16} />, content: apiRaw },
  { id: 'connectors', title: 'Connectors', icon: <Plug size={16} />, content: connectorsRaw },
  { id: 'permissions', title: 'Permissions', icon: <Shield size={16} />, content: permissionsRaw },
  { id: 'vectorchord', title: 'VectorChord', icon: <Database size={16} />, content: vectorchordRaw },
]

export function DocsPage() {
  const [activeId, setActiveId] = useState(chapters[0].id)
  const active = chapters.find((c) => c.id === activeId) ?? chapters[0]

  return (
    <div className={`${styles.docsLayout} fade-in`}>
      {/* Chapter sidebar */}
      <nav className={styles.tocSidebar}>
        <div className={styles.tocHeader}>
          <span className={styles.tocLabel}>Documentation</span>
        </div>
        <ul className={styles.tocList}>
          {chapters.map((ch, i) => (
            <li key={ch.id}>
              <button
                className={`${styles.tocItem} ${ch.id === activeId ? styles.tocItemActive : ''}`}
                onClick={() => setActiveId(ch.id)}
              >
                <span className={styles.tocIcon}>{ch.icon}</span>
                <span className={styles.tocChapterNum}>{String(i + 1).padStart(2, '0')}</span>
                <span className={styles.tocTitle}>{ch.title}</span>
              </button>
            </li>
          ))}
        </ul>
      </nav>

      {/* Content area */}
      <div className={styles.contentArea}>
        <article className={styles.article} key={activeId}>
          <Markdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeHighlight]}>
            {active.content}
          </Markdown>
        </article>
      </div>
    </div>
  )
}
