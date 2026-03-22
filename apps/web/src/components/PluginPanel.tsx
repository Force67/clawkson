import { useEffect, useState } from 'react'

interface PluginPanelProps {
  bundleUrl: string | null
  componentName: string
  pluginName: string
}

/**
 * Renders a plugin-provided settings panel component.
 * Used in the Settings page to embed plugin configuration panels.
 */
export function PluginPanel({ bundleUrl, componentName, pluginName }: PluginPanelProps) {
  const [Component, setComponent] = useState<React.ComponentType | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!bundleUrl) {
      setError(`No bundle available for ${pluginName}`)
      return
    }

    import(/* @vite-ignore */ bundleUrl)
      .then((mod) => {
        const Comp = mod[componentName] || mod.default
        if (!Comp) {
          setError(`Component "${componentName}" not found`)
          return
        }
        setComponent(() => Comp)
      })
      .catch((err) => {
        setError(`Failed to load: ${err.message}`)
      })
  }, [bundleUrl, componentName, pluginName])

  if (error) {
    return <p style={{ color: 'var(--text-secondary)', fontSize: '0.85rem' }}>{error}</p>
  }

  if (!Component) {
    return <p style={{ color: 'var(--text-secondary)', fontSize: '0.85rem' }}>Loading...</p>
  }

  return <Component />
}
