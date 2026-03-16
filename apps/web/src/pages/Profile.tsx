import { useState, useRef, useCallback } from 'react'
import {
  Camera, Check, Loader2, User as UserIcon, Shield, AtSign,
  AlignLeft, Save, X,
} from 'lucide-react'
import { useAuth } from '../lib/auth'
import { api } from '../lib/api'
import { PageHeader } from '../components/PageHeader'
import { Button } from '../components/Button'
import styles from './Profile.module.css'

export function ProfilePage() {
  const { user, setUser } = useAuth()

  const [displayName, setDisplayName] = useState(user?.display_name ?? '')
  const [bio, setBio] = useState(user?.bio ?? '')
  const [avatarUrl, setAvatarUrl] = useState(user?.avatar_url ?? '')

  const [saving, setSaving] = useState(false)
  const [saved, setSaved] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const fileInputRef = useRef<HTMLInputElement>(null)

  const handleAvatarChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0]
    if (!file) return
    if (!file.type.startsWith('image/')) {
      setError('Please select an image file.')
      return
    }
    if (file.size > 2 * 1024 * 1024) {
      setError('Image must be smaller than 2 MB.')
      return
    }
    const reader = new FileReader()
    reader.onload = (ev) => {
      setAvatarUrl(ev.target?.result as string)
      setError(null)
    }
    reader.readAsDataURL(file)
  }, [])

  async function handleSave(e: React.FormEvent) {
    e.preventDefault()
    setError(null)
    if (!displayName.trim()) {
      setError('Display name cannot be empty.')
      return
    }
    setSaving(true)
    try {
      const res = await api.auth.patchProfile({
        display_name: displayName.trim(),
        bio: bio.trim(),
        avatar_url: avatarUrl,
      })
      setUser(res.user)
      setSaved(true)
      setTimeout(() => setSaved(false), 2500)
    } catch (err) {
      setError(String(err))
    } finally {
      setSaving(false)
    }
  }

  function handleClearAvatar() {
    setAvatarUrl('')
    if (fileInputRef.current) fileInputRef.current.value = ''
  }

  const initials = (displayName || user?.display_name || '?').charAt(0).toUpperCase()

  return (
    <div className="fade-in">
      <PageHeader
        title="Profile"
        description="Your identity and personal context shared with agents."
      />

      <div className={styles.layout}>
        {/* Avatar card */}
        <div className={styles.avatarCard}>
          <div className={styles.avatarWrap}>
            {avatarUrl ? (
              <img src={avatarUrl} alt="Avatar" className={styles.avatarImg} />
            ) : (
              <div className={styles.avatarInitials}>{initials}</div>
            )}
            <button
              type="button"
              className={styles.avatarOverlay}
              onClick={() => fileInputRef.current?.click()}
              title="Change avatar"
            >
              <Camera size={18} />
            </button>
            {avatarUrl && (
              <button
                type="button"
                className={styles.avatarClear}
                onClick={handleClearAvatar}
                title="Remove avatar"
              >
                <X size={12} />
              </button>
            )}
          </div>
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            onChange={handleAvatarChange}
            className={styles.hiddenInput}
          />
          <p className={styles.avatarHint}>JPEG / PNG / WebP · max 2 MB</p>
          {user?.role === 'admin' && (
            <div className={styles.roleBadge}>
              <Shield size={12} />
              Admin
            </div>
          )}
        </div>

        {/* Form card */}
        <form className={styles.formCard} onSubmit={handleSave}>
          <div className={styles.fieldGroup}>
            <label className={styles.fieldLabel}>
              <span className={styles.fieldLabelText}>
                <UserIcon size={13} />
                Display Name
              </span>
              <input
                className={styles.input}
                value={displayName}
                onChange={e => setDisplayName(e.target.value)}
                placeholder="Your name"
                maxLength={80}
              />
            </label>

            <label className={styles.fieldLabel}>
              <span className={styles.fieldLabelText}>
                <AtSign size={13} />
                Email
              </span>
              <input
                className={`${styles.input} ${styles.inputReadonly}`}
                value={user?.email ?? ''}
                readOnly
                tabIndex={-1}
              />
              <span className={styles.hint}>Email cannot be changed.</span>
            </label>

            <label className={styles.fieldLabel}>
              <span className={styles.fieldLabelText}>
                <AlignLeft size={13} />
                About you
              </span>
              <textarea
                className={styles.textarea}
                value={bio}
                onChange={e => setBio(e.target.value)}
                placeholder={`Tell your agents who you are. For example:\n"I'm a software engineer working at Acme Corp. My preferred language is Python. Timezone: UTC+1."`}
                rows={6}
                maxLength={2000}
              />
              <span className={styles.hint}>
                This context is injected into agent prompts so they can tailor responses to you.
                {bio.length > 0 && <> &nbsp;·&nbsp; {bio.length} / 2000</>}
              </span>
            </label>
          </div>

          {error && (
            <div className={styles.errorBanner}>
              <X size={13} />
              {error}
            </div>
          )}

          <div className={styles.formActions}>
            {saved && (
              <span className={styles.savedBadge}>
                <Check size={13} /> Saved
              </span>
            )}
            <Button type="submit" disabled={saving}>
              {saving
                ? <><Loader2 size={14} className="spinning" /> Saving…</>
                : <><Save size={14} /> Save changes</>}
            </Button>
          </div>
        </form>
      </div>
    </div>
  )
}
