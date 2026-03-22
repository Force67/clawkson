import { useState, useEffect } from 'react'
import { api, PluginManifest, PluginSidebarItem, PluginRoute, PluginSettingsPanel, PluginConnectorCard } from './api'

interface PluginState {
  manifests: PluginManifest[]
  loading: boolean
  error: string | null
}

/**
 * Hook to load and access plugin manifests.
 */
export function usePlugins() {
  const [state, setState] = useState<PluginState>({
    manifests: [],
    loading: true,
    error: null,
  })

  useEffect(() => {
    let cancelled = false
    api.plugins.list()
      .then(manifests => {
        if (!cancelled) {
          setState({ manifests, loading: false, error: null })
        }
      })
      .catch(err => {
        if (!cancelled) {
          setState({ manifests: [], loading: false, error: String(err) })
        }
      })
    return () => { cancelled = true }
  }, [])

  return state
}

/**
 * Hook to get aggregated sidebar items from all plugins.
 */
export function usePluginNav(): PluginSidebarItem[] {
  const { manifests } = usePlugins()
  return manifests
    .filter(m => m.frontend)
    .flatMap(m => m.frontend!.sidebar_items)
}

/**
 * Hook to get aggregated routes from all plugins.
 */
export function usePluginRoutes(): PluginRoute[] {
  const { manifests } = usePlugins()
  return manifests
    .filter(m => m.frontend)
    .flatMap(m => m.frontend!.routes)
}

/**
 * Hook to get aggregated settings panels from all plugins.
 */
export function usePluginSettings(): PluginSettingsPanel[] {
  const { manifests } = usePlugins()
  return manifests
    .filter(m => m.frontend)
    .flatMap(m => m.frontend!.settings_panels)
}

/**
 * Hook to get aggregated connector cards from all plugins.
 */
export function usePluginConnectorCards(): PluginConnectorCard[] {
  const { manifests } = usePlugins()
  return manifests
    .filter(m => m.frontend)
    .flatMap(m => m.frontend!.connector_cards)
}
