import { useEffect, useMemo, useState } from 'react'
import { listLogs } from '../lib/tauri'
import type { LogEntry } from '../types'

type LogPulse = {
  date: string
  count: number
  failed: number
}

function statusTone(status: string): 'ok' | 'warn' | 'error' {
  const normalized = status.toLowerCase()
  if (normalized.includes('fail') || normalized.includes('error')) return 'error'
  if (normalized.includes('warn') || normalized.includes('pending')) return 'warn'
  return 'ok'
}

function buildLogPulse(logs: LogEntry[], days = 10): LogPulse[] {
  const buckets = new Map<string, LogPulse>()
  for (const item of logs) {
    if (!item.timestamp) continue
    const date = item.timestamp.slice(0, 10)
    const existing = buckets.get(date) ?? { date, count: 0, failed: 0 }
    existing.count += 1
    if (statusTone(item.status || '') === 'error') existing.failed += 1
    buckets.set(date, existing)
  }
  return [...buckets.values()]
    .sort((a, b) => a.date.localeCompare(b.date))
    .slice(-days)
}

export default function LogsTab({
  centralHome,
  t,
}: {
  centralHome: string
  t: (key: string, fallback?: string) => string
}) {
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [selectedLogIndex, setSelectedLogIndex] = useState<number>(-1)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (!centralHome) {
      setLogs([])
      setSelectedLogIndex(-1)
      setLoading(false)
      return
    }
    setLoading(true)
    listLogs(centralHome)
      .then((list) => {
        const arr = Array.isArray(list) ? list : []
        setLogs(arr)
        setSelectedLogIndex(arr.length > 0 ? 0 : -1)
      })
      .catch(() => {
        setLogs([])
        setSelectedLogIndex(-1)
      })
      .finally(() => setLoading(false))
  }, [centralHome])

  const selectedLog = useMemo(
    () => (selectedLogIndex >= 0 && selectedLogIndex < logs.length ? logs[selectedLogIndex] : null),
    [selectedLogIndex, logs],
  )

  const failedCount = logs.filter((item) => (item.status || '').toLowerCase().includes('fail')).length
  const successCount = logs.filter((item) => {
    const status = (item.status || '').toLowerCase()
    return status.includes('success') || status.includes('done')
  }).length
  const pulse = useMemo(() => buildLogPulse(logs), [logs])
  const pulseMax = Math.max(1, ...pulse.map((item) => item.count))

  if (loading) {
    return <div className="panel">{t('Loading...', '載入中…')}</div>
  }

  return (
    <div className="page-stack">
      <div className="panel section-hero logs-hero">
        <div className="section-hero-content">
          <p className="eyebrow">{t('logs.hero.eyebrow')}</p>
          <h3>{t('logs.hero.title')}</h3>
          <p className="muted">{t('logs.hero.desc')}</p>
        </div>
        <div className="section-hero-stats">
          <div className="hero-metric">
            <span>{t('logs.hero.total')}</span>
            <strong>{logs.length}</strong>
            <small>{t('logs.hero.eventsCaptured')}</small>
          </div>
          <div className="hero-metric">
            <span>{t('logs.hero.failures')}</span>
            <strong>{failedCount}</strong>
            <small>{t('logs.hero.needsAttention')}</small>
          </div>
          <div className="hero-metric">
            <span>{t('logs.hero.success')}</span>
            <strong>{successCount}</strong>
            <small>{t('logs.hero.healthyExecutions')}</small>
          </div>
        </div>
      </div>

      <div className="panel panel-strong logs-pulse-panel">
        <div className="panel-head-inline">
          <h3>{t('logs.pulse.title')}</h3>
          <span className="muted">{t('logs.pulse.subtitle')}</span>
        </div>
        {pulse.length === 0 ? (
          <p className="muted">{t('logs.pulse.empty')}</p>
        ) : (
          <div className="logs-pulse-grid">
            {pulse.map((item) => {
              const totalHeight = Math.max(8, Math.round((item.count / pulseMax) * 100))
              const failHeight = item.count === 0 ? 0 : Math.round((item.failed / item.count) * totalHeight)
              return (
                <div key={item.date} className="pulse-col">
                  <div className="pulse-bar-wrap">
                    <div className="pulse-bar-total" style={{ height: `${totalHeight}%` }}>
                      {failHeight > 0 && <div className="pulse-bar-failed" style={{ height: `${failHeight}%` }} />}
                    </div>
                  </div>
                  <div className="pulse-meta">
                    <strong>{item.count}</strong>
                    <span>{item.date.slice(5)}</span>
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </div>

      <div className="logs-layout">
        <div className="panel left-panel logs-stream-panel">
          <div className="panel-head-inline">
            <h3>{t('logs.stream.title')}</h3>
            <span className="muted">{t('logs.stream.subtitle')}</span>
          </div>
          <div className="record-list">
            {logs.map((item, index) => {
              const tone = statusTone(item.status || '')
              const toneLabel =
                tone === 'error' ? t('logs.badge.error') : tone === 'warn' ? t('logs.badge.warn') : t('logs.badge.ok')
              return (
                <button
                  type="button"
                  key={item.jsonPath ?? `${item.timestamp}-${item.eventId}-${index}`}
                  className={
                    selectedLogIndex === index
                      ? `record-item selected log-item selected tone-${tone}`
                      : `record-item log-item tone-${tone}`
                  }
                  onClick={() => setSelectedLogIndex(index)}
                >
                  <p>{item.timestamp.slice(0, 19)}</p>
                  <p>
                    <strong>{item.taskIntent || '-'}</strong> | {item.status || '-'}
                  </p>
                  <p>{item.title || t('logs.detail.noTitle')}</p>
                  <span className={`log-tone-pill ${tone}`}>{toneLabel}</span>
                </button>
              )
            })}
            {logs.length === 0 && <p className="muted">{t('common.noLogs')}</p>}
          </div>
        </div>

        <div className="panel right-panel log-detail-panel">
          <div className="panel-head-inline">
            <h3>{t('logs.detail.title')}</h3>
            <span className="muted">{selectedLog?.eventId || t('common.selectLog')}</span>
          </div>
          <ul className="simple-list log-meta-list">
            <li>
              <span>{t('logs.detail.eventId')}</span>
              <strong>{selectedLog?.eventId || '-'}</strong>
            </li>
            <li>
              <span>{t('logs.detail.task')}</span>
              <strong>{selectedLog?.taskIntent || '-'}</strong>
            </li>
            <li>
              <span>{t('logs.detail.status')}</span>
              <strong>{selectedLog?.status || '-'}</strong>
            </li>
            <li>
              <span>{t('logs.detail.timestamp')}</span>
              <strong>{selectedLog?.timestamp || '-'}</strong>
            </li>
            <li>
              <span>{t('logs.detail.titleLabel')}</span>
              <strong>{selectedLog?.title || t('logs.detail.noTitle')}</strong>
            </li>
          </ul>
          <h4 className="sub-title">{t('logs.detail.payload')}</h4>
          <pre className="json-preview">{selectedLog ? JSON.stringify(selectedLog.raw, null, 2) : '{}'}</pre>
        </div>
      </div>
    </div>
  )
}
