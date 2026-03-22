import { useEffect, useState } from 'react'

interface PluginPageProps {
  bundleUrl: string | null
  componentName: string
  pluginName: string
}

/**
 * Renders a plugin-provided page component by dynamically importing its bundle.
 * If no bundle URL is provided, shows a placeholder.
 */
export function PluginPage({ bundleUrl, componentName, pluginName }: PluginPageProps) {
  const [Component, setComponent] = useState<React.ComponentType | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!bundleUrl) {
      setError(`Plugin "${pluginName}" does not provide a frontend bundle.`)
      return
    }

    // Dynamic import of the plugin bundle
    import(/* @vite-ignore */ bundleUrl)
      .then((mod) => {
        const Comp = mod[componentName] || mod.default
        if (!Comp) {
          setError(`Component "${componentName}" not found in plugin bundle.`)
          return
        }
        setComponent(() => Comp)
      })
      .catch((err) => {
        setError(`Failed to load plugin component: ${err.message}`)
      })
  }, [bundleUrl, componentName, pluginName])

  if (error) {
    return (
      <div style={{ padding: '2rem', color: 'var(--text-secondary)' }}>
        <h2 style={{ color: 'var(--text-primary)', marginBottom: '0.5rem' }}>
          Plugin: {pluginName}
        </h2>
        <p>{error}</p>
      </div>
    )
  }

  if (!Component) {
    return (
      <div style={{ padding: '2rem', color: 'var(--text-secondary)' }}>
        Loading plugin...
      </div>
    )
  }

  return <Component />
}
