import { useState, useEffect, useCallback } from 'react'
import {
  Container,
  Loader2,
  RefreshCw,
  Square,
  Trash2,
  Bot,
  HardDrive,
  Cpu,
  CircleDot,
  MessageSquare,
} from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { PageHeader } from '../components/PageHeader'
import { Card } from '../components/Card'
import { Button } from '../components/Button'
import { EmptyState } from '../components/EmptyState'
import { api, type ContainerStatus, type Agent } from '../lib/api'
import styles from './Containers.module.css'

type ContainerWithAgent = ContainerStatus & { agent_name?: string }

export function ContainersPage() {
  const [containers, setContainers] = useState<ContainerWithAgent[]>([])
  const [loading, setLoading] = useState(true)
  const [refreshing, setRefreshing] = useState(false)
  const [actionInFlight, setActionInFlight] = useState<string | null>(null)
  const navigate = useNavigate()

  const fetchData = useCallback(async (showRefresh = false) => {
    if (showRefresh) setRefreshing(true)
    try {
      const [ctrs, agents] = await Promise.all([
        api.containers.list(),
        api.agents.list(),
      ])
      const agentMap = new Map(agents.map((a: Agent) => [a.id, a.name]))
      setContainers(
        ctrs.map((c: ContainerStatus) => ({
          ...c,
          agent_name: agentMap.get(c.agent_id),
        }))
      )
    } catch {
      setContainers([])
    } finally {
      setLoading(false)
      setRefreshing(false)
    }
  }, [])

  useEffect(() => { fetchData() }, [fetchData])

  const handleStop = async (c: ContainerWithAgent) => {
    const key = `${c.agent_id}:${c.conversation_id}`
    setActionInFlight(key)
    try {
      await api.containers.stop(c.agent_id, c.conversation_id)
      await fetchData()
    } finally {
      setActionInFlight(null)
    }
  }

  const handleRemove = async (c: ContainerWithAgent) => {
    const key = `${c.agent_id}:${c.conversation_id}`
    setActionInFlight(key)
    try {
      await api.containers.remove(c.agent_id, c.conversation_id)
      setContainers(prev =>
        prev.filter(x => !(x.agent_id === c.agent_id && x.conversation_id === c.conversation_id))
      )
    } finally {
      setActionInFlight(null)
    }
  }

  const runningCount = containers.filter(c => c.state === 'running').length
  const stoppedCount = containers.filter(c => c.state === 'stopped').length

  return (
    <div className="fade-in">
      <PageHeader
        title="Containers"
        description="Active sandbox containers across all agents and conversations."
        actions={
          <Button variant="secondary" size="sm" onClick={() => fetchData(true)} disabled={refreshing}>
            <RefreshCw size={14} className={refreshing ? 'spinning' : ''} /> Refresh
          </Button>
        }
      />

      {/* Stats strip */}
      {!loading && containers.length > 0 && (
        <div className={styles.stats}>
          <div className={styles.stat}>
            <Container size={13} />
            <span className={styles.statValue}>{containers.length}</span>
            <span className={styles.statLabel}>Total</span>
          </div>
          <div className={styles.stat}>
            <CircleDot size={13} className={styles.statRunning} />
            <span className={styles.statValue}>{runningCount}</span>
            <span className={styles.statLabel}>Running</span>
          </div>
          <div className={styles.stat}>
            <Square size={13} className={styles.statStopped} />
            <span className={styles.statValue}>{stoppedCount}</span>
            <span className={styles.statLabel}>Stopped</span>
          </div>
        </div>
      )}

      {loading ? (
        <Card>
          <div className={styles.loadingState}>
            <Loader2 size={20} className="spinning" />
            <span>Loading containers...</span>
          </div>
        </Card>
      ) : containers.length === 0 ? (
        <EmptyState
          icon={Container}
          title="No containers"
          description="No sandbox containers are currently active. Containers are created automatically when chatting with agents that have sandboxing enabled."
        />
      ) : (
        <div className={styles.containerList}>
          {containers.map(c => {
            const key = `${c.agent_id}:${c.conversation_id}`
            const busy = actionInFlight === key
            const isRunning = c.state === 'running'

            return (
              <div key={key} className={styles.containerCard}>
                <div className={styles.cardTop}>
                  <div className={styles.cardLeft}>
                    <div className={`${styles.stateIcon} ${styles[`state_${c.state}`]}`}>
                      <Container size={16} />
                    </div>
                    <div className={styles.cardInfo}>
                      <div className={styles.cardAgent}>
                        <Bot size={12} />
                        <span>{c.agent_name || c.agent_id.slice(0, 8)}</span>
                      </div>
                      <div className={styles.cardConvo}>
                        <MessageSquare size={11} />
                        <span
                          className={styles.convoLink}
                          onClick={() => navigate(`/conversations/${c.conversation_id}`)}
                        >
                          {c.conversation_id.slice(0, 8)}...
                        </span>
                      </div>
                    </div>
                  </div>
                  <div className={`${styles.stateBadge} ${styles[`badge_${c.state}`]}`}>
                    {c.state}
                  </div>
                </div>

                <div className={styles.cardMeta}>
                  <span className={styles.metaTag}>
                    <HardDrive size={10} /> {c.image}
                  </span>
                  <span className={styles.metaTag}>
                    <Cpu size={10} /> {c.workspace_path.split('/').slice(-2).join('/')}
                  </span>
                </div>

                <div className={styles.cardActions}>
                  {isRunning && (
                    <button
                      className={styles.actionBtn}
                      onClick={() => handleStop(c)}
                      disabled={busy}
                      title="Stop container"
                    >
                      {busy ? <Loader2 size={12} className="spinning" /> : <Square size={12} />}
                      Stop
                    </button>
                  )}
                  <button
                    className={`${styles.actionBtn} ${styles.actionBtnDanger}`}
                    onClick={() => handleRemove(c)}
                    disabled={busy}
                    title="Remove container"
                  >
                    {busy ? <Loader2 size={12} className="spinning" /> : <Trash2 size={12} />}
                    Remove
                  </button>
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
