import { Plus, BookOpen, Tag, Search } from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import styles from './KnowledgeBase.module.css'

const MOCK_ENTRIES = [
  {
    id: '1',
    title: 'Company Meeting Notes Template',
    content: 'Standard template for all-hands meeting notes including agenda, action items, and follow-ups.',
    tags: ['template', 'meetings'],
    updatedAt: '2 days ago',
  },
  {
    id: '2',
    title: 'API Integration Guidelines',
    content: 'Best practices for integrating third-party APIs including rate limiting, error handling, and retry strategies.',
    tags: ['api', 'engineering', 'guidelines'],
    updatedAt: '1 week ago',
  },
  {
    id: '3',
    title: 'Product Roadmap Q1 2026',
    content: 'High-level product initiatives and milestones for Q1 2026.',
    tags: ['product', 'roadmap'],
    updatedAt: '3 days ago',
  },
]

export function KnowledgeBasePage() {
  return (
    <div className="fade-in">
      <PageHeader
        title="Knowledge Base"
        description="Manage the knowledge that your agents can access and reference."
        actions={
          <Button>
            <Plus size={16} /> Add Entry
          </Button>
        }
      />

      <div className={styles.searchBar}>
        <Search size={16} />
        <input type="text" placeholder="Search knowledge base..." className={styles.searchInput} />
      </div>

      <div className={styles.grid}>
        {MOCK_ENTRIES.map(entry => (
          <Card key={entry.id} interactive>
            <div className={styles.entryHeader}>
              <BookOpen size={18} className={styles.entryIcon} />
              <h3 className={styles.entryTitle}>{entry.title}</h3>
            </div>
            <p className={styles.entryContent}>{entry.content}</p>
            <div className={styles.entryFooter}>
              <div className={styles.tags}>
                {entry.tags.map(tag => (
                  <span key={tag} className={styles.tag}>
                    <Tag size={10} />
                    {tag}
                  </span>
                ))}
              </div>
              <span className={styles.entryTime}>{entry.updatedAt}</span>
            </div>
          </Card>
        ))}
      </div>
    </div>
  )
}
