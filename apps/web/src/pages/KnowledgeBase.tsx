import { useState, useEffect, useCallback } from 'react'
import {
  Plus, Database, Trash2, Share2, Zap, X, Upload,
  ChevronLeft, Users, Bot, FileText, CheckCircle, AlertCircle, Loader2,
} from 'lucide-react'
import { api, type KnowledgeBase, type KnowledgeEntry, type KbShareInfo, type Agent } from '../lib/api'
import { useAuth } from '../lib/auth'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import { EmptyState } from '../components/EmptyState'
import { DocUpload } from '../components/DocUpload'
import styles from './KnowledgeBase.module.css'

type View = 'list' | 'detail'

export function KnowledgeBasePage() {
  const { user } = useAuth()
  const [view, setView] = useState<View>('list')
  const [bases, setBases] = useState<KnowledgeBase[]>([])
  const [selectedKb, setSelectedKb] = useState<KnowledgeBase | null>(null)
  const [entries, setEntries] = useState<KnowledgeEntry[]>([])
  const [agents, setAgents] = useState<Agent[]>([])
  const [linkedAgentIds, setLinkedAgentIds] = useState<string[]>([])
  const [shares, setShares] = useState<KbShareInfo[]>([])
  const [loading, setLoading] = useState(true)

  // Create KB form
  const [showCreateKb, setShowCreateKb] = useState(false)
  const [newKbName, setNewKbName] = useState('')
  const [newKbDesc, setNewKbDesc] = useState('')

  // Create entry form
  const [showCreateEntry, setShowCreateEntry] = useState(false)
  const [newEntryTitle, setNewEntryTitle] = useState('')
  const [newEntryContent, setNewEntryContent] = useState('')

  // Share form
  const [showSharePanel, setShowSharePanel] = useState(false)
  const [shareEmail, setShareEmail] = useState('')
  const [shareError, setShareError] = useState('')

  // Agent panel
  const [showAgentPanel, setShowAgentPanel] = useState(false)

  // Upload modal
  const [showUpload, setShowUpload] = useState(false)

  // Embed state
  const [embedding, setEmbedding] = useState(false)
  const [embedResult, setEmbedResult] = useState<{ embedded: number; failed: number } | null>(null)

  const loadBases = useCallback(async () => {
    try {
      const data = await api.knowledge.listBases()
      setBases(data)
    } catch { /* */ }
    setLoading(false)
  }, [])

  useEffect(() => { loadBases() }, [loadBases])

  const openKb = useCallback(async (kb: KnowledgeBase) => {
    setSelectedKb(kb)
    setView('detail')
    setEmbedResult(null)
    try {
      const [ents, agentIds, allAgents] = await Promise.all([
        api.knowledge.listEntries(kb.id),
        api.knowledge.listAgents(kb.id),
        api.agents.list(),
      ])
      setEntries(ents)
      setLinkedAgentIds(agentIds)
      setAgents(allAgents)
      if (kb.owner_id === user?.id) {
        const sh = await api.knowledge.listShares(kb.id)
        setShares(sh)
      }
    } catch { /* */ }
  }, [user])

  const handleCreateKb = async () => {
    if (!newKbName.trim()) return
    try {
      const kb = await api.knowledge.createBase({ name: newKbName, description: newKbDesc })
      setBases(prev => [kb, ...prev])
      setNewKbName('')
      setNewKbDesc('')
      setShowCreateKb(false)
    } catch { /* */ }
  }

  const handleDeleteKb = async (id: string) => {
    try {
      await api.knowledge.deleteBase(id)
      setBases(prev => prev.filter(b => b.id !== id))
      if (selectedKb?.id === id) {
        setView('list')
        setSelectedKb(null)
      }
    } catch { /* */ }
  }

  const handleCreateEntry = async () => {
    if (!selectedKb || !newEntryTitle.trim() || !newEntryContent.trim()) return
    try {
      const entry = await api.knowledge.createEntry(selectedKb.id, {
        title: newEntryTitle,
        content: newEntryContent,
      })
      setEntries(prev => [entry, ...prev])
      setNewEntryTitle('')
      setNewEntryContent('')
      setShowCreateEntry(false)
    } catch { /* */ }
  }

  const handleDeleteEntry = async (entryId: string) => {
    if (!selectedKb) return
    try {
      await api.knowledge.deleteEntry(selectedKb.id, entryId)
      setEntries(prev => prev.filter(e => e.id !== entryId))
    } catch { /* */ }
  }

  // Embed error
  const [embedError, setEmbedError] = useState('')

  const handleEmbed = async () => {
    if (!selectedKb) return
    setEmbedding(true)
    setEmbedResult(null)
    setEmbedError('')
    try {
      const result = await api.knowledge.embed(selectedKb.id)
      setEmbedResult(result)
      // Refresh entries to update has_embedding status
      const ents = await api.knowledge.listEntries(selectedKb.id)
      setEntries(ents)
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Embedding failed'
      console.error('Embed error:', msg)
      setEmbedError(msg)
    }
    setEmbedding(false)
  }

  const handleShare = async () => {
    if (!selectedKb || !shareEmail.trim()) return
    setShareError('')
    try {
      const info = await api.knowledge.createShare(selectedKb.id, shareEmail, 'read')
      setShares(prev => [...prev, info])
      setShareEmail('')
    } catch (err) {
      setShareError(err instanceof Error && err.message.includes('404') ? 'User not found' : 'Failed to share')
    }
  }

  const handleRemoveShare = async (userId: string) => {
    if (!selectedKb) return
    try {
      await api.knowledge.removeShare(selectedKb.id, userId)
      setShares(prev => prev.filter(s => s.user_id !== userId))
    } catch { /* */ }
  }

  const handleLinkAgent = async (agentId: string) => {
    if (!selectedKb) return
    try {
      await api.knowledge.linkAgent(selectedKb.id, agentId)
      setLinkedAgentIds(prev => [...prev, agentId])
    } catch { /* */ }
  }

  const handleUnlinkAgent = async (agentId: string) => {
    if (!selectedKb) return
    try {
      await api.knowledge.unlinkAgent(selectedKb.id, agentId)
      setLinkedAgentIds(prev => prev.filter(id => id !== agentId))
    } catch { /* */ }
  }

  const isOwner = selectedKb?.owner_id === user?.id || user?.role === 'admin'
  const unembeddedCount = entries.filter(e => !e.has_embedding).length

  // ── List view ───────────────────────────────────────────────────
  if (view === 'list') {
    return (
      <div className="fade-in">
        <PageHeader
          title="Knowledge Base"
          description="Manage knowledge that your agents can search and reference."
          actions={
            <Button onClick={() => setShowCreateKb(true)}>
              <Plus size={16} /> New Knowledge Base
            </Button>
          }
        />

        {showCreateKb && (
          <Card className={styles.createForm}>
            <h3 className={styles.formTitle}>Create Knowledge Base</h3>
            <div className={styles.field}>
              <label className={styles.label}>Name</label>
              <input
                className={styles.input}
                value={newKbName}
                onChange={e => setNewKbName(e.target.value)}
                placeholder="e.g. Product Documentation"
                autoFocus
              />
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Description</label>
              <input
                className={styles.input}
                value={newKbDesc}
                onChange={e => setNewKbDesc(e.target.value)}
                placeholder="What kind of knowledge goes here?"
              />
            </div>
            <div className={styles.formActions}>
              <Button variant="ghost" onClick={() => setShowCreateKb(false)}>Cancel</Button>
              <Button onClick={handleCreateKb}>Create</Button>
            </div>
          </Card>
        )}

        {loading ? (
          <div className={styles.loadingState}>Loading...</div>
        ) : bases.length === 0 ? (
          <EmptyState
            icon={Database}
            title="No knowledge bases yet"
            description="Create a knowledge base to store documents, notes, and reference material for your agents."
          />
        ) : (
          <div className={styles.grid}>
            {bases.map(kb => (
              <Card key={kb.id} interactive onClick={() => openKb(kb)}>
                <div className={styles.kbHeader}>
                  <div className={styles.kbIcon}><Database size={18} /></div>
                  <div className={styles.kbMeta}>
                    <h3 className={styles.kbName}>{kb.name}</h3>
                    <span className={styles.kbCount}>{kb.entry_count} entries</span>
                  </div>
                  {(kb.owner_id === user?.id || user?.role === 'admin') && (
                    <button
                      className={styles.deleteBtn}
                      onClick={e => { e.stopPropagation(); handleDeleteKb(kb.id) }}
                      title="Delete"
                    >
                      <Trash2 size={14} />
                    </button>
                  )}
                </div>
                {kb.description && (
                  <p className={styles.kbDescription}>{kb.description}</p>
                )}
                <div className={styles.kbFooter}>
                  <span className={styles.kbModel}>{kb.embedding_model}</span>
                  {kb.owner_id !== user?.id && (
                    <span className={styles.sharedBadge}><Share2 size={10} /> Shared</span>
                  )}
                </div>
              </Card>
            ))}
          </div>
        )}
      </div>
    )
  }

  // ── Detail view ─────────────────────────────────────────────────
  return (
    <div className="fade-in">
      <div className={styles.detailHeader}>
        <button className={styles.backBtn} onClick={() => { setView('list'); setSelectedKb(null) }}>
          <ChevronLeft size={18} /> Back
        </button>
        <div className={styles.detailTitle}>
          <Database size={20} className={styles.detailIcon} />
          <div>
            <h2 className={styles.detailName}>{selectedKb?.name}</h2>
            {selectedKb?.description && (
              <p className={styles.detailDesc}>{selectedKb.description}</p>
            )}
          </div>
        </div>
        <div className={styles.detailActions}>
          {isOwner && (
            <>
              <Button variant="ghost" size="sm" onClick={() => setShowSharePanel(!showSharePanel)}>
                <Users size={14} /> Share
              </Button>
              <Button variant="ghost" size="sm" onClick={() => setShowAgentPanel(!showAgentPanel)}>
                <Bot size={14} /> Agents
              </Button>
            </>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={handleEmbed}
            disabled={embedding || unembeddedCount === 0}
          >
            {embedding ? <Loader2 size={14} className={styles.spinning} /> : <Zap size={14} />}
            {embedding ? 'Embedding...' : unembeddedCount > 0 ? `Embed ${unembeddedCount} entries` : 'All embedded'}
          </Button>
          {isOwner && (
            <>
              <Button variant="ghost" size="sm" onClick={() => setShowUpload(true)}>
                <Upload size={14} /> Upload Files
              </Button>
              <Button size="sm" onClick={() => setShowCreateEntry(true)}>
                <Plus size={14} /> Add Entry
              </Button>
            </>
          )}
        </div>
      </div>

      {embedResult && (
        <div className={styles.embedNotice}>
          <CheckCircle size={14} />
          Embedded {embedResult.embedded} entries
          {embedResult.failed > 0 && <>, <AlertCircle size={14} /> {embedResult.failed} failed</>}
        </div>
      )}

      {embedError && (
        <div className={styles.embedErrorNotice}>
          <AlertCircle size={14} />
          Embedding failed: {embedError}
        </div>
      )}

      {/* Share panel */}
      {showSharePanel && (
        <Card className={styles.sidePanel}>
          <div className={styles.panelHeader}>
            <h3>Shared With</h3>
            <button className={styles.closeBtn} onClick={() => setShowSharePanel(false)}><X size={16} /></button>
          </div>
          <div className={styles.shareForm}>
            <input
              className={styles.input}
              value={shareEmail}
              onChange={e => setShareEmail(e.target.value)}
              placeholder="Email address"
              onKeyDown={e => e.key === 'Enter' && handleShare()}
            />
            <Button size="sm" onClick={handleShare}>Share</Button>
          </div>
          {shareError && <div className={styles.shareError}>{shareError}</div>}
          <div className={styles.shareList}>
            {shares.map(s => (
              <div key={s.user_id} className={styles.shareItem}>
                <div className={styles.shareUser}>
                  <span className={styles.shareAvatar}>{s.display_name.charAt(0).toUpperCase()}</span>
                  <div>
                    <div className={styles.shareName}>{s.display_name}</div>
                    <div className={styles.shareEmail}>{s.email}</div>
                  </div>
                </div>
                <button className={styles.removeShareBtn} onClick={() => handleRemoveShare(s.user_id)}>
                  <X size={14} />
                </button>
              </div>
            ))}
            {shares.length === 0 && <div className={styles.emptyPanel}>Not shared with anyone yet.</div>}
          </div>
        </Card>
      )}

      {/* Agent access panel */}
      {showAgentPanel && (
        <Card className={styles.sidePanel}>
          <div className={styles.panelHeader}>
            <h3>Agent Access</h3>
            <button className={styles.closeBtn} onClick={() => setShowAgentPanel(false)}><X size={16} /></button>
          </div>
          <div className={styles.agentList}>
            {agents.map(agent => {
              const linked = linkedAgentIds.includes(agent.id)
              return (
                <div key={agent.id} className={`${styles.agentItem} ${linked ? styles.agentLinked : ''}`}>
                  <div className={styles.agentInfo}>
                    <Bot size={16} />
                    <span>{agent.name}</span>
                  </div>
                  <button
                    className={`${styles.agentToggle} ${linked ? styles.agentToggleActive : ''}`}
                    onClick={() => linked ? handleUnlinkAgent(agent.id) : handleLinkAgent(agent.id)}
                  >
                    {linked ? 'Remove' : 'Grant'}
                  </button>
                </div>
              )
            })}
            {agents.length === 0 && <div className={styles.emptyPanel}>No agents configured yet.</div>}
          </div>
        </Card>
      )}

      {/* Create entry form */}
      {showCreateEntry && (
        <Card className={styles.createForm}>
          <h3 className={styles.formTitle}>Add Entry</h3>
          <div className={styles.field}>
            <label className={styles.label}>Title</label>
            <input
              className={styles.input}
              value={newEntryTitle}
              onChange={e => setNewEntryTitle(e.target.value)}
              placeholder="Entry title"
              autoFocus
            />
          </div>
          <div className={styles.field}>
            <label className={styles.label}>Content</label>
            <textarea
              className={styles.textarea}
              value={newEntryContent}
              onChange={e => setNewEntryContent(e.target.value)}
              placeholder="Paste or write knowledge content..."
              rows={6}
            />
          </div>
          <div className={styles.formActions}>
            <Button variant="ghost" onClick={() => setShowCreateEntry(false)}>Cancel</Button>
            <Button onClick={handleCreateEntry}>Add Entry</Button>
          </div>
        </Card>
      )}

      {/* Entries list */}
      {entries.length === 0 ? (
        <EmptyState
          icon={FileText}
          title="No entries yet"
          description="Add knowledge entries — text documents, notes, or reference material."
        />
      ) : (
        <div className={styles.entriesList}>
          {entries.map(entry => (
            <div key={entry.id} className={styles.entryCard}>
              <div className={styles.entryTop}>
                <div className={styles.entryTitleRow}>
                  <FileText size={16} className={styles.entryIcon} />
                  <h4 className={styles.entryTitle}>{entry.title}</h4>
                </div>
                <div className={styles.entryStatus}>
                  {entry.has_embedding ? (
                    <span className={styles.embeddedBadge}><CheckCircle size={12} /> Embedded</span>
                  ) : (
                    <span className={styles.pendingBadge}><AlertCircle size={12} /> Pending</span>
                  )}
                  {isOwner && (
                    <button
                      className={styles.deleteEntryBtn}
                      onClick={() => handleDeleteEntry(entry.id)}
                    >
                      <Trash2 size={13} />
                    </button>
                  )}
                </div>
              </div>
              <p className={styles.entryContent}>{entry.content}</p>
            </div>
          ))}
        </div>
      )}

      {showUpload && selectedKb && (
        <DocUpload
          kbId={selectedKb.id}
          onEntriesCreated={async () => {
            const ents = await api.knowledge.listEntries(selectedKb.id)
            setEntries(ents)
          }}
          onClose={() => setShowUpload(false)}
        />
      )}
    </div>
  )
}
