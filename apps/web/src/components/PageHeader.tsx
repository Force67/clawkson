import styles from './PageHeader.module.css'

interface PageHeaderProps {
  title: string
  description?: string
  actions?: React.ReactNode
}

export function PageHeader({ title, description, actions }: PageHeaderProps) {
  return (
    <div className={styles.pageHeader}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <h1 className={styles.pageTitle}>{title}</h1>
          {description && <p className={styles.pageDescription}>{description}</p>}
        </div>
        {actions && <div>{actions}</div>}
      </div>
    </div>
  )
}
