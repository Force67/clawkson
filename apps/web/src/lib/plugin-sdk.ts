/**
 * Clawkson Plugin SDK
 *
 * Shared API for plugin UIs. Plugins import from this module to access
 * the Clawkson API, theme variables, and shared UI components.
 */

// Re-export the API client and types
export { api } from './api'
export type {
  PluginManifest,
  PluginSidebarItem,
  PluginRoute,
  PluginSettingsPanel,
  PluginConnectorCard,
  PluginFrontendManifest,
  Agent,
  Connector,
  LlmConnector,
  Conversation,
  Message,
  KnowledgeBase,
} from './api'

// Re-export plugin hooks
export { usePlugins, usePluginNav, usePluginRoutes, usePluginSettings, usePluginConnectorCards } from './usePlugins'

/**
 * Hook to access current theme CSS custom properties.
 * Plugin components should use CSS variables from the design system rather than hardcoding colors.
 */
export function useTheme() {
  return {
    // Access CSS variables via getComputedStyle
    getVar: (name: string) => {
      return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
    },
  }
}
