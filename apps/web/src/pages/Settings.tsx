import { useState, useEffect } from 'react'
import { ChevronDown, Settings as SettingsIcon, Link } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { api, type Settings } from '../lib/api'
import styles from './Settings.module.css'

export function SettingsPage() {
  const [settings, setSettings] = useState<Settings | null>(null)
  const navigate = useNavigate()

  useEffect(() => {
    api.settings.get().then(setSettings)
  }, [])

  return (
    <div className="fade-in">
      <PageHeader
        title="Settings"
        description="Application preferences and display options."
      />

      <div className={styles.sections}>
        {/* ── Inference Connectors shortcut ── */}
        <Card>
          <div className={styles.sectionHeader}>
            <div className={styles.sectionHeaderLeft}>
              <div className={styles.sectionIconWrap}><Link size={16} /></div>
              <div>
                <h3 className={styles.sectionTitle}>Inference Connectors</h3>
                <p className={styles.sectionDesc}>
                  Manage LLM providers (Azure OpenAI, OpenRouter, OpenAI, Custom) from the Connectors page.
                </p>
              </div>
            </div>
            <button className={styles.linkBtn} onClick={() => navigate('/connectors')}>
              Go to Connectors →
            </button>
          </div>
        </Card>

        {/* ── Appearance ── */}
        <Card>
          <div className={styles.sectionHeader}>
            <div className={styles.sectionHeaderLeft}>
              <div className={styles.sectionIconWrap}><SettingsIcon size={16} /></div>
              <div>
                <h3 className={styles.sectionTitle}>Appearance</h3>
                <p className={styles.sectionDesc}>Interface theme and display preferences.</p>
              </div>
            </div>
          </div>

          <div className={styles.formGroup}>
            <label className={styles.label}>Theme</label>
            <div className={styles.selectWrap}>
              <select
                className={styles.select}
                value={settings?.theme ?? 'dark'}
                onChange={async e => {
                  const s = await api.settings.patch({ theme: e.target.value })
                  setSettings(s)
                }}
              >
                <option value="dark">Dark</option>
                <option value="light">Light</option>
                <option value="system">System</option>
              </select>
              <ChevronDown size={14} className={styles.selectChevron} />
            </div>
          </div>
        </Card>
      </div>
    </div>
  )
}
