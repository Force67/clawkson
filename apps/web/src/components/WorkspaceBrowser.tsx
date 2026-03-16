import { useState, useEffect, useCallback, useRef } from 'react'
import {
  FolderOpen, File, Upload, Download, Trash2,
  RefreshCw, Loader2, ChevronRight, ArrowLeft, AlertCircle,
  Wifi,
} from 'lucide-react'
import { api, type WorkspaceEntry, type WorkspaceListing } from '../lib/api'
import styles from './WorkspaceBrowser.module.css'

interface WorkspaceBrowserProps {
  agentId: string
  conversationId: string
  /** If true, show a live-watch indicator and subscribe to the watch SSE. */
  liveWatch?: boolean
}

function formatBytes(b: number): string {
  if (b < 1024) return `${b} B`
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`
  return `${(b / (1024 * 1024)).toFixed(1)} MB`
}

function formatDate(iso: string | null): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return d.toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' })
}

export function WorkspaceBrowser({ agentId, conversationId, liveWatch = false }: WorkspaceBrowserProps) {
  const [listing, setListing] = useState<WorkspaceListing | null>(null)
  const [currentPath, setCurrentPath] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const [uploading, setUploading] = useState(false)
  const [uploadError, setUploadError] = useState('')
  const [deletingPath, setDeletingPath] = useState<string | null>(null)
  const [liveActive, setLiveActive] = useState(false)
  const [recentChange, setRecentChange] = useState<string | null>(null)
  const fileInputRef = useRef<HTMLInputElement>(null)
  const eventSourceRef = useRef<EventSource | null>(null)

  const load = useCallback(async (path: string) => {
    setLoading(true)
    setError('')
    try {
      const result = await api.containers.workspace.list(agentId, conversationId, path)
      setListing(result)
      setCurrentPath(path)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [agentId, conversationId])

  useEffect(() => {
    load('')
  }, [load])

  // Live watch via SSE
  useEffect(() => {
    if (!liveWatch) return

    const url = api.containers.workspace.watchUrl(agentId, conversationId)
    const es = new EventSource(url, { withCredentials: true })
    eventSourceRef.current = es

    es.onopen = () => setLiveActive(true)
    es.onerror = () => setLiveActive(false)
    es.onmessage = (e) => {
      try {
        const event = JSON.parse(e.data)
        const label = `${event.type}: ${event.path}`
        setRecentChange(label)
        // Reload the current directory listing so the UI reflects changes.
        load(currentPath)
        // Clear the badge after 4s.
        setTimeout(() => setRecentChange(prev => prev === label ? null : prev), 4000)
      } catch {}
    }

    return () => {
      es.close()
      eventSourceRef.current = null
      setLiveActive(false)
    }
  }, [agentId, liveWatch, currentPath, load])

  // ── Navigation ────────────────────────────────────────────────

  const navigateInto = (entry: WorkspaceEntry) => {
    if (entry.is_dir) load(entry.path)
  }

  const navigateUp = () => {
    const parts = currentPath.split('/').filter(Boolean)
    parts.pop()
    load(parts.join('/'))
  }

  const breadcrumbs = (() => {
    const parts = currentPath.split('/').filter(Boolean)
    const crumbs: { label: string; path: string }[] = [{ label: 'workspace', path: '' }]
    parts.forEach((p, i) => {
      crumbs.push({ label: p, path: parts.slice(0, i + 1).join('/') })
    })
    return crumbs
  })()

  // ── Upload ────────────────────────────────────────────────────

  const handleFileSelect = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files ?? [])
    if (!files.length) return
    setUploading(true)
    setUploadError('')
    try {
      const result = await api.containers.workspace.upload(agentId, conversationId, files, currentPath || undefined)
      if (result.errors.length) {
        setUploadError(result.errors.join('; '))
      }
      await load(currentPath)
    } catch (err) {
      setUploadError(String(err))
    } finally {
      setUploading(false)
      // Reset the input so the same file can be re-uploaded.
      if (fileInputRef.current) fileInputRef.current.value = ''
    }
  }

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault()
    const files = Array.from(e.dataTransfer.files)
    if (!files.length) return
    setUploading(true)
    setUploadError('')
    try {
      const result = await api.containers.workspace.upload(agentId, conversationId, files, currentPath || undefined)
      if (result.errors.length) setUploadError(result.errors.join('; '))
      await load(currentPath)
    } catch (err) {
      setUploadError(String(err))
    } finally {
      setUploading(false)
    }
  }

  // ── Delete ────────────────────────────────────────────────────

  const handleDelete = async (entry: WorkspaceEntry) => {
    if (!confirm(`Delete "${entry.name}"${entry.is_dir ? ' and all its contents' : ''}?`)) return
    setDeletingPath(entry.path)
    try {
      await api.containers.workspace.delete(agentId, conversationId, entry.path, entry.is_dir)
      await load(currentPath)
    } catch (err) {
      setError(String(err))
    } finally {
      setDeletingPath(null)
    }
  }

  // ── Render ────────────────────────────────────────────────────

  return (
    <div className={styles.browser}>
      {/* Header */}
      <div className={styles.header}>
        <div className={styles.breadcrumbs}>
          {breadcrumbs.map((crumb, i) => (
            <span key={crumb.path} className={styles.breadcrumbGroup}>
              {i > 0 && <ChevronRight size={12} className={styles.breadcrumbSep} />}
              <button
                className={`${styles.breadcrumb} ${i === breadcrumbs.length - 1 ? styles.breadcrumbActive : ''}`}
                onClick={() => load(crumb.path)}
              >
                {crumb.label}
              </button>
            </span>
          ))}
        </div>

        <div className={styles.headerActions}>
          {liveWatch && (
            <span className={`${styles.liveIndicator} ${liveActive ? styles.liveOn : styles.liveOff}`}>
              <Wifi size={11} />
              {liveActive ? 'live' : 'offline'}
            </span>
          )}
          {recentChange && (
            <span className={styles.changeToast}>{recentChange}</span>
          )}
          <button
            className={styles.iconBtn}
            onClick={() => load(currentPath)}
            title="Refresh"
            disabled={loading}
          >
            <RefreshCw size={14} className={loading ? styles.spinning : ''} />
          </button>
          <button
            className={styles.iconBtn}
            onClick={() => fileInputRef.current?.click()}
            title="Upload files"
            disabled={uploading}
          >
            {uploading
              ? <Loader2 size={14} className={styles.spinning} />
              : <Upload size={14} />}
          </button>
          <input
            ref={fileInputRef}
            type="file"
            multiple
            className={styles.hiddenInput}
            onChange={handleFileSelect}
          />
        </div>
      </div>

      {/* Error banner */}
      {(error || uploadError) && (
        <div className={styles.errorBanner}>
          <AlertCircle size={13} />
          <span>{error || uploadError}</span>
          <button onClick={() => { setError(''); setUploadError('') }}>✕</button>
        </div>
      )}

      {/* Drop zone + file table */}
      <div
        className={styles.dropZone}
        onDragOver={e => e.preventDefault()}
        onDrop={handleDrop}
      >
        {loading && !listing ? (
          <div className={styles.loadingState}>
            <Loader2 size={18} className={styles.spinning} />
          </div>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th className={styles.thName}>Name</th>
                <th className={styles.thSize}>Size</th>
                <th className={styles.thDate}>Modified</th>
                <th className={styles.thActions} />
              </tr>
            </thead>
            <tbody>
              {currentPath && (
                <tr className={styles.row} onClick={navigateUp}>
                  <td className={styles.tdName}>
                    <ArrowLeft size={13} className={styles.entryIcon} />
                    <span className={styles.entryName}>..</span>
                  </td>
                  <td /><td /><td />
                </tr>
              )}
              {listing?.entries.length === 0 && !currentPath && (
                <tr>
                  <td colSpan={4} className={styles.empty}>
                    Workspace is empty. Drag and drop files to upload.
                  </td>
                </tr>
              )}
              {listing?.entries.map(entry => (
                <tr
                  key={entry.path}
                  className={`${styles.row} ${entry.is_dir ? styles.rowDir : ''}`}
                  onClick={() => entry.is_dir && navigateInto(entry)}
                >
                  <td className={styles.tdName}>
                    {entry.is_dir
                      ? <FolderOpen size={13} className={`${styles.entryIcon} ${styles.iconDir}`} />
                      : <File size={13} className={styles.entryIcon} />}
                    <span className={styles.entryName}>{entry.name}</span>
                  </td>
                  <td className={styles.tdSize}>
                    {entry.is_dir ? '—' : formatBytes(entry.size)}
                  </td>
                  <td className={styles.tdDate}>{formatDate(entry.modified_at)}</td>
                  <td className={styles.tdActions} onClick={e => e.stopPropagation()}>
                    {!entry.is_dir && (
                      <a
                        href={api.containers.workspace.downloadUrl(agentId, conversationId, entry.path)}
                        download={entry.name}
                        className={styles.actionBtn}
                        title="Download"
                      >
                        <Download size={13} />
                      </a>
                    )}
                    <button
                      className={`${styles.actionBtn} ${styles.actionBtnDanger}`}
                      title="Delete"
                      disabled={deletingPath === entry.path}
                      onClick={() => handleDelete(entry)}
                    >
                      {deletingPath === entry.path
                        ? <Loader2 size={13} className={styles.spinning} />
                        : <Trash2 size={13} />}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <p className={styles.hint}>Drop files anywhere above to upload to the current directory.</p>
    </div>
  )
}
