import { useCallback, useEffect, useMemo, useState } from 'react'
import type { TimelineGroup, TimelineResponse, UnifiedMemoryItem, UnifiedSearchResult } from '../types'
import { getTimeline, unifiedSearch } from '../lib/tauri'

type GroupBy = 'day' | 'week' | 'month'
type SourceFilter = 'records' | 'memory'

const SOURCE_COLORS: Record<string, string> = {
  record: '#3b82f6',
  memory: '#10b981',
}

const SOURCE_LABELS: Record<string, string> = {
  record: 'Record',
  memory: 'Memory',
}

export default function TimelineTab({
  centralHome,
  t,
}: {
  centralHome: string
  t: (key: string, fallback?: string) => string
}) {
  const [groupBy, setGroupBy] = useState<GroupBy>('day')
  const [timeline, setTimeline] = useState<TimelineResponse | null>(null)
  const [searchResults, setSearchResults] = useState<UnifiedSearchResult | null>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [debouncedQuery, setDebouncedQuery] = useState('')
  const [selectedItem, setSelectedItem] = useState<UnifiedMemoryItem | null>(null)
  const [sourceFilters, setSourceFilters] = useState<SourceFilter[]>(['records', 'memory'])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Debounce search query
  useEffect(() => {
    const timer = setTimeout(() => setDebouncedQuery(searchQuery), 300)
    return () => clearTimeout(timer)
  }, [searchQuery])

  // Load timeline
  const loadTimeline = useCallback(async () => {
    if (!centralHome) return
    setLoading(true)
    setError(null)
    try {
      const result = await getTimeline({
        centralHome,
        groupBy,
        sources: sourceFilters,
        limit: 30,
      })
      setTimeline(result)
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }, [centralHome, groupBy, sourceFilters])

  // Search
  const doSearch = useCallback(async () => {
    if (!centralHome || !debouncedQuery.trim()) {
      setSearchResults(null)
      return
    }
    setLoading(true)
    setError(null)
    try {
      const result = await unifiedSearch({
        centralHome,
        query: debouncedQuery.trim(),
        sources: sourceFilters,
        limit: 50,
      })
      setSearchResults(result)
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }, [centralHome, debouncedQuery, sourceFilters])

  useEffect(() => {
    if (debouncedQuery.trim()) {
      void doSearch()
    } else {
      setSearchResults(null)
      void loadTimeline()
    }
  }, [debouncedQuery, doSearch, loadTimeline])

  const toggleSource = (src: SourceFilter) => {
    setSourceFilters((prev) => {
      if (prev.includes(src)) {
        const next = prev.filter((s) => s !== src)
        return next.length > 0 ? next : prev // Don't allow empty
      }
      return [...prev, src]
    })
  }

  const isSearchMode = !!debouncedQuery.trim() && searchResults !== null

  // Items to display
  const displayGroups: TimelineGroup[] = useMemo(() => {
    if (isSearchMode && searchResults) {
      // Group search results by day for display
      const grouped: Record<string, UnifiedMemoryItem[]> = {}
      for (const item of searchResults.items) {
        const day = item.createdAt.slice(0, 10)
        if (!grouped[day]) grouped[day] = []
        grouped[day].push(item)
      }
      return Object.entries(grouped)
        .sort(([a], [b]) => b.localeCompare(a))
        .map(([date, items]) => ({
          label: date,
          date,
          items,
          count: items.length,
          sourceCounts: {},
        }))
    }
    return timeline?.groups ?? []
  }, [isSearchMode, searchResults, timeline])

  if (!centralHome) {
    return (
      <div className="timeline-empty">
        <p>{t('timeline.noCentralHome', 'Please select a Central Home first.')}</p>
      </div>
    )
  }

  return (
    <div className="timeline-root">
      {/* Search + Controls */}
      <div className="timeline-controls">
        <input
          className="timeline-search"
          type="text"
          placeholder={t('timeline.searchPlaceholder', 'Search across records & memory...')}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
        />
        <div className="timeline-filters">
          <div className="timeline-source-toggles">
            {(['records', 'memory'] as SourceFilter[]).map((src) => (
              <button
                key={src}
                type="button"
                className={`source-toggle ${sourceFilters.includes(src) ? 'active' : ''}`}
                style={{
                  borderColor: SOURCE_COLORS[src === 'records' ? 'record' : 'memory'],
                  backgroundColor: sourceFilters.includes(src)
                    ? SOURCE_COLORS[src === 'records' ? 'record' : 'memory'] + '22'
                    : 'transparent',
                }}
                onClick={() => toggleSource(src)}
              >
                {SOURCE_LABELS[src === 'records' ? 'record' : 'memory']}
              </button>
            ))}
          </div>
          {!isSearchMode && (
            <div className="timeline-group-btns">
              {(['day', 'week', 'month'] as GroupBy[]).map((g) => (
                <button
                  key={g}
                  type="button"
                  className={`group-btn ${groupBy === g ? 'active' : ''}`}
                  onClick={() => setGroupBy(g)}
                >
                  {t(`timeline.${g}`, g.charAt(0).toUpperCase() + g.slice(1))}
                </button>
              ))}
            </div>
          )}
        </div>
        {isSearchMode && searchResults && (
          <div className="timeline-search-meta">
            {t('timeline.searchResults', 'Results')}: {searchResults.total}
            {' · '}
            {searchResults.tookMs}ms
            {Object.entries(searchResults.sourceCounts).map(([src, count]) => (
              <span key={src} className="source-count-badge" style={{ color: SOURCE_COLORS[src] || '#888' }}>
                {' '}{src}: {count}
              </span>
            ))}
          </div>
        )}
      </div>

      {error && <div className="timeline-error">{error}</div>}
      {loading && <div className="timeline-loading">{t('timeline.loading', 'Loading...')}</div>}

      {/* Timeline Content */}
      <div className="timeline-content">
        <div className="timeline-groups">
          {displayGroups.length === 0 && !loading && (
            <div className="timeline-empty">
              <p>{isSearchMode
                ? t('timeline.noResults', 'No results found.')
                : t('timeline.noData', 'No data yet. Records and memory will appear here.')
              }</p>
            </div>
          )}
          {displayGroups.map((group) => (
            <div key={group.label} className="timeline-group">
              <div className="timeline-group-header">
                <span className="timeline-group-date">{group.label}</span>
                <span className="timeline-group-count">{group.count} items</span>
              </div>
              <div className="timeline-group-items">
                {group.items.map((item, idx) => (
                  <button
                    key={`${item.id}-${idx}`}
                    type="button"
                    className={`timeline-item ${selectedItem?.id === item.id ? 'selected' : ''}`}
                    onClick={() => setSelectedItem(item)}
                  >
                    <span
                      className="source-badge"
                      style={{ backgroundColor: SOURCE_COLORS[item.source] || '#888' }}
                    >
                      {item.sourceType}
                    </span>
                    <span className="timeline-item-title">{item.title}</span>
                    <span className="timeline-item-time">
                      {item.createdAt.slice(11, 16) || ''}
                    </span>
                    {item.tags.length > 0 && (
                      <span className="timeline-item-tags">
                        {item.tags.slice(0, 3).map((tag) => (
                          <span key={tag} className="timeline-tag">
                            {tag}
                          </span>
                        ))}
                      </span>
                    )}
                    <span
                      className="timeline-item-snippet"
                      dangerouslySetInnerHTML={{ __html: item.snippet }}
                    />
                  </button>
                ))}
              </div>
            </div>
          ))}
        </div>

        {/* Detail Panel */}
        {selectedItem && (
          <div className="timeline-detail">
            <div className="timeline-detail-header">
              <span
                className="source-badge"
                style={{ backgroundColor: SOURCE_COLORS[selectedItem.source] || '#888' }}
              >
                {selectedItem.sourceType}
              </span>
              <h3>{selectedItem.title}</h3>
              <button
                type="button"
                className="ghost-btn"
                onClick={() => setSelectedItem(null)}
              >
                ✕
              </button>
            </div>
            <div className="timeline-detail-meta">
              <span>{selectedItem.createdAt}</span>
              {selectedItem.tags.length > 0 && (
                <span className="timeline-detail-tags">
                  {selectedItem.tags.map((tag) => (
                    <span key={tag} className="timeline-tag">{tag}</span>
                  ))}
                </span>
              )}
            </div>
            <div className="timeline-detail-body">
              <pre>{selectedItem.body}</pre>
            </div>
          </div>
        )}
      </div>

      {/* Stats */}
      {!isSearchMode && timeline && (
        <div className="timeline-footer">
          {t('timeline.total', 'Total')}: {timeline.totalItems} items · {timeline.totalGroups} groups
        </div>
      )}
    </div>
  )
}
