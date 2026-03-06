import styles from './Card.module.css'

interface CardProps {
  children: React.ReactNode
  interactive?: boolean
  onClick?: () => void
  className?: string
  style?: React.CSSProperties
}

export function Card({ children, interactive, onClick, className, style }: CardProps) {
  return (
    <div
      className={`${styles.card} ${interactive ? styles.cardInteractive : ''} ${className ?? ''}`}
      onClick={onClick}
      style={style}
    >
      {children}
    </div>
  )
}
