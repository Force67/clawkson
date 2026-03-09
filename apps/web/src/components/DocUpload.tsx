import { useState, useRef, useCallback, type DragEvent, type ChangeEvent } from 'react'
import {
  Upload, FileText, X, CheckCircle, AlertCircle, Loader2,
  FileJson, FileSpreadsheet, File as FileIcon,
} from 'lucide-react'
import { api, type UploadResult } from '../lib/api'
import styles from './DocUpload.module.css'

interface DocUploadProps {
  kbId: string
  onEntriesCreated: () => void
  onClose: () => void
}

interface FileItem {
  id: string
  file: File
  name: string
  size: number
  ext: string
  preview: string
  status: 'pending' | 'uploading' | 'done' | 'error'
  error?: string
  entriesCreated?: number
}

const ACCEPTED = ['.txt', '.md', '.pdf', '.json', '.csv']
const MAX_SIZE = 10 * 1024 * 1024 // 10 MB

function fileIcon(ext: string) {
  switch (ext) {
    case '.json': return FileJson
    case '.csv': return FileSpreadsheet
    case '.md': return FileText
    default: return FileIcon
  }
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

let idCounter = 0

export function DocUpload({ kbId, onEntriesCreated, onClose }: DocUploadProps) {
  const [files, setFiles] = useState<FileItem[]>([])
  const [dragOver, setDragOver] = useState(false)
  const [uploading, setUploading] = useState(false)
  const [uploadSummary, setUploadSummary] = useState<UploadResult | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const dragCounter = useRef(0)

  const addFiles = useCallback((fileList: FileList | File[]) => {
    const arr = Array.from(fileList)
    const items: FileItem[] = []

    for (const file of arr) {
      const ext = '.' + file.name.split('.').pop()?.toLowerCase()
      if (!ACCEPTED.includes(ext)) continue
      if (file.size > MAX_SIZE) continue

      const preview = file.name.length > 40
        ? file.name.slice(0, 20) + '...' + file.name.slice(-15)
        : file.name

      items.push({
        id: `f-${++idCounter}`,
        file,
        name: file.name,
        size: file.size,
        ext,
        preview,
        status: 'pending',
      })
    }

    setFiles(prev => [...prev, ...items])
  }, [])

  const handleDragEnter = (e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    dragCounter.current++
    setDragOver(true)
  }

  const handleDragLeave = (e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    dragCounter.current--
    if (dragCounter.current === 0) setDragOver(false)
  }

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
  }

  const handleDrop = (e: DragEvent) => {
    e.preventDefault()
    e.stopPropagation()
    dragCounter.current = 0
    setDragOver(false)
    if (e.dataTransfer.files.length) {
      addFiles(e.dataTransfer.files)
    }
  }

  const handleFileSelect = (e: ChangeEvent<HTMLInputElement>) => {
    if (e.target.files?.length) {
      addFiles(e.target.files)
      e.target.value = ''
    }
  }

  const removeFile = (id: string) => {
    setFiles(prev => prev.filter(f => f.id !== id))
  }

  const uploadAll = async () => {
    if (uploading) return
    setUploading(true)

    const pending = files.filter(f => f.status === 'pending' || f.status === 'error')

    // Mark all pending as uploading
    setFiles(prev => prev.map(f =>
      pending.some(p => p.id === f.id) ? { ...f, status: 'uploading' as const, error: undefined } : f
    ))

    try {
      const result: UploadResult = await api.knowledge.uploadFiles(
        kbId,
        pending.map(f => f.file),
      )

      setUploadSummary(result)

      // Match errors back to files by filename prefix
      setFiles(prev => prev.map(f => {
        if (!pending.some(p => p.id === f.id)) return f
        const fileError = result.errors.find(e => e.startsWith(f.name))
        if (fileError) {
          return { ...f, status: 'error' as const, error: fileError }
        }
        return { ...f, status: 'done' as const }
      }))
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Upload failed'
      setFiles(prev => prev.map(f =>
        pending.some(p => p.id === f.id) ? { ...f, status: 'error' as const, error: msg } : f
      ))
    }

    setUploading(false)
    onEntriesCreated()
  }

  const pendingCount = files.filter(f => f.status === 'pending' || f.status === 'error').length
  const doneCount = files.filter(f => f.status === 'done').length

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={e => e.stopPropagation()}>
        <div className={styles.header}>
          <div className={styles.headerLeft}>
            <Upload size={18} className={styles.headerIcon} />
            <h2 className={styles.title}>Upload Documents</h2>
          </div>
          <button className={styles.closeBtn} onClick={onClose}>
            <X size={18} />
          </button>
        </div>

        <p className={styles.subtitle}>
          Drop your files below to add them as knowledge entries.
          Supports <span className={styles.formats}>.txt .md .pdf .json .csv</span>
        </p>

        <div
          className={`${styles.dropzone} ${dragOver ? styles.dropzoneActive : ''}`}
          onDragEnter={handleDragEnter}
          onDragLeave={handleDragLeave}
          onDragOver={handleDragOver}
          onDrop={handleDrop}
          onClick={() => inputRef.current?.click()}
        >
          <input
            ref={inputRef}
            type="file"
            accept={ACCEPTED.join(',')}
            multiple
            className={styles.fileInput}
            onChange={handleFileSelect}
          />
          <div className={styles.dropContent}>
            <div className={styles.dropIcon}>
              <Upload size={28} strokeWidth={1.5} />
            </div>
            <div className={styles.dropText}>
              <span className={styles.dropMain}>
                {dragOver ? 'Release to add files' : 'Drag & drop files here'}
              </span>
              <span className={styles.dropSub}>or click to browse</span>
            </div>
          </div>
          <div className={styles.dropBorder} />
        </div>

        {files.length > 0 && (
          <>
            <div className={styles.fileList}>
              {files.map(item => {
                const Icon = fileIcon(item.ext)
                return (
                  <div
                    key={item.id}
                    className={`${styles.fileItem} ${
                      item.status === 'done' ? styles.fileDone :
                      item.status === 'error' ? styles.fileError : ''
                    }`}
                  >
                    <div className={styles.fileIcon}>
                      <Icon size={18} />
                    </div>
                    <div className={styles.fileMeta}>
                      <span className={styles.fileName}>{item.name}</span>
                      <span className={styles.fileSize}>{formatSize(item.size)}</span>
                    </div>
                    <div className={styles.fileStatus}>
                      {item.status === 'pending' && (
                        <span className={styles.statusPending}>Ready</span>
                      )}
                      {item.status === 'uploading' && (
                        <Loader2 size={14} className={styles.spinning} />
                      )}
                      {item.status === 'done' && (
                        <CheckCircle size={14} className={styles.statusDone} />
                      )}
                      {item.status === 'error' && (
                        <span className={styles.statusErrorMsg} title={item.error}>
                          <AlertCircle size={14} /> {item.error}
                        </span>
                      )}
                    </div>
                    {(item.status === 'pending' || item.status === 'error') && (
                      <button
                        className={styles.removeBtn}
                        onClick={() => removeFile(item.id)}
                      >
                        <X size={14} />
                      </button>
                    )}
                  </div>
                )
              })}
            </div>

            {uploadSummary && (
              <div className={styles.uploadSummary}>
                <CheckCircle size={14} />
                {uploadSummary.entries_created} entries created, {uploadSummary.embedded} embedded
                {uploadSummary.embed_failed > 0 && (
                  <span className={styles.summaryWarn}>
                    <AlertCircle size={12} /> {uploadSummary.embed_failed} embed failed
                  </span>
                )}
              </div>
            )}

            <div className={styles.footer}>
              <div className={styles.footerInfo}>
                {doneCount > 0 && (
                  <span className={styles.doneCount}>{doneCount} uploaded</span>
                )}
                {pendingCount > 0 && (
                  <span className={styles.pendingCount}>{pendingCount} ready</span>
                )}
              </div>
              <div className={styles.footerActions}>
                <button className={styles.clearBtn} onClick={() => setFiles([])}>
                  Clear all
                </button>
                <button
                  className={styles.uploadBtn}
                  onClick={uploadAll}
                  disabled={uploading || pendingCount === 0}
                >
                  {uploading ? (
                    <>
                      <Loader2 size={15} className={styles.spinning} />
                      Uploading...
                    </>
                  ) : (
                    <>
                      <Upload size={15} />
                      Upload {pendingCount > 0 ? pendingCount : ''} {pendingCount === 1 ? 'file' : 'files'}
                    </>
                  )}
                </button>
              </div>
            </div>
          </>
        )}
      </div>
    </div>
  )
}
