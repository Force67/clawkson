import { useState, useEffect, useMemo, useCallback, useRef } from 'react'
import {
  ChevronLeft,
  ChevronRight,
  Plus,
  X,
  Clock,
  MapPin,
  Tag,
  Trash2,
  Edit3,
  Calendar as CalendarIcon,
  Sun,
  Sunrise,
  Sunset,
  Moon,
  Check,
  Circle,
  Share2,
  Users,
  Eye,
  Pencil,
  UserPlus,
  Loader2,
} from 'lucide-react'
import {
  api,
  type CalendarEvent,
  type EventCategory,
  type CalendarShareResponse,
  type SharedCalendar,
  type SharePermission,
} from '../lib/api'
import styles from './Calendar.module.css'

// ── Constants ─────────────────────────────────────────────────────

type ViewMode = 'month' | 'week' | 'day'

const CATEGORIES: Record<EventCategory, { label: string; color: string; bg: string }> = {
  work:     { label: 'Work',     color: '#4a6cf7', bg: 'rgba(74, 108, 247, 0.10)' },
  meeting:  { label: 'Meeting',  color: '#e8685a', bg: 'rgba(232, 104, 90, 0.10)' },
  personal: { label: 'Personal', color: '#34d399', bg: 'rgba(52, 211, 153, 0.10)' },
  health:   { label: 'Health',   color: '#f59e0b', bg: 'rgba(245, 158, 11, 0.10)' },
  travel:   { label: 'Travel',   color: '#8b5cf6', bg: 'rgba(139, 92, 246, 0.10)' },
  creative: { label: 'Creative', color: '#ec4899', bg: 'rgba(236, 72, 153, 0.10)' },
}

const DAYS = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']
const MONTHS = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December',
]

const HOURS = Array.from({ length: 24 }, (_, i) => i)

// ── Helpers ───────────────────────────────────────────────────────

function toDateStr(d: Date): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

function parseDate(s: string): Date {
  const [y, m, d] = s.split('-').map(Number)
  return new Date(y, m - 1, d)
}

function getMonday(d: Date): Date {
  const day = d.getDay()
  const diff = d.getDate() - day + (day === 0 ? -6 : 1)
  return new Date(d.getFullYear(), d.getMonth(), diff)
}

function addDays(d: Date, n: number): Date {
  const r = new Date(d)
  r.setDate(r.getDate() + n)
  return r
}

function getMonthDays(year: number, month: number): Date[] {
  const first = new Date(year, month, 1)
  const firstDay = first.getDay()
  const offset = firstDay === 0 ? -6 : 1 - firstDay
  const start = new Date(year, month, 1 + offset)
  const days: Date[] = []
  for (let i = 0; i < 42; i++) days.push(addDays(start, i))
  return days
}

function formatTime12(t: string): string {
  const [h, m] = t.split(':').map(Number)
  const ampm = h >= 12 ? 'pm' : 'am'
  const hour = h % 12 || 12
  return `${hour}:${m.toString().padStart(2, '0')}${ampm}`
}

function getTimeOfDay(h: number): { icon: typeof Sun; label: string } {
  if (h < 6)  return { icon: Moon, label: 'Night' }
  if (h < 12) return { icon: Sunrise, label: 'Morning' }
  if (h < 17) return { icon: Sun, label: 'Afternoon' }
  if (h < 21) return { icon: Sunset, label: 'Evening' }
  return { icon: Moon, label: 'Night' }
}

// ── Component ─────────────────────────────────────────────────────

export function CalendarPage() {
  const today = useMemo(() => toDateStr(new Date()), [])
  const [selectedDate, setSelectedDate] = useState(today)
  const [viewMode, setViewMode] = useState<ViewMode>('month')
  const [viewDate, setViewDate] = useState(new Date())
  const [events, setEvents] = useState<CalendarEvent[]>([])
  const [loading, setLoading] = useState(true)
  const [showModal, setShowModal] = useState(false)
  const [editingEvent, setEditingEvent] = useState<CalendarEvent | null>(null)
  const [miniCalMonth, setMiniCalMonth] = useState(new Date())
  const [showSharePanel, setShowSharePanel] = useState(false)

  // Shared calendars state
  const [sharedCalendars, setSharedCalendars] = useState<SharedCalendar[]>([])
  const [viewingCalendar, setViewingCalendar] = useState<SharedCalendar | null>(null)
  const [sharedEvents, setSharedEvents] = useState<CalendarEvent[]>([])

  // ── Data fetching ──────────────────────────────────────────────

  const loadEvents = useCallback(async () => {
    try {
      const data = await api.calendar.list()
      setEvents(data)
    } catch (err) {
      console.error('Failed to load calendar events:', err)
    } finally {
      setLoading(false)
    }
  }, [])

  const loadSharedCalendars = useCallback(async () => {
    try {
      const cals = await api.calendar.listSharedCalendars()
      setSharedCalendars(cals)
    } catch { /* ignore */ }
  }, [])

  useEffect(() => {
    loadEvents()
    loadSharedCalendars()
  }, [loadEvents, loadSharedCalendars])

  // Load shared calendar events when viewing someone else's calendar
  useEffect(() => {
    if (!viewingCalendar) {
      setSharedEvents([])
      return
    }
    api.calendar.listSharedEvents(viewingCalendar.owner_id)
      .then(setSharedEvents)
      .catch(() => setSharedEvents([]))
  }, [viewingCalendar])

  // The events to display — either own or shared
  const displayEvents = viewingCalendar ? sharedEvents : events
  const isReadOnly = viewingCalendar ? viewingCalendar.permission === 'read' : false

  // ── Event CRUD ────────────────────────────────────────────────

  const saveEvent = useCallback(async (data: {
    id?: string; title: string; date: string; start_time: string; end_time: string;
    category: string; location?: string; notes?: string; completed?: boolean
  }) => {
    try {
      if (data.id) {
        const updated = await api.calendar.update(data.id, {
          title: data.title, date: data.date, start_time: data.start_time,
          end_time: data.end_time, category: data.category,
          location: data.location, notes: data.notes, completed: data.completed,
        })
        setEvents(prev => prev.map(e => e.id === updated.id ? updated : e))
      } else {
        const created = await api.calendar.create({
          title: data.title, date: data.date, start_time: data.start_time,
          end_time: data.end_time, category: data.category,
          location: data.location, notes: data.notes,
        })
        setEvents(prev => [...prev, created])
      }
    } catch (err) {
      console.error('Failed to save event:', err)
    }
    setShowModal(false)
    setEditingEvent(null)
  }, [])

  const deleteEvent = useCallback(async (id: string) => {
    try {
      await api.calendar.delete(id)
      setEvents(prev => prev.filter(e => e.id !== id))
    } catch (err) {
      console.error('Failed to delete event:', err)
    }
    setShowModal(false)
    setEditingEvent(null)
  }, [])

  const toggleComplete = useCallback(async (id: string) => {
    const evt = events.find(e => e.id === id)
    if (!evt) return
    const newVal = !evt.completed
    // Optimistic update
    setEvents(prev => prev.map(e => e.id === id ? { ...e, completed: newVal } : e))
    try {
      await api.calendar.toggleComplete(id, newVal)
    } catch {
      // Revert on failure
      setEvents(prev => prev.map(e => e.id === id ? { ...e, completed: !newVal } : e))
    }
  }, [events])

  // ── Navigation ────────────────────────────────────────────────

  const nav = useCallback((dir: -1 | 1) => {
    setViewDate(prev => {
      const d = new Date(prev)
      if (viewMode === 'month') d.setMonth(d.getMonth() + dir)
      else if (viewMode === 'week') d.setDate(d.getDate() + dir * 7)
      else d.setDate(d.getDate() + dir)
      return d
    })
  }, [viewMode])

  const goToday = useCallback(() => {
    setViewDate(new Date())
    setSelectedDate(today)
    setMiniCalMonth(new Date())
  }, [today])

  // ── Derived data ──────────────────────────────────────────────

  const monthDays = useMemo(
    () => getMonthDays(viewDate.getFullYear(), viewDate.getMonth()),
    [viewDate]
  )
  const weekStart = useMemo(() => getMonday(viewDate), [viewDate])
  const weekDays = useMemo(
    () => Array.from({ length: 7 }, (_, i) => addDays(weekStart, i)),
    [weekStart]
  )

  const eventsForDate = useCallback(
    (d: string) => displayEvents
      .filter(e => e.date === d)
      .sort((a, b) => a.start_time.localeCompare(b.start_time)),
    [displayEvents]
  )

  const selectedEvents = useMemo(() => eventsForDate(selectedDate), [eventsForDate, selectedDate])

  const upcoming = useMemo(() => {
    const todayDate = new Date()
    const endDate = addDays(todayDate, 7)
    const todayStr = toDateStr(todayDate)
    const endStr = toDateStr(endDate)
    return displayEvents
      .filter(e => e.date >= todayStr && e.date <= endStr)
      .sort((a, b) => a.date.localeCompare(b.date) || a.start_time.localeCompare(b.start_time))
  }, [displayEvents])

  const miniDays = useMemo(
    () => getMonthDays(miniCalMonth.getFullYear(), miniCalMonth.getMonth()),
    [miniCalMonth]
  )

  const headingText = useMemo(() => {
    if (viewMode === 'month') {
      return `${MONTHS[viewDate.getMonth()]} ${viewDate.getFullYear()}`
    }
    if (viewMode === 'week') {
      const end = addDays(weekStart, 6)
      const sameMonth = weekStart.getMonth() === end.getMonth()
      if (sameMonth) {
        return `${MONTHS[weekStart.getMonth()]} ${weekStart.getDate()}\u2013${end.getDate()}, ${weekStart.getFullYear()}`
      }
      return `${MONTHS[weekStart.getMonth()].slice(0, 3)} ${weekStart.getDate()} \u2013 ${MONTHS[end.getMonth()].slice(0, 3)} ${end.getDate()}, ${end.getFullYear()}`
    }
    const sd = parseDate(selectedDate)
    return `${DAYS[(sd.getDay() + 6) % 7]}, ${MONTHS[sd.getMonth()]} ${sd.getDate()}`
  }, [viewMode, viewDate, weekStart, selectedDate])

  // ── Render ────────────────────────────────────────────────────

  return (
    <div className="fade-in">
      {/* Header */}
      <div className={styles.header}>
        <div className={styles.headerLeft}>
          <h1 className={styles.title}>// Calendar</h1>
          {viewingCalendar && (
            <div className={styles.viewingBadge}>
              <Eye size={12} />
              <span>{viewingCalendar.display_name}'s calendar</span>
              <button onClick={() => setViewingCalendar(null)} className={styles.viewingClose}>
                <X size={11} />
              </button>
            </div>
          )}
        </div>
        <div className={styles.headerActions}>
          <button className={styles.todayBtn} onClick={goToday}>Today</button>
          <button
            className={styles.shareBtn}
            onClick={() => setShowSharePanel(true)}
            title="Sharing"
          >
            <Share2 size={14} />
          </button>
          {!isReadOnly && (
            <button className={styles.addBtn} onClick={() => { setEditingEvent(null); setShowModal(true) }}>
              <Plus size={15} strokeWidth={2.5} />
              <span>New Event</span>
            </button>
          )}
        </div>
      </div>

      <div className={styles.layout}>
        {/* ── Left sidebar ── */}
        <aside className={styles.sidebar}>
          {/* Mini Calendar */}
          <div className={styles.miniCal}>
            <div className={styles.miniCalHeader}>
              <span className={styles.miniCalTitle}>
                {MONTHS[miniCalMonth.getMonth()].slice(0, 3)} {miniCalMonth.getFullYear()}
              </span>
              <div className={styles.miniCalNav}>
                <button onClick={() => setMiniCalMonth(d => { const n = new Date(d); n.setMonth(n.getMonth() - 1); return n })}>
                  <ChevronLeft size={14} />
                </button>
                <button onClick={() => setMiniCalMonth(d => { const n = new Date(d); n.setMonth(n.getMonth() + 1); return n })}>
                  <ChevronRight size={14} />
                </button>
              </div>
            </div>
            <div className={styles.miniCalGrid}>
              {['M','T','W','T','F','S','S'].map((d, i) => (
                <span key={i} className={styles.miniCalDayLabel}>{d}</span>
              ))}
              {miniDays.map((d, i) => {
                const ds = toDateStr(d)
                const isCurrentMonth = d.getMonth() === miniCalMonth.getMonth()
                const isToday = ds === today
                const isSelected = ds === selectedDate
                const hasEvents = displayEvents.some(e => e.date === ds)
                return (
                  <button
                    key={i}
                    className={`${styles.miniCalDay} ${!isCurrentMonth ? styles.miniCalDayMuted : ''} ${isToday ? styles.miniCalDayToday : ''} ${isSelected ? styles.miniCalDaySelected : ''}`}
                    onClick={() => { setSelectedDate(ds); setViewDate(d) }}
                  >
                    {d.getDate()}
                    {hasEvents && <span className={styles.miniCalDot} />}
                  </button>
                )
              })}
            </div>
          </div>

          {/* Shared Calendars */}
          {sharedCalendars.length > 0 && (
            <div className={styles.legendCard}>
              <span className={styles.legendTitle}>Shared with me</span>
              <div className={styles.legendList}>
                {sharedCalendars.map(cal => (
                  <div
                    key={cal.owner_id}
                    className={`${styles.sharedCalItem} ${viewingCalendar?.owner_id === cal.owner_id ? styles.sharedCalItemActive : ''}`}
                    onClick={() => setViewingCalendar(
                      viewingCalendar?.owner_id === cal.owner_id ? null : cal
                    )}
                  >
                    <div className={styles.sharedCalAvatar}>
                      {cal.display_name.charAt(0).toUpperCase()}
                    </div>
                    <div className={styles.sharedCalInfo}>
                      <span className={styles.sharedCalName}>{cal.display_name}</span>
                      <span className={styles.sharedCalPerm}>
                        {cal.permission === 'write' ? <Pencil size={9} /> : <Eye size={9} />}
                        {cal.permission}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Category Legend */}
          <div className={styles.legendCard}>
            <span className={styles.legendTitle}>Categories</span>
            <div className={styles.legendList}>
              {(Object.entries(CATEGORIES) as [EventCategory, typeof CATEGORIES[EventCategory]][]).map(([key, cat]) => (
                <div key={key} className={styles.legendItem}>
                  <span className={styles.legendDot} style={{ background: cat.color }} />
                  <span>{cat.label}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Upcoming */}
          <div className={styles.upcomingCard}>
            <span className={styles.upcomingTitle}>Upcoming</span>
            {loading ? (
              <div className={styles.upcomingEmpty}>
                <Loader2 size={16} className="spinning" />
              </div>
            ) : upcoming.length === 0 ? (
              <div className={styles.upcomingEmpty}>
                <CalendarIcon size={16} strokeWidth={1.2} />
                <span>No upcoming events</span>
              </div>
            ) : (
              <div className={styles.upcomingList}>
                {upcoming.slice(0, 8).map(evt => {
                  const cat = CATEGORIES[evt.category as EventCategory] || CATEGORIES.work
                  const d = parseDate(evt.date)
                  const isEventToday = evt.date === today
                  return (
                    <div
                      key={evt.id}
                      className={`${styles.upcomingItem} ${evt.completed ? styles.upcomingItemDone : ''}`}
                      onClick={() => { if (!isReadOnly) { setEditingEvent(evt); setShowModal(true) } }}
                    >
                      <div className={styles.upcomingItemAccent} style={{ background: cat.color }} />
                      <div className={styles.upcomingItemContent}>
                        <span className={styles.upcomingItemTitle}>{evt.title}</span>
                        <span className={styles.upcomingItemMeta}>
                          {isEventToday ? 'Today' : `${MONTHS[d.getMonth()].slice(0, 3)} ${d.getDate()}`}
                          {' \u00b7 '}
                          {formatTime12(evt.start_time)}
                        </span>
                      </div>
                      {!isReadOnly && (
                        <button
                          className={styles.checkBtn}
                          onClick={(e) => { e.stopPropagation(); toggleComplete(evt.id) }}
                        >
                          {evt.completed ? <Check size={12} /> : <Circle size={12} />}
                        </button>
                      )}
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        </aside>

        {/* ── Main calendar area ── */}
        <main className={styles.main}>
          {/* View controls */}
          <div className={styles.viewControls}>
            <div className={styles.navGroup}>
              <button className={styles.navBtn} onClick={() => nav(-1)}>
                <ChevronLeft size={16} />
              </button>
              <h2 className={styles.viewHeading}>{headingText}</h2>
              <button className={styles.navBtn} onClick={() => nav(1)}>
                <ChevronRight size={16} />
              </button>
            </div>
            <div className={styles.viewToggle}>
              {(['month', 'week', 'day'] as ViewMode[]).map(m => (
                <button
                  key={m}
                  className={`${styles.viewToggleBtn} ${viewMode === m ? styles.viewToggleBtnActive : ''}`}
                  onClick={() => setViewMode(m)}
                >
                  {m.charAt(0).toUpperCase() + m.slice(1)}
                </button>
              ))}
            </div>
          </div>

          {/* ── Month View ── */}
          {viewMode === 'month' && (
            <div className={styles.monthGrid}>
              {DAYS.map(d => (
                <div key={d} className={styles.monthDayLabel}>{d}</div>
              ))}
              {monthDays.map((d, i) => {
                const ds = toDateStr(d)
                const isCurrentMonth = d.getMonth() === viewDate.getMonth()
                const isToday = ds === today
                const isSelected = ds === selectedDate
                const dayEvents = eventsForDate(ds)
                return (
                  <div
                    key={i}
                    className={`${styles.monthCell} ${!isCurrentMonth ? styles.monthCellMuted : ''} ${isToday ? styles.monthCellToday : ''} ${isSelected ? styles.monthCellSelected : ''}`}
                    onClick={() => setSelectedDate(ds)}
                    onDoubleClick={() => { setSelectedDate(ds); setViewMode('day') }}
                  >
                    <span className={`${styles.monthCellDate} ${isToday ? styles.monthCellDateToday : ''}`}>
                      {d.getDate()}
                    </span>
                    <div className={styles.monthCellEvents}>
                      {dayEvents.slice(0, 3).map(evt => {
                        const cat = CATEGORIES[evt.category as EventCategory] || CATEGORIES.work
                        return (
                          <div
                            key={evt.id}
                            className={`${styles.monthEvent} ${evt.completed ? styles.monthEventDone : ''}`}
                            style={{ background: cat.bg, borderLeftColor: cat.color }}
                            onClick={(e) => { e.stopPropagation(); if (!isReadOnly) { setEditingEvent(evt); setShowModal(true) } }}
                          >
                            <span className={styles.monthEventTime}>{formatTime12(evt.start_time)}</span>
                            <span className={styles.monthEventTitle}>{evt.title}</span>
                          </div>
                        )
                      })}
                      {dayEvents.length > 3 && (
                        <span className={styles.monthEventMore}>+{dayEvents.length - 3} more</span>
                      )}
                    </div>
                  </div>
                )
              })}
            </div>
          )}

          {/* ── Week View ── */}
          {viewMode === 'week' && (
            <div className={styles.weekView}>
              <div className={styles.weekHeader}>
                <div className={styles.weekTimeGutter} />
                {weekDays.map(d => {
                  const ds = toDateStr(d)
                  const isToday = ds === today
                  const isSelected = ds === selectedDate
                  return (
                    <div
                      key={ds}
                      className={`${styles.weekDayHeader} ${isToday ? styles.weekDayHeaderToday : ''} ${isSelected ? styles.weekDayHeaderSelected : ''}`}
                      onClick={() => setSelectedDate(ds)}
                    >
                      <span className={styles.weekDayName}>{DAYS[(d.getDay() + 6) % 7]}</span>
                      <span className={`${styles.weekDayNum} ${isToday ? styles.weekDayNumToday : ''}`}>
                        {d.getDate()}
                      </span>
                    </div>
                  )
                })}
              </div>
              <div className={styles.weekBody}>
                <div className={styles.weekGrid}>
                  <div className={styles.weekTimeGutter}>
                    {HOURS.map(h => (
                      <div key={h} className={styles.weekTimeSlot}>
                        <span className={styles.weekTimeLabel}>
                          {h === 0 ? '12 am' : h < 12 ? `${h} am` : h === 12 ? '12 pm' : `${h - 12} pm`}
                        </span>
                      </div>
                    ))}
                  </div>
                  {weekDays.map(d => {
                    const ds = toDateStr(d)
                    const isToday = ds === today
                    const dayEvents = eventsForDate(ds)
                    return (
                      <div
                        key={ds}
                        className={`${styles.weekDayCol} ${isToday ? styles.weekDayColToday : ''}`}
                        onClick={() => { if (!isReadOnly) { setSelectedDate(ds); setEditingEvent(null); setShowModal(true) } }}
                      >
                        {HOURS.map(h => (
                          <div key={h} className={styles.weekHourSlot} />
                        ))}
                        {dayEvents.map(evt => {
                          const [sh, sm] = evt.start_time.split(':').map(Number)
                          const [eh, em] = evt.end_time.split(':').map(Number)
                          const startMin = sh * 60 + sm
                          const endMin = eh * 60 + em
                          const duration = Math.max(endMin - startMin, 30)
                          const top = (startMin / (24 * 60)) * 100
                          const height = (duration / (24 * 60)) * 100
                          const cat = CATEGORIES[evt.category as EventCategory] || CATEGORIES.work
                          return (
                            <div
                              key={evt.id}
                              className={`${styles.weekEvent} ${evt.completed ? styles.weekEventDone : ''}`}
                              style={{ top: `${top}%`, height: `${height}%`, background: cat.bg, borderLeftColor: cat.color }}
                              onClick={(e) => { e.stopPropagation(); if (!isReadOnly) { setEditingEvent(evt); setShowModal(true) } }}
                            >
                              <span className={styles.weekEventTitle}>{evt.title}</span>
                              <span className={styles.weekEventTime}>
                                {formatTime12(evt.start_time)} - {formatTime12(evt.end_time)}
                              </span>
                            </div>
                          )
                        })}
                        {isToday && <NowLine />}
                      </div>
                    )
                  })}
                </div>
              </div>
            </div>
          )}

          {/* ── Day View ── */}
          {viewMode === 'day' && (
            <div className={styles.dayView}>
              <div className={styles.dayHeader}>
                <div className={styles.dayHeaderInfo}>
                  <span className={styles.dayHeaderDate}>
                    {parseDate(selectedDate).getDate()}
                  </span>
                  <div>
                    <span className={styles.dayHeaderDay}>
                      {DAYS[(parseDate(selectedDate).getDay() + 6) % 7]}day
                    </span>
                    <span className={styles.dayHeaderMonth}>
                      {MONTHS[parseDate(selectedDate).getMonth()]} {parseDate(selectedDate).getFullYear()}
                    </span>
                  </div>
                </div>
                <div className={styles.dayStats}>
                  <div className={styles.dayStat}>
                    <span className={styles.dayStatNum}>{selectedEvents.length}</span>
                    <span className={styles.dayStatLabel}>events</span>
                  </div>
                  <div className={styles.dayStat}>
                    <span className={styles.dayStatNum}>
                      {selectedEvents.reduce((sum, e) => {
                        const [sh, sm] = e.start_time.split(':').map(Number)
                        const [eh, em] = e.end_time.split(':').map(Number)
                        return sum + ((eh * 60 + em) - (sh * 60 + sm))
                      }, 0)}
                    </span>
                    <span className={styles.dayStatLabel}>min booked</span>
                  </div>
                </div>
              </div>
              <div className={styles.dayTimeline}>
                <div className={styles.dayTimeGrid}>
                  {HOURS.map(h => {
                    const tod = getTimeOfDay(h)
                    const TodIcon = tod.icon
                    const hourEvents = selectedEvents.filter(e => {
                      const [sh] = e.start_time.split(':').map(Number)
                      return sh === h
                    })
                    return (
                      <div
                        key={h}
                        className={`${styles.dayTimeRow} ${hourEvents.length > 0 ? styles.dayTimeRowActive : ''}`}
                        onClick={() => { if (!isReadOnly) { setEditingEvent(null); setShowModal(true) } }}
                      >
                        <div className={styles.dayTimeLabel}>
                          <span>{h === 0 ? '12 am' : h < 12 ? `${h} am` : h === 12 ? '12 pm' : `${h - 12} pm`}</span>
                          {(h === 6 || h === 12 || h === 17 || h === 21) && (
                            <TodIcon size={11} className={styles.dayTimeTodIcon} />
                          )}
                        </div>
                        <div className={styles.dayTimeContent}>
                          {hourEvents.map(evt => {
                            const cat = CATEGORIES[evt.category as EventCategory] || CATEGORIES.work
                            return (
                              <div
                                key={evt.id}
                                className={`${styles.dayEvent} ${evt.completed ? styles.dayEventDone : ''}`}
                                style={{ borderLeftColor: cat.color, background: cat.bg }}
                                onClick={(e) => { e.stopPropagation(); if (!isReadOnly) { setEditingEvent(evt); setShowModal(true) } }}
                              >
                                <div className={styles.dayEventHeader}>
                                  <span className={styles.dayEventTitle}>{evt.title}</span>
                                  {!isReadOnly && (
                                    <button
                                      className={styles.dayEventCheck}
                                      onClick={(e) => { e.stopPropagation(); toggleComplete(evt.id) }}
                                    >
                                      {evt.completed ? <Check size={13} /> : <Circle size={13} />}
                                    </button>
                                  )}
                                </div>
                                <div className={styles.dayEventMeta}>
                                  <Clock size={10} />
                                  {formatTime12(evt.start_time)} \u2013 {formatTime12(evt.end_time)}
                                  {evt.location && (
                                    <>
                                      <span className={styles.dayEventSep}>\u00b7</span>
                                      <MapPin size={10} />
                                      {evt.location}
                                    </>
                                  )}
                                </div>
                                {evt.notes && <p className={styles.dayEventNotes}>{evt.notes}</p>}
                              </div>
                            )
                          })}
                        </div>
                      </div>
                    )
                  })}
                  {selectedDate === today && <NowLine />}
                </div>
              </div>
            </div>
          )}
        </main>
      </div>

      {/* ── Event Modal ── */}
      {showModal && !isReadOnly && (
        <EventModal
          event={editingEvent}
          defaultDate={selectedDate}
          onSave={saveEvent}
          onDelete={deleteEvent}
          onClose={() => { setShowModal(false); setEditingEvent(null) }}
        />
      )}

      {/* ── Share Panel ── */}
      {showSharePanel && (
        <SharePanel
          onClose={() => setShowSharePanel(false)}
          onUpdated={loadSharedCalendars}
        />
      )}
    </div>
  )
}

// ── Now Line ──────────────────────────────────────────────────────

function NowLine() {
  const [pos, setPos] = useState(0)
  useEffect(() => {
    const update = () => {
      const now = new Date()
      setPos(((now.getHours() * 60 + now.getMinutes()) / (24 * 60)) * 100)
    }
    update()
    const interval = setInterval(update, 60000)
    return () => clearInterval(interval)
  }, [])
  return (
    <div className={styles.nowLine} style={{ top: `${pos}%` }}>
      <div className={styles.nowDot} />
      <div className={styles.nowBar} />
    </div>
  )
}

// ── Event Modal ───────────────────────────────────────────────────

interface EventModalProps {
  event: CalendarEvent | null
  defaultDate: string
  onSave: (data: {
    id?: string; title: string; date: string; start_time: string; end_time: string;
    category: string; location?: string; notes?: string; completed?: boolean
  }) => void
  onDelete: (id: string) => void
  onClose: () => void
}

function EventModal({ event, defaultDate, onSave, onDelete, onClose }: EventModalProps) {
  const isEdit = !!event
  const [title, setTitle] = useState(event?.title ?? '')
  const [date, setDate] = useState(event?.date ?? defaultDate)
  const [startTime, setStartTime] = useState(event?.start_time ?? '09:00')
  const [endTime, setEndTime] = useState(event?.end_time ?? '10:00')
  const [category, setCategory] = useState<EventCategory>(event?.category ?? 'work')
  const [location, setLocation] = useState(event?.location ?? '')
  const [notes, setNotes] = useState(event?.notes ?? '')
  const titleRef = useRef<HTMLInputElement>(null)

  useEffect(() => { titleRef.current?.focus() }, [])

  const handleSave = () => {
    if (!title.trim()) return
    onSave({
      id: event?.id,
      title: title.trim(),
      date,
      start_time: startTime,
      end_time: endTime,
      category,
      location: location.trim() || undefined,
      notes: notes.trim() || undefined,
      completed: event?.completed ?? false,
    })
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) handleSave()
    if (e.key === 'Escape') onClose()
  }

  return (
    <div className={styles.modalOverlay} onClick={onClose} onKeyDown={handleKeyDown}>
      <div className={styles.modal} onClick={e => e.stopPropagation()}>
        <div className={styles.modalHeader}>
          <div className={styles.modalHeaderLeft}>
            <div className={styles.modalIcon} style={{ background: CATEGORIES[category].bg, color: CATEGORIES[category].color }}>
              <CalendarIcon size={16} />
            </div>
            <span className={styles.modalLabel}>{isEdit ? 'Edit Event' : 'New Event'}</span>
          </div>
          <button className={styles.modalClose} onClick={onClose}><X size={16} /></button>
        </div>
        <div className={styles.modalBody}>
          <input
            ref={titleRef}
            className={styles.modalTitleInput}
            placeholder="Event title..."
            value={title}
            onChange={e => setTitle(e.target.value)}
            onKeyDown={handleKeyDown}
          />
          <div className={styles.modalRow}>
            <div className={styles.modalField}>
              <label className={styles.modalFieldLabel}><CalendarIcon size={11} /> Date</label>
              <input type="date" className={styles.modalInput} value={date} onChange={e => setDate(e.target.value)} />
            </div>
          </div>
          <div className={styles.modalRow}>
            <div className={styles.modalField}>
              <label className={styles.modalFieldLabel}><Clock size={11} /> Start</label>
              <input type="time" className={styles.modalInput} value={startTime} onChange={e => setStartTime(e.target.value)} />
            </div>
            <div className={styles.modalField}>
              <label className={styles.modalFieldLabel}><Clock size={11} /> End</label>
              <input type="time" className={styles.modalInput} value={endTime} onChange={e => setEndTime(e.target.value)} />
            </div>
          </div>
          <div className={styles.modalField}>
            <label className={styles.modalFieldLabel}><Tag size={11} /> Category</label>
            <div className={styles.categoryPicker}>
              {(Object.entries(CATEGORIES) as [EventCategory, typeof CATEGORIES[EventCategory]][]).map(([key, cat]) => (
                <button
                  key={key}
                  className={`${styles.categoryBtn} ${category === key ? styles.categoryBtnActive : ''}`}
                  style={{ '--cat-color': cat.color, '--cat-bg': cat.bg } as React.CSSProperties}
                  onClick={() => setCategory(key as EventCategory)}
                >
                  <span className={styles.categoryDot} style={{ background: cat.color }} />
                  {cat.label}
                </button>
              ))}
            </div>
          </div>
          <div className={styles.modalField}>
            <label className={styles.modalFieldLabel}><MapPin size={11} /> Location</label>
            <input className={styles.modalInput} placeholder="Optional" value={location} onChange={e => setLocation(e.target.value)} onKeyDown={handleKeyDown} />
          </div>
          <div className={styles.modalField}>
            <label className={styles.modalFieldLabel}><Edit3 size={11} /> Notes</label>
            <textarea className={`${styles.modalInput} ${styles.modalTextarea}`} placeholder="Add notes..." value={notes} onChange={e => setNotes(e.target.value)} rows={3} />
          </div>
        </div>
        <div className={styles.modalFooter}>
          {isEdit && (
            <button className={styles.deleteBtn} onClick={() => onDelete(event!.id)}>
              <Trash2 size={13} /> Delete
            </button>
          )}
          <div className={styles.modalFooterRight}>
            <button className={styles.cancelBtn} onClick={onClose}>Cancel</button>
            <button className={styles.saveBtn} onClick={handleSave} disabled={!title.trim()}>
              {isEdit ? 'Save Changes' : 'Create Event'}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

// ── Share Panel ───────────────────────────────────────────────────

interface SharePanelProps {
  onClose: () => void
  onUpdated: () => void
}

function SharePanel({ onClose, onUpdated }: SharePanelProps) {
  const [shares, setShares] = useState<CalendarShareResponse[]>([])
  const [email, setEmail] = useState('')
  const [permission, setPermission] = useState<SharePermission>('read')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    api.calendar.listShares()
      .then(setShares)
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [])

  const handleShare = async () => {
    if (!email.trim()) return
    setSaving(true)
    setError('')
    try {
      const res = await api.calendar.createShare(email.trim(), permission)
      setShares(prev => [...prev.filter(s => s.user.id !== res.user.id), res])
      setEmail('')
      onUpdated()
    } catch (err: any) {
      setError(err?.message?.includes('404') ? 'User not found' : err?.message || 'Failed to share')
    } finally {
      setSaving(false)
    }
  }

  const handleRemove = async (userId: string) => {
    try {
      await api.calendar.removeShare(userId)
      setShares(prev => prev.filter(s => s.user.id !== userId))
      onUpdated()
    } catch { /* ignore */ }
  }

  return (
    <div className={styles.modalOverlay} onClick={onClose}>
      <div className={styles.modal} onClick={e => e.stopPropagation()} style={{ maxWidth: 420 }}>
        <div className={styles.modalHeader}>
          <div className={styles.modalHeaderLeft}>
            <div className={styles.modalIcon} style={{ background: 'var(--accent-muted)', color: 'var(--accent)' }}>
              <Users size={16} />
            </div>
            <span className={styles.modalLabel}>Share Calendar</span>
          </div>
          <button className={styles.modalClose} onClick={onClose}><X size={16} /></button>
        </div>

        <div className={styles.modalBody}>
          <p className={styles.shareDesc}>
            Share your entire calendar with other users on this instance.
          </p>

          {/* Add share form */}
          <div className={styles.shareForm}>
            <input
              className={styles.modalInput}
              placeholder="User email..."
              value={email}
              onChange={e => { setEmail(e.target.value); setError('') }}
              onKeyDown={e => { if (e.key === 'Enter') handleShare() }}
            />
            <div className={styles.shareFormRow}>
              <div className={styles.permToggle}>
                <button
                  className={`${styles.permBtn} ${permission === 'read' ? styles.permBtnActive : ''}`}
                  onClick={() => setPermission('read')}
                >
                  <Eye size={12} /> View
                </button>
                <button
                  className={`${styles.permBtn} ${permission === 'write' ? styles.permBtnActive : ''}`}
                  onClick={() => setPermission('write')}
                >
                  <Pencil size={12} /> Edit
                </button>
              </div>
              <button className={styles.saveBtn} onClick={handleShare} disabled={!email.trim() || saving}>
                <UserPlus size={13} /> Share
              </button>
            </div>
          </div>

          {error && <div className={styles.shareError}>{error}</div>}

          {/* Current shares */}
          {loading ? (
            <div className={styles.upcomingEmpty}><Loader2 size={16} className="spinning" /></div>
          ) : shares.length === 0 ? (
            <div className={styles.shareEmpty}>Not shared with anyone yet</div>
          ) : (
            <div className={styles.shareList}>
              {shares.map(s => (
                <div key={s.user.id} className={styles.shareItem}>
                  <div className={styles.shareItemAvatar}>
                    {s.user.display_name.charAt(0).toUpperCase()}
                  </div>
                  <div className={styles.shareItemInfo}>
                    <span className={styles.shareItemName}>{s.user.display_name}</span>
                    <span className={styles.shareItemEmail}>{s.user.email}</span>
                  </div>
                  <span className={styles.shareItemPerm}>
                    {s.share.permission === 'write' ? <Pencil size={10} /> : <Eye size={10} />}
                    {s.share.permission}
                  </span>
                  <button className={styles.shareItemRemove} onClick={() => handleRemove(s.user.id)}>
                    <X size={13} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
