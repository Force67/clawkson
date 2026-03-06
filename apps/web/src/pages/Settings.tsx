import { Save, Moon, Globe, Key } from 'lucide-react'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import styles from './Settings.module.css'

export function SettingsPage() {
  return (
    <div className="fade-in">
      <PageHeader
        title="Settings"
        description="Configure agents, connectors, and application preferences."
      />

      <div className={styles.sections}>
        {/* LLM Configuration */}
        <Card>
          <div className={styles.sectionHeader}>
            <Key size={18} className={styles.sectionIcon} />
            <div>
              <h3 className={styles.sectionTitle}>LLM Configuration</h3>
              <p className={styles.sectionDesc}>Bring your own LLM connectors for inference.</p>
            </div>
          </div>

          <div className={styles.formGroup}>
            <label className={styles.label}>Provider</label>
            <select className={styles.select}>
              <option>OpenAI</option>
              <option>Anthropic</option>
              <option>Local (Ollama)</option>
              <option>Custom</option>
            </select>
          </div>

          <div className={styles.formGroup}>
            <label className={styles.label}>API Base URL</label>
            <input className={styles.input} type="text" placeholder="https://api.openai.com/v1" />
          </div>

          <div className={styles.formGroup}>
            <label className={styles.label}>API Key</label>
            <input className={styles.input} type="password" placeholder="sk-..." />
          </div>

          <div className={styles.formGroup}>
            <label className={styles.label}>Model</label>
            <input className={styles.input} type="text" placeholder="gpt-4o" />
          </div>
        </Card>

        {/* Appearance */}
        <Card>
          <div className={styles.sectionHeader}>
            <Moon size={18} className={styles.sectionIcon} />
            <div>
              <h3 className={styles.sectionTitle}>Appearance</h3>
              <p className={styles.sectionDesc}>Customize the look and feel.</p>
            </div>
          </div>

          <div className={styles.formGroup}>
            <label className={styles.label}>Theme</label>
            <select className={styles.select}>
              <option>Dark</option>
              <option>Light</option>
              <option>System</option>
            </select>
          </div>
        </Card>

        {/* General */}
        <Card>
          <div className={styles.sectionHeader}>
            <Globe size={18} className={styles.sectionIcon} />
            <div>
              <h3 className={styles.sectionTitle}>General</h3>
              <p className={styles.sectionDesc}>Backend and runtime configuration.</p>
            </div>
          </div>

          <div className={styles.formGroup}>
            <label className={styles.label}>API Endpoint</label>
            <input className={styles.input} type="text" defaultValue="http://localhost:47821" />
          </div>

          <div className={styles.formGroup}>
            <label className={styles.label}>Denkwerk Orchestrator URL</label>
            <input className={styles.input} type="text" placeholder="http://localhost:8080" />
          </div>
        </Card>

        <div className={styles.actions}>
          <Button>
            <Save size={16} /> Save Changes
          </Button>
        </div>
      </div>
    </div>
  )
}
