import styles from './StatusBadge.module.css'

type Status = 'online' | 'offline' | 'busy' | 'error'

interface StatusBadgeProps {
  status: Status
}

export function StatusBadge({ status }: StatusBadgeProps) {
  return (
    <span className={`${styles.badge} ${styles[status]}`}>
      <span
        style={{
          width: 6,
          height: 6,
          borderRadius: '50%',
          background: 'currentColor',
        }}
      />
      {status}
    </span>
  )
}
