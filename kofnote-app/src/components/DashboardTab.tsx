import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  forceCenter,
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  type Simulation,
  type SimulationLinkDatum,
  type SimulationNodeDatum,
} from 'd3-force'
import { getDashboardStats, listRecords } from '../lib/tauri'
import { DASHBOARD_GRAPH_HEIGHT, DASHBOARD_GRAPH_WIDTH, RECORD_TYPES, TYPE_COLORS } from '../constants'
import type { DashboardStats, RecordItem, RecordType } from '../types'

type DashboardGraphNodeKind = 'core' | 'type' | 'tag' | 'record'

type DashboardGraphNode = SimulationNodeDatum & {
  id: string
  kind: DashboardGraphNodeKind
  label: string
  color: string
  radius: number
  count: number
  recordType?: RecordType
  tag?: string
  jsonPath?: string
}

type DashboardGraphLink = SimulationLinkDatum<DashboardGraphNode> & {
  id: string
  source: string | DashboardGraphNode
  target: string | DashboardGraphNode
  relation: 'core-type' | 'type-record' | 'record-tag'
  weight: number
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value))
}

function linkNodeId(endpoint: string | DashboardGraphNode): string {
  return typeof endpoint === 'string' ? endpoint : endpoint.id
}

function shortenLabel(value: string, max = 26): string {
  if (value.length <= max) return value
  return `${value.slice(0, Math.max(8, max - 1)).trim()}…`
}

function createDashboardGraph(
  records: RecordItem[],
  stats: DashboardStats | null,
): { nodes: DashboardGraphNode[]; links: DashboardGraphLink[] } {
  const nodes: DashboardGraphNode[] = []
  const links: DashboardGraphLink[] = []
  const tagNodeMap = new Map<string, DashboardGraphNode>()

  const coreNode: DashboardGraphNode = {
    id: 'core:knowledge',
    kind: 'core',
    label: 'KOF CORE',
    color: '#7f8dff',
    radius: 26,
    count: records.length,
  }
  nodes.push(coreNode)

  for (const type of RECORD_TYPES) {
    const count = stats?.typeCounts[type] ?? records.filter((r) => r.recordType === type).length
    const typeNode: DashboardGraphNode = {
      id: `type:${type}`,
      kind: 'type',
      label: type,
      color: TYPE_COLORS[type],
      radius: count === 0 ? 10 : clamp(12 + count, 12, 22),
      count,
      recordType: type,
    }
    nodes.push(typeNode)
    links.push({
      id: `core-${type}`,
      source: coreNode.id,
      target: typeNode.id,
      relation: 'core-type',
      weight: Math.max(1, count),
    })
  }

  const sortedRecords = [...records].sort((a, b) => b.createdAt.localeCompare(a.createdAt))
  const recordNodes = sortedRecords.slice(0, 18).map((record) => {
    const tagCount = record.tags.length
    const node: DashboardGraphNode = {
      id: `record:${record.jsonPath ?? `${record.createdAt}-${record.title}`}`,
      kind: 'record',
      label: shortenLabel(record.title || record.recordType, 24),
      color: `${TYPE_COLORS[record.recordType]}cc`,
      radius: clamp(9 + tagCount, 9, 15),
      count: tagCount,
      recordType: record.recordType,
      jsonPath: record.jsonPath ?? undefined,
    }
    return { node, record }
  })

  for (const entry of recordNodes) {
    nodes.push(entry.node)
    links.push({
      id: `${entry.node.id}->type:${entry.record.recordType}`,
      source: `type:${entry.record.recordType}`,
      target: entry.node.id,
      relation: 'type-record',
      weight: Math.max(1, entry.record.tags.length),
    })
  }

  const tagCounts = new Map<string, number>()
  for (const record of records) {
    for (const tag of record.tags) {
      const cleanTag = tag.trim()
      if (!cleanTag) continue
      tagCounts.set(cleanTag, (tagCounts.get(cleanTag) ?? 0) + 1)
    }
  }
  const topTags = [...tagCounts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 18)

  for (const [tag, count] of topTags) {
    const tagNode: DashboardGraphNode = {
      id: `tag:${tag}`,
      kind: 'tag',
      label: shortenLabel(tag, 16),
      color: '#f266c4',
      radius: clamp(8 + count * 0.9, 10, 17),
      count,
      tag,
    }
    tagNodeMap.set(tag, tagNode)
    nodes.push(tagNode)
    links.push({
      id: `core-tag:${tag}`,
      source: coreNode.id,
      target: tagNode.id,
      relation: 'core-type',
      weight: Math.max(1, count),
    })
  }

  for (const entry of recordNodes) {
    const linkedTags = entry.record.tags.map((t) => t.trim()).filter(Boolean).slice(0, 4)
    for (const tag of linkedTags) {
      const tagNode = tagNodeMap.get(tag)
      if (!tagNode) continue
      links.push({
        id: `${entry.node.id}->${tagNode.id}`,
        source: entry.node.id,
        target: tagNode.id,
        relation: 'record-tag',
        weight: Math.max(1, tagNode.count),
      })
    }
  }

  return { nodes, links }
}

export type DashboardTabNavigateOpts = {
  type?: RecordType
  keyword?: string
  jsonPath?: string
}

export default function DashboardTab({
  centralHome,
  t,
  onNavigateToRecords,
}: {
  centralHome: string
  t: (key: string, fallback?: string) => string
  onNavigateToRecords: (opts: DashboardTabNavigateOpts) => void
}) {
  const [stats, setStats] = useState<DashboardStats | null>(null)
  const [allRecords, setAllRecords] = useState<RecordItem[]>([])
  const [loading, setLoading] = useState(true)
  const [graphNodes, setGraphNodes] = useState<DashboardGraphNode[]>([])
  const [graphLinks, setGraphLinks] = useState<DashboardGraphLink[]>([])
  const [focusedNodeId, setFocusedNodeId] = useState<string | null>(null)

  const svgRef = useRef<SVGSVGElement | null>(null)
  const simulationRef = useRef<Simulation<DashboardGraphNode, DashboardGraphLink> | null>(null)
  const draggingRef = useRef<string | null>(null)

  useEffect(() => {
    if (!centralHome) {
      setStats(null)
      setAllRecords([])
      setLoading(false)
      return
    }
    setLoading(true)
    Promise.all([getDashboardStats(centralHome), listRecords(centralHome)])
      .then(([s, list]) => {
        setStats(s)
        setAllRecords(Array.isArray(list) ? list : [])
      })
      .catch(() => {
        setStats(null)
        setAllRecords([])
      })
      .finally(() => setLoading(false))
  }, [centralHome])

  const graphModel = useMemo(
    () => createDashboardGraph(allRecords, stats),
    [allRecords, stats],
  )

  useEffect(() => {
    if (graphModel.nodes.length === 0) {
      setGraphNodes([])
      setGraphLinks([])
      setFocusedNodeId(null)
      simulationRef.current?.stop()
      simulationRef.current = null
      return
    }

    const seededNodes = graphModel.nodes.map((node, index) => {
      const angle = (Math.PI * 2 * index) / Math.max(1, graphModel.nodes.length)
      return {
        ...node,
        x:
          node.x ??
          DASHBOARD_GRAPH_WIDTH / 2 +
            Math.cos(angle) * (node.kind === 'core' ? 0 : node.kind === 'type' ? 170 : node.kind === 'tag' ? 218 : 132),
        y:
          node.y ??
          DASHBOARD_GRAPH_HEIGHT / 2 +
            Math.sin(angle) * (node.kind === 'core' ? 0 : node.kind === 'type' ? 162 : node.kind === 'tag' ? 210 : 124),
      }
    })
    const seededLinks = graphModel.links.map((link) => ({ ...link }))

    const simulation = forceSimulation<DashboardGraphNode>(seededNodes)
      .force(
        'link',
        forceLink<DashboardGraphNode, DashboardGraphLink>(seededLinks)
          .id((node) => node.id)
          .distance((link) => {
            if (link.relation === 'core-type') return 110
            if (link.relation === 'type-record') return 88
            return 72
          })
          .strength((link) => (link.relation === 'record-tag' ? 0.23 : 0.34)),
      )
      .force(
        'charge',
        forceManyBody<DashboardGraphNode>().strength((node) => {
          if (node.kind === 'core') return -560
          if (node.kind === 'type') return -310
          if (node.kind === 'tag') return -180
          return -140
        }),
      )
      .force('center', forceCenter(DASHBOARD_GRAPH_WIDTH / 2, DASHBOARD_GRAPH_HEIGHT / 2))
      .force(
        'collide',
        forceCollide<DashboardGraphNode>().radius((node) => node.radius + 8).strength(0.86),
      )
      .alpha(1)
      .alphaDecay(0.035)

    simulationRef.current?.stop()
    simulationRef.current = simulation

    let frameId = 0
    const publish = () => {
      if (frameId) return
      frameId = window.requestAnimationFrame(() => {
        frameId = 0
        setGraphNodes([...seededNodes])
        setGraphLinks([...seededLinks])
      })
    }
    simulation.on('tick', publish)
    publish()

    return () => {
      if (frameId) window.cancelAnimationFrame(frameId)
      simulation.stop()
      if (simulationRef.current === simulation) simulationRef.current = null
    }
  }, [graphModel])

  const resolvedLinks = useMemo(
    () =>
      graphLinks.map((link) => ({
        id: link.id,
        sourceId: linkNodeId(link.source),
        targetId: linkNodeId(link.target),
        relation: link.relation,
      })),
    [graphLinks],
  )

  const focusedNode = useMemo(
    () => graphNodes.find((n) => n.id === focusedNodeId) ?? null,
    [focusedNodeId, graphNodes],
  )

  const focusNeighbors = useMemo(() => {
    if (!focusedNodeId) return new Set<string>()
    const related = new Set<string>([focusedNodeId])
    for (const link of resolvedLinks) {
      if (link.sourceId === focusedNodeId || link.targetId === focusedNodeId) {
        related.add(link.sourceId)
        related.add(link.targetId)
      }
    }
    return related
  }, [focusedNodeId, resolvedLinks])

  useEffect(() => {
    if (!focusedNodeId || graphNodes.some((n) => n.id === focusedNodeId)) return
    setFocusedNodeId(null)
  }, [focusedNodeId, graphNodes])

  const toGraphPoint = useCallback((event: React.PointerEvent<SVGSVGElement>) => {
    const svg = svgRef.current
    if (!svg) return null
    const ctm = svg.getScreenCTM()
    if (!ctm) return null
    const point = svg.createSVGPoint()
    point.x = event.clientX
    point.y = event.clientY
    const gp = point.matrixTransform(ctm.inverse())
    return {
      x: clamp(gp.x, 16, DASHBOARD_GRAPH_WIDTH - 16),
      y: clamp(gp.y, 16, DASHBOARD_GRAPH_HEIGHT - 16),
    }
  }, [])

  const handleNodeActivate = useCallback(
    (node: DashboardGraphNode) => {
      setFocusedNodeId(node.id)
      if (node.kind === 'type' && node.recordType) {
        onNavigateToRecords({ type: node.recordType })
        return
      }
      if (node.kind === 'tag' && node.tag) {
        onNavigateToRecords({ keyword: node.tag })
        return
      }
      if (node.kind === 'record' && node.jsonPath) {
        onNavigateToRecords({ jsonPath: node.jsonPath })
      }
    },
    [onNavigateToRecords],
  )

  const handlePointerDown = useCallback((nodeId: string, event: React.PointerEvent<SVGGElement>) => {
    event.stopPropagation()
    draggingRef.current = nodeId
    event.currentTarget.setPointerCapture?.(event.pointerId)
    const sim = simulationRef.current
    if (!sim) return
    const node = sim.nodes().find((n) => n.id === nodeId)
    if (!node) return
    node.fx = node.x ?? DASHBOARD_GRAPH_WIDTH / 2
    node.fy = node.y ?? DASHBOARD_GRAPH_HEIGHT / 2
    sim.alphaTarget(0.28).restart()
  }, [])

  const releaseDrag = useCallback(() => {
    const id = draggingRef.current
    if (!id) return
    const sim = simulationRef.current
    if (sim) {
      const node = sim.nodes().find((n) => n.id === id)
      if (node) {
        node.fx = null
        node.fy = null
      }
      sim.alphaTarget(0)
    }
    draggingRef.current = null
  }, [])

  const handlePointerMove = useCallback(
    (event: React.PointerEvent<SVGSVGElement>) => {
      const id = draggingRef.current
      if (!id) return
      const point = toGraphPoint(event)
      const sim = simulationRef.current
      if (!point || !sim) return
      const node = sim.nodes().find((n) => n.id === id)
      if (!node) return
      node.fx = point.x
      node.fy = point.y
      sim.alphaTarget(0.28).restart()
    },
    [toGraphPoint],
  )

  if (loading) {
    return <div className="panel dashboard-empty">{t('Loading...', '載入中…')}</div>
  }

  if (!stats) {
    return (
      <div className="panel dashboard-empty">
        {t('Load a Central Home to see metrics.', '請先載入中央記錄路徑以查看儀表板。')}
      </div>
    )
  }

  const maxTypeCount = Math.max(1, ...RECORD_TYPES.map((item) => stats.typeCounts[item] ?? 0))
  const maxDailyCount = Math.max(1, ...stats.recentDailyCounts.map((row) => row.count))
  const dominantType = RECORD_TYPES.reduce<{ type: RecordType; count: number }>(
    (acc, item) => {
      const count = stats.typeCounts[item] ?? 0
      return count > acc.count ? { type: item, count } : acc
    },
    { type: RECORD_TYPES[0], count: stats.typeCounts[RECORD_TYPES[0]] ?? 0 },
  )
  const hottestDay = stats.recentDailyCounts.reduce<{ date: string; count: number }>(
    (acc, row) => (row.count > acc.count ? row : acc),
    stats.recentDailyCounts[0] ?? { date: '-', count: 0 },
  )
  const syncHealthPercent =
    stats.totalRecords === 0
      ? 100
      : Math.max(0, Math.round(((stats.totalRecords - stats.pendingSyncCount) / stats.totalRecords) * 100))

  const focusedLinkNodeIds = focusedNodeId
    ? resolvedLinks.reduce<string[]>((acc, link) => {
        if (link.sourceId === focusedNodeId) acc.push(link.targetId)
        else if (link.targetId === focusedNodeId) acc.push(link.sourceId)
        return acc
      }, [])
    : []
  const focusedLinkedNodes = [...new Set(focusedLinkNodeIds)]
    .map((id) => graphNodes.find((n) => n.id === id))
    .filter((n): n is DashboardGraphNode => Boolean(n))
    .slice(0, 8)

  const focusedRecordCount = (() => {
    if (!focusedNode) return allRecords.length
    if (focusedNode.kind === 'core') return allRecords.length
    if (focusedNode.kind === 'type' && focusedNode.recordType) {
      return allRecords.filter((r) => r.recordType === focusedNode.recordType).length
    }
    if (focusedNode.kind === 'tag' && focusedNode.tag) {
      return allRecords.filter((r) => r.tags.includes(focusedNode.tag || '')).length
    }
    if (focusedNode.kind === 'record') return 1
    return 0
  })()

  return (
    <div className="dashboard-grid">
      <div className="panel dashboard-hero">
        <div className="dashboard-hero-content">
          <p className="eyebrow">{t('Knowledge Pulse', '知識脈動')}</p>
          <h3>{t('Central Log Control Tower', 'Central Log 控制塔')}</h3>
          <p className="muted">
            {t(
              'Monitor knowledge volume, sync health, and recent activity rhythm in real time.',
              '即時掌握筆記累積、同步健康度與近 7 天活躍節奏。',
            )}
          </p>
        </div>
        <div className="hero-metrics">
          <div className="hero-metric">
            <span>{t('Dominant Type', '主要類型')}</span>
            <strong>{dominantType.type}</strong>
            <small>{t(`${dominantType.count} records`, `${dominantType.count} 筆`)}</small>
          </div>
          <div className="hero-metric">
            <span>{t('Hottest Day', '最高活躍日')}</span>
            <strong>{hottestDay.date}</strong>
            <small>{t(`${hottestDay.count} events`, `${hottestDay.count} 次事件`)}</small>
          </div>
          <div className="hero-metric">
            <span>{t('Top Concept', '熱門概念')}</span>
            <strong>{stats.topTags[0]?.tag ?? t('none', '無')}</strong>
            <small>{t(`${stats.topTags[0]?.count ?? 0} mentions`, `${stats.topTags[0]?.count ?? 0} 次提及`)}</small>
          </div>
        </div>
      </div>

      <div className="kpi-row">
        <div className="kpi-card records">
          <p className="kpi-label">{t('Records', '紀錄')}</p>
          <p className="kpi-sub">{t('Total knowledge assets', '知識資產總數')}</p>
          <p className="kpi-value">{stats.totalRecords}</p>
        </div>
        <div className="kpi-card logs">
          <p className="kpi-label">{t('Logs', '日誌')}</p>
          <p className="kpi-sub">{t('Captured process traces', '流程追蹤紀錄')}</p>
          <p className="kpi-value">{stats.totalLogs}</p>
        </div>
        <div className="kpi-card sync">
          <p className="kpi-label">{t('Sync Health', '同步健康度')}</p>
          <p className="kpi-sub">{t(`${stats.pendingSyncCount} pending sync`, `${stats.pendingSyncCount} 筆待同步`)}</p>
          <p className="kpi-value">{syncHealthPercent}%</p>
          <p className="kpi-trend">{t('ready state', '可用狀態')}</p>
        </div>
      </div>

      <div className="panel panel-strong dashboard-force-panel">
        <div className="panel-head-inline">
          <h3>{t('Interactive Knowledge Graph', '互動知識圖譜')}</h3>
          <span className="muted">{t('Drag · Focus · Click to filter Records', '拖曳 · 聚焦 · 點擊即可篩選紀錄')}</span>
        </div>
        <div className="dashboard-force-layout">
          <div className="dashboard-force-canvas">
            <svg
              ref={svgRef}
              className="dashboard-force-svg"
              viewBox={`0 0 ${DASHBOARD_GRAPH_WIDTH} ${DASHBOARD_GRAPH_HEIGHT}`}
              role="img"
              aria-label={t('Knowledge graph visualization', '知識圖譜視覺化')}
              onPointerMove={handlePointerMove}
              onPointerUp={releaseDrag}
              onPointerLeave={releaseDrag}
              onPointerCancel={releaseDrag}
              onClick={() => setFocusedNodeId(null)}
            >
              <defs>
                <radialGradient id="graphGlow" cx="50%" cy="50%" r="60%">
                  <stop offset="0%" stopColor="rgba(98, 222, 255, 0.34)" />
                  <stop offset="100%" stopColor="rgba(12, 19, 40, 0)" />
                </radialGradient>
              </defs>
              <rect x={0} y={0} width={DASHBOARD_GRAPH_WIDTH} height={DASHBOARD_GRAPH_HEIGHT} fill="url(#graphGlow)" />
              {resolvedLinks.map((link) => {
                const sourceNode = graphNodes.find((n) => n.id === link.sourceId)
                const targetNode = graphNodes.find((n) => n.id === link.targetId)
                if (!sourceNode || !targetNode) return null
                const active =
                  !focusedNodeId || link.sourceId === focusedNodeId || link.targetId === focusedNodeId
                return (
                  <line
                    key={link.id}
                    className={`graph-link ${link.relation} ${active ? 'active' : 'dim'}`}
                    x1={sourceNode.x ?? 0}
                    y1={sourceNode.y ?? 0}
                    x2={targetNode.x ?? 0}
                    y2={targetNode.y ?? 0}
                  />
                )
              })}
              {graphNodes.map((node) => {
                const x = node.x ?? DASHBOARD_GRAPH_WIDTH / 2
                const y = node.y ?? DASHBOARD_GRAPH_HEIGHT / 2
                const focused = focusedNodeId === node.id
                const dimmed = Boolean(focusedNodeId) && !focusNeighbors.has(node.id)
                return (
                  <g
                    key={node.id}
                    className={`graph-node kind-${node.kind}${focused ? ' focused' : ''}${dimmed ? ' dimmed' : ''}`}
                    transform={`translate(${x}, ${y})`}
                    onPointerDown={(e) => handlePointerDown(node.id, e)}
                    onClick={(e) => {
                      e.stopPropagation()
                      handleNodeActivate(node)
                    }}
                  >
                    <circle r={node.radius} style={{ fill: node.color }} />
                    <text
                      className="graph-node-label"
                      x={node.kind === 'record' ? node.radius + 8 : 0}
                      y={4}
                      textAnchor={node.kind === 'record' ? 'start' : 'middle'}
                    >
                      {node.label}
                    </text>
                  </g>
                )
              })}
            </svg>
          </div>
          <aside className="dashboard-force-inspector">
            <div className="inspector-card">
              <p className="eyebrow">{t('Graph Focus', '圖譜焦點')}</p>
              <h4>{focusedNode ? focusedNode.label : t('All Nodes', '所有節點')}</h4>
              <p className="muted">
                {focusedNode
                  ? t(
                      `${focusedNode.kind} · ${focusedRecordCount} related records`,
                      `${focusedNode.kind} · 關聯 ${focusedRecordCount} 筆紀錄`,
                    )
                  : t('Click a node to inspect and route to Records.', '點擊節點可查看詳情並跳轉到紀錄篩選。')}
              </p>
              {focusedNode && (
                <button type="button" className="ghost-btn" onClick={() => handleNodeActivate(focusedNode)}>
                  {t('Open in Records', '在紀錄頁開啟')}
                </button>
              )}
            </div>
            <div className="inspector-card">
              <p className="eyebrow">{t('Connected Nodes', '關聯節點')}</p>
              {focusedLinkedNodes.length === 0 ? (
                <p className="muted">{t('No specific focus yet.', '目前尚未聚焦特定節點。')}</p>
              ) : (
                <ul className="inspector-node-list">
                  {focusedLinkedNodes.map((node) => (
                    <li key={node.id}>
                      <button type="button" onClick={() => handleNodeActivate(node)}>
                        <span>{node.label}</span>
                        <small>{node.kind}</small>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </aside>
        </div>
      </div>

      <div className="panel-grid-2 dashboard-panels">
        <div className="panel panel-strong">
          <h3>{t('Type Distribution', '類型分佈')}</h3>
          <ul className="distribution-list">
            {RECORD_TYPES.map((item) => {
              const count = stats.typeCounts[item] ?? 0
              const width = count === 0 ? 0 : Math.max(10, Math.round((count / maxTypeCount) * 100))
              return (
                <li key={item} className="dist-item">
                  <div className="dist-label">
                    <span>{item}</span>
                    <strong>{count}</strong>
                  </div>
                  <div className="dist-track">
                    <div className="dist-fill" style={{ width: `${width}%` }} />
                  </div>
                </li>
              )
            })}
          </ul>
        </div>
        <div className="panel panel-strong">
          <h3>{t('Recent Activity Wave', '近期活動波形')}</h3>
          <div className="trend-grid">
            {stats.recentDailyCounts.map((row) => {
              const height = row.count === 0 ? 8 : Math.max(12, Math.round((row.count / maxDailyCount) * 100))
              return (
                <div key={row.date} className="trend-col">
                  <div className="trend-bar-wrap">
                    <div className="trend-bar" style={{ height: `${height}%` }} />
                  </div>
                  <span className="trend-value">{row.count}</span>
                  <span className="trend-date">{row.date.slice(5)}</span>
                </div>
              )
            })}
          </div>
        </div>
      </div>

      <div className="panel panel-strong">
        <div className="panel-head-inline">
          <h3>{t('Top Tags', '熱門標籤')}</h3>
          <span className="muted">{t('Most active concepts in current workspace', '目前工作區最活躍概念')}</span>
        </div>
        {stats.topTags.length === 0 ? (
          <p className="muted">{t('No tags yet.', '尚無標籤。')}</p>
        ) : (
          <div className="tag-grid">
            {stats.topTags.map((item, index) => (
              <button
                key={item.tag}
                type="button"
                className={index < 3 ? 'tag-chip is-hot tag-chip-btn' : 'tag-chip tag-chip-btn'}
                onClick={() => onNavigateToRecords({ keyword: item.tag })}
              >
                <span>{item.tag}</span>
                <strong>{item.count}</strong>
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
