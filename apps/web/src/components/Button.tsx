import styles from './Button.module.css'

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost'
  size?: 'default' | 'sm'
  children: React.ReactNode
}

export function Button({ variant = 'primary', size = 'default', className, children, ...props }: ButtonProps) {
  return (
    <button
      className={`${styles.button} ${styles[variant]} ${size === 'sm' ? styles.sm : ''} ${className ?? ''}`}
      {...props}
    >
      {children}
    </button>
  )
}
