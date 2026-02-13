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
import {
  clearClaudeApiKey,
  clearGeminiApiKey,
  clearNotionApiKey,
  clearOpenaiApiKey,
  deleteRecord,
  exportMarkdownReport,
  getAppSettings,
  getDashboardStats,
  getHealthDiagnostics,
  getHomeFingerprint,
  hasClaudeApiKey,
  hasGeminiApiKey,
  hasNotionApiKey,
  listDebateRuns,
  hasOpenaiApiKey,
  listLogs,
  listRecords,
  pickCentralHomeDirectory,
  notebooklmAddRecordSource,
  notebooklmAsk,
  notebooklmCreateNotebook,
  notebooklmHealthCheck,
  notebooklmListNotebooks,
  pullRecordsFromNotion,
  rebuildSearchIndex,
  replayDebateMode,
  resolveCentralHome,
  runAiAnalysis,
  runDebateMode,
  saveAppSettings,
  setClaudeApiKey,
  setGeminiApiKey,
  setNotionApiKey,
  searchRecords,
  syncRecordBidirectional,
  syncRecordsBidirectional,
  setOpenaiApiKey,
  upsertRecord,
} from '../lib/tauri'
import { getLanguageLabel, isSupportedLanguage, SUPPORTED_LANGUAGES, translate, type UiLanguage } from '../i18n'
import { buildProviderRegistrySettings, ProviderRegistry } from '../lib/providerRegistry'
import {
  DASHBOARD_GRAPH_HEIGHT,
  DASHBOARD_GRAPH_WIDTH,
  DEBATE_ROLES,
  DEFAULT_DEBATE_MODEL_BY_PROVIDER,
  DEFAULT_MODEL,
  LOCAL_STORAGE_KEY,
  LOCAL_STORAGE_LANGUAGE_KEY,
  RECORD_TYPES,
  TYPE_COLORS,
} from '../constants'
import { useNotices } from '../hooks/useNotices'
import type {
  AiProvider,
  AppSettings,
  DebateProviderConfig,
  DebateProgress,
  DebateModeResponse,
  DebateOutputType,
  DebateReplayResponse,
  DebateRunSummary,
  DashboardStats,
  HealthDiagnostics,
  HomeFingerprint,
  LogEntry,
  NotionConflictStrategy,
  NotebookSummary,
  RecordItem,
  RecordPayload,
  RecordType,
  SearchResult,
  WorkspaceProfile,
} from '../types'

type TabKey = 'dashboard' | 'records' | 'logs' | 'ai' | 'integrations' | 'settings' | 'health'

type SearchMeta = {
  indexed: boolean
  total: number
  tookMs: number
  snippets: Record<string, string>
}

type RecordFormState = {
  recordType: RecordType
  title: string
  createdAt: string
  date: string
  tagsText: string
  notionPageId: string
  notionUrl: string
  notionSyncStatus: string
  notionError: string
  finalBody: string
  sourceText: string
}

const TAB_ITEMS: TabKey[] = ['dashboard', 'records', 'logs', 'ai', 'integrations', 'settings', 'health']

type TemplateValues = Record<string, string | number>

type LogPulse = {
  date: string
  count: number
  failed: number
}

type AiInsights = {
  summary: string[]
  risks: string[]
  actions: string[]
}

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

function nowIso() {
  return new Date().toISOString()
}

function emptyForm(): RecordFormState {
  return {
    recordType: 'idea',
    title: '',
    createdAt: nowIso(),
    date: new Date().toISOString().slice(0, 10),
    tagsText: '',
    notionPageId: '',
    notionUrl: '',
    notionSyncStatus: 'SUCCESS',
    notionError: '',
    finalBody: '',
    sourceText: '',
  }
}

function formFromRecord(record: RecordItem): RecordFormState {
  return {
    recordType: record.recordType,
    title: record.title,
    createdAt: record.createdAt,
    date: record.date ?? '',
    tagsText: record.tags.join(', '),
    notionPageId: record.notionPageId ?? '',
    notionUrl: record.notionUrl ?? '',
    notionSyncStatus: record.notionSyncStatus || 'SUCCESS',
    notionError: record.notionError ?? '',
    finalBody: record.finalBody,
    sourceText: record.sourceText,
  }
}

function payloadFromForm(form: RecordFormState): RecordPayload {
  return {
    recordType: form.recordType,
    title: form.title,
    createdAt: form.createdAt,
    date: form.date || null,
    tags: form.tagsText
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean),
    notionPageId: form.notionPageId || null,
    notionUrl: form.notionUrl || null,
    notionSyncStatus: form.notionSyncStatus || 'SUCCESS',
    notionError: form.notionError || null,
    finalBody: form.finalBody,
    sourceText: form.sourceText,
  }
}

function parseProviderCapabilitiesInput(value: string): string[] {
  const normalized = value
    .split(/[\n,]/)
    .map((item) => item.trim().toLowerCase())
    .filter(Boolean)
  return [...new Set(normalized)]
}

function makeProfile(name = 'New Profile'): WorkspaceProfile {
  const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, '-') || 'profile'
  return {
    id: `profile-${slug}-${Date.now()}`,
    name,
    centralHome: '',
    defaultProvider: 'local',
    defaultModel: DEFAULT_MODEL,
  }
}

function getDebateModelDefault(providerId: string): string {
  return DEFAULT_DEBATE_MODEL_BY_PROVIDER[providerId] ?? 'auto'
}

function statusTone(status: string): 'ok' | 'warn' | 'error' {
  const normalized = status.toLowerCase()
  if (normalized.includes('fail') || normalized.includes('error')) {
    return 'error'
  }
  if (normalized.includes('warn') || normalized.includes('pending')) {
    return 'warn'
  }
  return 'ok'
}

function buildLogPulse(logs: LogEntry[], days = 10): LogPulse[] {
  const buckets = new Map<string, LogPulse>()

  for (const item of logs) {
    if (!item.timestamp) {
      continue
    }
    const date = item.timestamp.slice(0, 10)
    const existing = buckets.get(date) ?? { date, count: 0, failed: 0 }
    existing.count += 1
    if (statusTone(item.status || '') === 'error') {
      existing.failed += 1
    }
    buckets.set(date, existing)
  }

  return [...buckets.values()]
    .sort((a, b) => a.date.localeCompare(b.date))
    .slice(-days)
}

function cleanInsightLine(line: string): string {
  return line.replace(/^[\s\-*•\d.)]+/, '').trim()
}

function extractAiInsights(content: string): AiInsights {
  const lines = content
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)

  const summary: string[] = []
  const risks: string[] = []
  const actions: string[] = []

  for (const line of lines) {
    const cleaned = cleanInsightLine(line)
    if (!cleaned) {
      continue
    }
    const lower = cleaned.toLowerCase()

    if (risks.length < 4 && /(risk|blocker|issue|danger|風險|阻塞|問題|危險)/i.test(cleaned)) {
      risks.push(cleaned)
      continue
    }

    if (
      actions.length < 5 &&
      (/(action|next|todo|plan|execute|行動|下一步|待辦|執行)/i.test(cleaned) ||
        /^(\d+\.|[-*•])/.test(line))
    ) {
      actions.push(cleaned)
      continue
    }

    if (summary.length < 4 && !/(title|summary|摘要|結論)/i.test(lower)) {
      summary.push(cleaned)
    }
  }

  return {
    summary,
    risks,
    actions,
  }
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value))
}

function linkNodeId(endpoint: string | DashboardGraphNode): string {
  return typeof endpoint === 'string' ? endpoint : endpoint.id
}

function shortenLabel(value: string, max = 26): string {
  if (value.length <= max) {
    return value
  }
  return `${value.slice(0, Math.max(8, max - 1)).trim()}…`
}

function createDashboardGraph(records: RecordItem[], stats: DashboardStats | null): {
  nodes: DashboardGraphNode[]
  links: DashboardGraphLink[]
} {
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
    const count = stats?.typeCounts[type] ?? records.filter((record) => record.recordType === type).length
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
      if (!cleanTag) {
        continue
      }
      tagCounts.set(cleanTag, (tagCounts.get(cleanTag) ?? 0) + 1)
    }
  }

  const topTags = [...tagCounts.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 18)

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
    const linkedTags = entry.record.tags
      .map((tag) => tag.trim())
      .filter(Boolean)
      .slice(0, 4)

    for (const tag of linkedTags) {
      const tagNode = tagNodeMap.get(tag)
      if (!tagNode) {
        continue
      }
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

function App() {
  const [activeTab, setActiveTab] = useState<TabKey>('dashboard')
  const [language, setLanguage] = useState<UiLanguage>(() => {
    if (typeof window === 'undefined') {
      return 'en'
    }
    const saved = localStorage.getItem(LOCAL_STORAGE_LANGUAGE_KEY)
    return isSupportedLanguage(saved) ? saved : 'en'
  })

  const [centralHomeInput, setCentralHomeInput] = useState('')
  const [centralHome, setCentralHome] = useState('')

  const [allRecords, setAllRecords] = useState<RecordItem[]>([])
  const [displayedRecords, setDisplayedRecords] = useState<RecordItem[]>([])
  const [logs, setLogs] = useState<LogEntry[]>([])
  const [stats, setStats] = useState<DashboardStats | null>(null)
  const [health, setHealth] = useState<HealthDiagnostics | null>(null)

  const [selectedRecordPath, setSelectedRecordPath] = useState<string | null>(null)
  const [recordForm, setRecordForm] = useState<RecordFormState>(emptyForm())
  const [selectedLogIndex, setSelectedLogIndex] = useState<number>(-1)

  const [recordFilterType, setRecordFilterType] = useState<'all' | RecordType>('all')
  const [recordKeyword, setRecordKeyword] = useState('')
  const [recordDateFrom, setRecordDateFrom] = useState('')
  const [recordDateTo, setRecordDateTo] = useState('')
  const [visibleCount, setVisibleCount] = useState(200)
  const [searchMeta, setSearchMeta] = useState<SearchMeta | null>(null)

  const [aiProvider, setAiProvider] = useState<AiProvider>('local')
  const [aiModel, setAiModel] = useState(DEFAULT_MODEL)
  const [aiPrompt, setAiPrompt] = useState(
    '請整理最近的重點方向、重複模式、風險，並產生下一週可執行清單。',
  )
  const [aiResult, setAiResult] = useState('')
  const [aiIncludeLogs, setAiIncludeLogs] = useState(true)
  const [aiMaxRecords, setAiMaxRecords] = useState(30)
  const [debateProblem, setDebateProblem] = useState(
    '在不影響既有功能的前提下，決定 KOF Note Debate Mode v0.1 的最佳實作策略。',
  )
  const [debateConstraintsText, setDebateConstraintsText] = useState(
    'Local-first\n可 replay\n固定 5 角色 + 3 回合\n輸出可執行 Final Packet',
  )
  const [debateOutputType, setDebateOutputType] = useState<DebateOutputType>('decision')
  const [debateProvider, setDebateProvider] = useState('local')
  const [debateModel, setDebateModel] = useState('')
  const [debateAdvancedMode, setDebateAdvancedMode] = useState(false)
  const [debatePerRoleProvider, setDebatePerRoleProvider] = useState<Record<string, string>>({})
  const [debatePerRoleModel, setDebatePerRoleModel] = useState<Record<string, string>>({})
  const [debateWritebackType, setDebateWritebackType] = useState<'decision' | 'worklog'>('decision')
  const [debateMaxTurnSeconds, setDebateMaxTurnSeconds] = useState(35)
  const [debateMaxTurnTokens, setDebateMaxTurnTokens] = useState(900)
  const [debateRunId, setDebateRunId] = useState('')
  const [debateResult, setDebateResult] = useState<DebateModeResponse | null>(null)
  const [debateReplayResult, setDebateReplayResult] = useState<DebateReplayResponse | null>(null)
  const [debateRuns, setDebateRuns] = useState<DebateRunSummary[]>([])
  const [debateProgress, setDebateProgress] = useState<{
    round: string
    role: string
    turnIndex: number
    totalTurns: number
  } | null>(null)

  const [appSettings, setAppSettings] = useState<AppSettings>({
    profiles: [],
    activeProfileId: null,
    pollIntervalSec: 8,
    uiPreferences: {},
    integrations: {
      notion: {
        enabled: false,
        databaseId: '',
      },
      notebooklm: {
        command: 'uvx',
        args: ['kof-notebooklm-mcp'],
        defaultNotebookId: null,
      },
    },
    providerRegistry: buildProviderRegistrySettings(),
  })
  const [selectedProfileId, setSelectedProfileId] = useState<string>('')
  const [profileDraft, setProfileDraft] = useState<WorkspaceProfile>(makeProfile())

  const [openaiKeyDraft, setOpenaiKeyDraft] = useState('')
  const [hasOpenaiKey, setHasOpenaiKey] = useState(false)
  const [geminiKeyDraft, setGeminiKeyDraft] = useState('')
  const [hasGeminiKey, setHasGeminiKey] = useState(false)
  const [claudeKeyDraft, setClaudeKeyDraft] = useState('')
  const [hasClaudeKey, setHasClaudeKey] = useState(false)
  const [notionKeyDraft, setNotionKeyDraft] = useState('')
  const [hasNotionKey, setHasNotionKey] = useState(false)
  const [notionConflictStrategy, setNotionConflictStrategy] = useState<NotionConflictStrategy>('manual')
  const [notionSyncReport, setNotionSyncReport] = useState('')

  const [notebookList, setNotebookList] = useState<NotebookSummary[]>([])
  const [selectedNotebookId, setSelectedNotebookId] = useState('')
  const [newNotebookTitle, setNewNotebookTitle] = useState('')
  const [notebookQuestion, setNotebookQuestion] = useState('請總結最近重點與下週行動。')
  const [notebookAnswer, setNotebookAnswer] = useState('')
  const [notebookCitations, setNotebookCitations] = useState<string[]>([])
  const [notebookHealthText, setNotebookHealthText] = useState('')

  const [reportTitle, setReportTitle] = useState('')
  const [reportPath, setReportPath] = useState('')
  const [reportDays, setReportDays] = useState(7)

  const [fingerprint, setFingerprint] = useState<HomeFingerprint | null>(null)

  const [busy, setBusy] = useState(false)
  const [debateBusy, setDebateBusy] = useState(false)
  const [commandOpen, setCommandOpen] = useState(false)
  const [commandQuery, setCommandQuery] = useState('')
  const { notices, pushNotice } = useNotices()
  const [dashboardGraphNodes, setDashboardGraphNodes] = useState<DashboardGraphNode[]>([])
  const [dashboardGraphLinks, setDashboardGraphLinks] = useState<DashboardGraphLink[]>([])
  const [dashboardFocusedNodeId, setDashboardFocusedNodeId] = useState<string | null>(null)

  const commandInputRef = useRef<HTMLInputElement | null>(null)
  const dashboardGraphSvgRef = useRef<SVGSVGElement | null>(null)
  const dashboardGraphSimulationRef = useRef<Simulation<DashboardGraphNode, DashboardGraphLink> | null>(null)
  const dashboardGraphDraggingRef = useRef<string | null>(null)
  const t = useCallback(
    (keyOrEn: string, zhOrValues?: string | TemplateValues) => {
      if (typeof zhOrValues === 'string') {
        return language === 'zh-TW' ? zhOrValues : keyOrEn
      }
      return translate(language, keyOrEn, zhOrValues)
    },
    [language],
  )
  const languageName = useCallback((code: UiLanguage) => getLanguageLabel(code, language), [language])
  const tabLabel = useCallback(
    (key: TabKey) => {
      switch (key) {
        case 'dashboard':
          return t('tab.dashboard')
        case 'records':
          return t('tab.records')
        case 'logs':
          return t('tab.logs')
        case 'ai':
          return t('tab.ai')
        case 'integrations':
          return t('tab.integrations')
        case 'settings':
          return t('tab.settings')
        case 'health':
          return t('tab.health')
        default:
          return key
      }
    },
    [t],
  )

  const debateProviderRegistry = useMemo(() => new ProviderRegistry(appSettings.providerRegistry), [appSettings.providerRegistry])
  const debateProviderOptions = useMemo(() => {
    const enabled = debateProviderRegistry.list({ enabledOnly: true }).map((item) => item.id)
    return ['local', ...enabled.filter((item) => item !== 'local')]
  }, [debateProviderRegistry])
  const debateModelDefault = useMemo(() => getDebateModelDefault(debateProvider), [debateProvider])
  const debateProviderLabel = useCallback(
    (providerId: string) => {
      if (providerId === 'local') {
        return 'local'
      }
      const provider = debateProviderRegistry.get(providerId)
      return provider ? `${provider.id} (${provider.type})` : providerId
    },
    [debateProviderRegistry],
  )

  const withBusy = useCallback(async <T,>(task: () => Promise<T>) => {
    setBusy(true)
    try {
      return await task()
    } finally {
      setBusy(false)
    }
  }, [])

  const dashboardGraphModel = useMemo(() => createDashboardGraph(allRecords, stats), [allRecords, stats])

  useEffect(() => {
    if (dashboardGraphModel.nodes.length === 0) {
      setDashboardGraphNodes([])
      setDashboardGraphLinks([])
      setDashboardFocusedNodeId(null)
      dashboardGraphSimulationRef.current?.stop()
      dashboardGraphSimulationRef.current = null
      return
    }

    const seededNodes = dashboardGraphModel.nodes.map((node, index) => {
      const angle = (Math.PI * 2 * index) / Math.max(1, dashboardGraphModel.nodes.length)
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

    const seededLinks = dashboardGraphModel.links.map((link) => ({ ...link }))

    const simulation = forceSimulation<DashboardGraphNode>(seededNodes)
      .force(
        'link',
        forceLink<DashboardGraphNode, DashboardGraphLink>(seededLinks)
          .id((node) => node.id)
          .distance((link) => {
            if (link.relation === 'core-type') {
              return 110
            }
            if (link.relation === 'type-record') {
              return 88
            }
            return 72
          })
          .strength((link) => {
            if (link.relation === 'record-tag') {
              return 0.23
            }
            return 0.34
          }),
      )
      .force(
        'charge',
        forceManyBody<DashboardGraphNode>().strength((node) => {
          if (node.kind === 'core') {
            return -560
          }
          if (node.kind === 'type') {
            return -310
          }
          if (node.kind === 'tag') {
            return -180
          }
          return -140
        }),
      )
      .force('center', forceCenter(DASHBOARD_GRAPH_WIDTH / 2, DASHBOARD_GRAPH_HEIGHT / 2))
      .force(
        'collide',
        forceCollide<DashboardGraphNode>()
          .radius((node) => node.radius + 8)
          .strength(0.86),
      )
      .alpha(1)
      .alphaDecay(0.035)

    dashboardGraphSimulationRef.current?.stop()
    dashboardGraphSimulationRef.current = simulation

    let frameId = 0
    const publish = () => {
      if (frameId) {
        return
      }
      frameId = window.requestAnimationFrame(() => {
        frameId = 0
        setDashboardGraphNodes([...seededNodes])
        setDashboardGraphLinks([...seededLinks])
      })
    }

    simulation.on('tick', publish)
    publish()

    return () => {
      if (frameId) {
        window.cancelAnimationFrame(frameId)
      }
      simulation.stop()
      if (dashboardGraphSimulationRef.current === simulation) {
        dashboardGraphSimulationRef.current = null
      }
    }
  }, [dashboardGraphModel])

  const focusRecordFromGraph = useCallback((jsonPath: string) => {
    const target = allRecords.find((record) => record.jsonPath === jsonPath)
    if (!target) {
      return
    }
    setSelectedRecordPath(target.jsonPath ?? null)
    setRecordForm(formFromRecord(target))
  }, [allRecords])

  const handleDashboardGraphNodeActivate = useCallback(
    (node: DashboardGraphNode) => {
      setDashboardFocusedNodeId(node.id)

      if (node.kind === 'type' && node.recordType) {
        setRecordKeyword('')
        setRecordFilterType(node.recordType)
        setActiveTab('records')
        pushNotice('info', t(`Switched Records filter to ${node.recordType}.`, `已切換到 ${node.recordType} 類型篩選。`))
        return
      }

      if (node.kind === 'tag' && node.tag) {
        setRecordFilterType('all')
        setRecordKeyword(node.tag)
        setActiveTab('records')
        pushNotice('info', t(`Searching Records by tag: ${node.tag}`, `已以標籤搜尋紀錄：${node.tag}`))
        return
      }

      if (node.kind === 'record' && node.jsonPath) {
        focusRecordFromGraph(node.jsonPath)
        setActiveTab('records')
        pushNotice('info', t('Opened record in Records panel.', '已在紀錄面板開啟該筆紀錄。'))
      }
    },
    [focusRecordFromGraph, pushNotice, t],
  )

  const toGraphPoint = useCallback((event: React.PointerEvent<SVGSVGElement>) => {
    const svg = dashboardGraphSvgRef.current
    if (!svg) {
      return null
    }
    const ctm = svg.getScreenCTM()
    if (!ctm) {
      return null
    }
    const point = svg.createSVGPoint()
    point.x = event.clientX
    point.y = event.clientY
    const graphPoint = point.matrixTransform(ctm.inverse())
    return {
      x: clamp(graphPoint.x, 16, DASHBOARD_GRAPH_WIDTH - 16),
      y: clamp(graphPoint.y, 16, DASHBOARD_GRAPH_HEIGHT - 16),
    }
  }, [])

  const handleDashboardNodePointerDown = useCallback(
    (nodeId: string, event: React.PointerEvent<SVGGElement>) => {
      event.stopPropagation()
      dashboardGraphDraggingRef.current = nodeId
      event.currentTarget.setPointerCapture?.(event.pointerId)

      const simulation = dashboardGraphSimulationRef.current
      if (!simulation) {
        return
      }
      const node = simulation.nodes().find((item) => item.id === nodeId)
      if (!node) {
        return
      }
      node.fx = node.x ?? DASHBOARD_GRAPH_WIDTH / 2
      node.fy = node.y ?? DASHBOARD_GRAPH_HEIGHT / 2
      simulation.alphaTarget(0.28).restart()
    },
    [],
  )

  const releaseDashboardDrag = useCallback(() => {
    const draggingId = dashboardGraphDraggingRef.current
    if (!draggingId) {
      return
    }
    const simulation = dashboardGraphSimulationRef.current
    if (simulation) {
      const node = simulation.nodes().find((item) => item.id === draggingId)
      if (node) {
        node.fx = null
        node.fy = null
      }
      simulation.alphaTarget(0)
    }
    dashboardGraphDraggingRef.current = null
  }, [])

  const handleDashboardGraphPointerMove = useCallback(
    (event: React.PointerEvent<SVGSVGElement>) => {
      const draggingId = dashboardGraphDraggingRef.current
      if (!draggingId) {
        return
      }
      const point = toGraphPoint(event)
      const simulation = dashboardGraphSimulationRef.current
      if (!point || !simulation) {
        return
      }
      const node = simulation.nodes().find((item) => item.id === draggingId)
      if (!node) {
        return
      }
      node.fx = point.x
      node.fy = point.y
      simulation.alphaTarget(0.28).restart()
    },
    [toGraphPoint],
  )

  const visibleRecords = useMemo(
    () => displayedRecords.slice(0, Math.min(visibleCount, displayedRecords.length)),
    [displayedRecords, visibleCount],
  )

  const selectedLog = useMemo(
    () => (selectedLogIndex >= 0 && selectedLogIndex < logs.length ? logs[selectedLogIndex] : null),
    [selectedLogIndex, logs],
  )

  const selectedRecord = useMemo(
    () => allRecords.find((item) => item.jsonPath === selectedRecordPath) ?? null,
    [allRecords, selectedRecordPath],
  )

  const selectedProfile = useMemo(
    () => appSettings.profiles.find((item) => item.id === selectedProfileId) ?? null,
    [appSettings.profiles, selectedProfileId],
  )

  const dashboardResolvedLinks = useMemo(
    () =>
      dashboardGraphLinks.map((link) => ({
        id: link.id,
        sourceId: linkNodeId(link.source),
        targetId: linkNodeId(link.target),
        relation: link.relation,
      })),
    [dashboardGraphLinks],
  )

  const dashboardFocusedNode = useMemo(
    () => dashboardGraphNodes.find((node) => node.id === dashboardFocusedNodeId) ?? null,
    [dashboardFocusedNodeId, dashboardGraphNodes],
  )

  const dashboardFocusNeighbors = useMemo(() => {
    if (!dashboardFocusedNodeId) {
      return new Set<string>()
    }
    const related = new Set<string>([dashboardFocusedNodeId])
    for (const link of dashboardResolvedLinks) {
      if (link.sourceId === dashboardFocusedNodeId || link.targetId === dashboardFocusedNodeId) {
        related.add(link.sourceId)
        related.add(link.targetId)
      }
    }
    return related
  }, [dashboardFocusedNodeId, dashboardResolvedLinks])

  useEffect(() => {
    if (!dashboardFocusedNodeId) {
      return
    }
    if (!dashboardGraphNodes.some((node) => node.id === dashboardFocusedNodeId)) {
      setDashboardFocusedNodeId(null)
    }
  }, [dashboardFocusedNodeId, dashboardGraphNodes])

  const centralHomeDisplay = useMemo(() => {
    const raw = (centralHome || centralHomeInput).trim()
    if (!raw) {
      return {
        name: '',
        fullPath: '',
      }
    }
    const normalized = raw.replace(/\\/g, '/').replace(/\/+$/, '')
    const parts = normalized.split('/').filter(Boolean)
    return {
      name: parts[parts.length - 1] ?? raw,
      fullPath: raw,
    }
  }, [centralHome, centralHomeInput])

  const refreshCore = useCallback(
    async (home: string) => {
      const [nextRecords, nextLogs, nextStats, nextHealth, nextFingerprint] = await Promise.all([
        listRecords(home),
        listLogs(home),
        getDashboardStats(home),
        getHealthDiagnostics(home),
        getHomeFingerprint(home),
      ])

      setAllRecords(nextRecords)
      setLogs(nextLogs)
      setStats(nextStats)
      setHealth(nextHealth)
      setFingerprint(nextFingerprint)

      return {
        records: nextRecords,
        logs: nextLogs,
      }
    },
    [],
  )

  const refreshDebateRuns = useCallback(
    async (home: string) => {
      if (!home.trim()) {
        setDebateRuns([])
        return []
      }

      const runs = await listDebateRuns({ centralHome: home })
      setDebateRuns(runs)
      return runs
    },
    [],
  )

  const applySearch = useCallback(async () => {
    if (!centralHome) {
      return
    }

    const hasFilter =
      recordKeyword.trim() || recordFilterType !== 'all' || recordDateFrom.trim() || recordDateTo.trim()

    if (!hasFilter) {
      setDisplayedRecords(allRecords)
      setSearchMeta(null)
      setVisibleCount(200)
      return
    }

    const result: SearchResult = await searchRecords({
      centralHome,
      query: recordKeyword.trim() || undefined,
      recordType: recordFilterType === 'all' ? undefined : recordFilterType,
      dateFrom: recordDateFrom.trim() || undefined,
      dateTo: recordDateTo.trim() || undefined,
      limit: 1000,
      offset: 0,
    })

    setDisplayedRecords(result.records)
    setSearchMeta({ indexed: result.indexed, total: result.total, tookMs: result.tookMs, snippets: result.snippets })
    setVisibleCount(200)
  }, [allRecords, centralHome, recordDateFrom, recordDateTo, recordFilterType, recordKeyword])

  const loadCentralHome = useCallback(
    async (input = centralHomeInput) => {
      if (!input.trim()) {
        pushNotice('error', t('Central Home path is required.', '中央路徑不可為空。'))
        return
      }

      await withBusy(async () => {
        const resolved = await resolveCentralHome(input.trim())
        setCentralHome(resolved.centralHome)
        setCentralHomeInput(resolved.centralHome)
        localStorage.setItem(LOCAL_STORAGE_KEY, resolved.centralHome)

        const [data] = await Promise.all([refreshCore(resolved.centralHome), refreshDebateRuns(resolved.centralHome)])
        setDisplayedRecords(data.records)
        setSearchMeta(null)

        if (data.records.length > 0) {
          const first = data.records[0]
          setSelectedRecordPath(first.jsonPath ?? null)
          setRecordForm(formFromRecord(first))
        } else {
          setSelectedRecordPath(null)
          setRecordForm(emptyForm())
        }

        setSelectedLogIndex(data.logs.length > 0 ? 0 : -1)
        pushNotice(
          'success',
          resolved.corrected
            ? t(`Loaded and normalized to ${resolved.centralHome}`, `已載入並正規化為 ${resolved.centralHome}`)
            : t(`Loaded ${resolved.centralHome}`, `已載入 ${resolved.centralHome}`),
        )
      })
    },
    [centralHomeInput, pushNotice, refreshCore, refreshDebateRuns, t, withBusy],
  )

  const handlePickCentralHome = useCallback(async () => {
    try {
      const selected = await pickCentralHomeDirectory(centralHome || centralHomeInput || undefined)
      if (!selected) {
        return
      }
      setCentralHomeInput(selected)
      await loadCentralHome(selected)
    } catch (error) {
      pushNotice('error', t(`Error: ${String(error)}`, `錯誤：${String(error)}`))
    }
  }, [centralHome, centralHomeInput, loadCentralHome, pushNotice, t])

  const loadSettings = useCallback(async () => {
    const [settings, hasOpenai, hasGemini, hasClaude, notionKey] = await Promise.all([
      getAppSettings(),
      hasOpenaiApiKey(),
      hasGeminiApiKey(),
      hasClaudeApiKey(),
      hasNotionApiKey(),
    ])
    const normalizedSettings: AppSettings = {
      ...settings,
      providerRegistry: buildProviderRegistrySettings(settings.providerRegistry),
    }
    setAppSettings(normalizedSettings)
    setHasOpenaiKey(hasOpenai)
    setHasGeminiKey(hasGemini)
    setHasClaudeKey(hasClaude)
    setHasNotionKey(notionKey)
    setSelectedNotebookId(normalizedSettings.integrations.notebooklm.defaultNotebookId ?? '')

    if (normalizedSettings.activeProfileId) {
      setSelectedProfileId(normalizedSettings.activeProfileId)
      const active = normalizedSettings.profiles.find((item) => item.id === normalizedSettings.activeProfileId)
      if (active) {
        setProfileDraft(active)
      }
    }

    return normalizedSettings
  }, [])

  useEffect(() => {
    localStorage.setItem(LOCAL_STORAGE_LANGUAGE_KEY, language)
  }, [language])

  useEffect(() => {
    void (async () => {
      try {
        const settings = await loadSettings()
        const cachedHome = localStorage.getItem(LOCAL_STORAGE_KEY)
        if (cachedHome) {
          setCentralHomeInput(cachedHome)
          await loadCentralHome(cachedHome)
          return
        }

        const active = settings.profiles.find((item) => item.id === settings.activeProfileId)
        if (active?.centralHome) {
          setCentralHomeInput(active.centralHome)
          await loadCentralHome(active.centralHome)
        }
      } catch (error) {
        pushNotice('error', t(`Error: ${String(error)}`, `錯誤：${String(error)}`))
      }
    })()
  }, [loadCentralHome, loadSettings, pushNotice, t])

  useEffect(() => {
    if (!centralHome) {
      return
    }

    const timer = window.setTimeout(() => {
      void applySearch().catch((error) => pushNotice('error', t(`Error: ${String(error)}`, `錯誤：${String(error)}`)))
    }, 250)

    return () => window.clearTimeout(timer)
  }, [applySearch, centralHome, pushNotice, t])

  useEffect(() => {
    if (!debateProviderOptions.includes(debateProvider)) {
      setDebateProvider('local')
    }
  }, [debateProvider, debateProviderOptions])

  useEffect(() => {
    let unlisten: (() => void) | undefined
    let cancelled = false

    void import('@tauri-apps/api/event')
      .then(({ listen }) =>
        listen<DebateProgress>('debate-progress', (event) => {
          const payload = event.payload
          setDebateProgress({
            round: payload.round,
            role: payload.role,
            turnIndex: payload.turnIndex,
            totalTurns: payload.totalTurns,
          })
        }),
      )
      .then((fn) => {
        if (cancelled) {
          fn()
          return
        }
        unlisten = fn
      })
      .catch(() => {
        // Ignore event bridge setup failures.
      })

    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  useEffect(() => {
    if (!centralHome) {
      return
    }

    const interval = Math.max(3, appSettings.pollIntervalSec || 8) * 1000
    const handle = window.setInterval(() => {
      void (async () => {
        try {
          const next = await getHomeFingerprint(centralHome)
          if (fingerprint && next.token !== fingerprint.token) {
            const data = await refreshCore(centralHome)
            setDisplayedRecords(data.records)
            setSearchMeta(null)
            pushNotice('info', t('Auto refreshed after external updates.', '偵測到外部更新，已自動刷新。'))
          }
          setFingerprint(next)
        } catch {
          // Ignore polling failures.
        }
      })()
    }, interval)

    return () => window.clearInterval(handle)
  }, [appSettings.pollIntervalSec, centralHome, fingerprint, pushNotice, refreshCore, t])

  const handleSaveRecord = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }

    await withBusy(async () => {
      const payload = payloadFromForm(recordForm)
      const saved = await upsertRecord(centralHome, payload, selectedRecordPath)
      const data = await refreshCore(centralHome)

      setAllRecords(data.records)
      setDisplayedRecords(data.records)
      setSearchMeta(null)

      const matched = data.records.find((item) => item.jsonPath === saved.jsonPath)
      if (matched) {
        setSelectedRecordPath(matched.jsonPath ?? null)
        setRecordForm(formFromRecord(matched))
      }

      pushNotice('success', t('Record saved.', '紀錄已儲存。'))
    })
  }, [centralHome, pushNotice, recordForm, refreshCore, selectedRecordPath, t, withBusy])

  const handleDeleteRecord = useCallback(async () => {
    if (!centralHome || !selectedRecordPath) {
      pushNotice('error', t('Select a record first.', '請先選擇一筆紀錄。'))
      return
    }

    if (!window.confirm(t('Delete selected record?', '要刪除選取的紀錄嗎？'))) {
      return
    }

    await withBusy(async () => {
      await deleteRecord(centralHome, selectedRecordPath)
      const data = await refreshCore(centralHome)
      setDisplayedRecords(data.records)
      setSearchMeta(null)

      if (data.records.length > 0) {
        const first = data.records[0]
        setSelectedRecordPath(first.jsonPath ?? null)
        setRecordForm(formFromRecord(first))
      } else {
        setSelectedRecordPath(null)
        setRecordForm(emptyForm())
      }

      pushNotice('success', t('Record deleted.', '紀錄已刪除。'))
    })
  }, [centralHome, pushNotice, refreshCore, selectedRecordPath, t, withBusy])

  const handleRunAi = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }

    await withBusy(async () => {
      const result = await runAiAnalysis({
        centralHome,
        provider: aiProvider,
        model: aiModel,
        prompt: aiPrompt,
        includeLogs: aiIncludeLogs,
        maxRecords: aiMaxRecords,
      })
      setAiResult(result.content)
      pushNotice('success', t(`${result.provider} analysis completed.`, `${result.provider} 分析已完成。`))
    })
  }, [aiIncludeLogs, aiMaxRecords, aiModel, aiPrompt, aiProvider, centralHome, pushNotice, t, withBusy])

  const handleRunDebate = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }
    if (debateBusy) {
      pushNotice('info', t('Debate is still running.', '辯論仍在執行中。'))
      return
    }
    if (!debateProblem.trim()) {
      pushNotice('error', t('Debate problem cannot be empty.', '辯論問題不可為空。'))
      return
    }

    const constraints = debateConstraintsText
      .split(/\r?\n|,/)
      .map((item) => item.trim())
      .filter(Boolean)

    setDebateBusy(true)
    setDebateProgress(null)
    pushNotice(
      'info',
      t(
        'Debate started in background. You can keep navigating while it runs.',
        '辯論已在背景開始執行，執行期間仍可繼續操作介面。',
      ),
    )
    try {
      const providerModel = debateProvider === 'local' ? 'local-heuristic-v1' : debateModel.trim()
      const participants = DEBATE_ROLES.map((role) => ({
        role,
        modelProvider: debateAdvancedMode
          ? (debatePerRoleProvider[role] || debateProvider)
          : debateProvider,
        modelName: debateAdvancedMode
          ? (debatePerRoleModel[role] || providerModel)
          : providerModel,
      }))

      const result = await runDebateMode({
        centralHome,
        request: {
          problem: debateProblem.trim(),
          constraints,
          outputType: debateOutputType,
          participants,
          maxTurnSeconds: Math.max(5, Math.min(120, debateMaxTurnSeconds || 35)),
          maxTurnTokens: Math.max(128, Math.min(4096, debateMaxTurnTokens || 900)),
          writebackRecordType: debateWritebackType,
        },
      })

      setDebateResult(result)
      setDebateReplayResult(null)
      setDebateRunId(result.runId)
      setAiResult(JSON.stringify(result.finalPacket, null, 2))
      await refreshDebateRuns(centralHome)

      if (result.degraded) {
        const hasCodexCliError = result.errorCodes.some((item) => item.includes('DEBATE_ERR_PROVIDER_CODEX_CLI'))
        pushNotice(
          'info',
          t(
            `Debate completed in degraded mode. run=${result.runId}. codes=${result.errorCodes.join(', ') || '-'}.`,
            `辯論以降級模式完成。run=${result.runId}。代碼=${result.errorCodes.join(', ') || '-'}。`,
          ),
        )
        if (hasCodexCliError) {
          pushNotice(
            'error',
            t(
              'codex-cli failed. Verify `codex login`, network, and ~/.codex permission.',
              'codex-cli 失敗。請確認 `codex login`、網路，以及 ~/.codex 權限。',
            ),
          )
        }
      } else {
        pushNotice(
          'success',
          t(`Debate completed. run=${result.runId}.`, `辯論完成。run=${result.runId}。`),
        )
      }
    } catch (error) {
      pushNotice('error', t(`Debate failed: ${String(error)}`, `辯論失敗：${String(error)}`))
    } finally {
      setDebateBusy(false)
      setDebateProgress(null)
    }
  }, [
    centralHome,
    debateBusy,
    debateConstraintsText,
    debateMaxTurnSeconds,
    debateMaxTurnTokens,
    debateAdvancedMode,
    debateModel,
    debateOutputType,
    debatePerRoleModel,
    debatePerRoleProvider,
    debateProblem,
    debateProvider,
    debateWritebackType,
    refreshDebateRuns,
    pushNotice,
    t,
  ])

  const replayDebateByRunId = useCallback(async (runId: string) => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }
    if (debateBusy) {
      pushNotice('info', t('Debate is still running.', '辯論仍在執行中。'))
      return
    }
    if (!runId.trim()) {
      pushNotice('error', t('Run ID cannot be empty.', 'Run ID 不可為空。'))
      return
    }

    setDebateBusy(true)
    try {
      const replay = await replayDebateMode({
        centralHome,
        runId: runId.trim(),
      })
      setDebateReplayResult(replay)
      setAiResult(JSON.stringify(replay.finalPacket, null, 2))

      if (replay.consistency.issues.length > 0) {
        pushNotice(
          'info',
          t(
            `Replay loaded with ${replay.consistency.issues.length} consistency issue(s).`,
            `Replay 已載入，含 ${replay.consistency.issues.length} 個一致性問題。`,
          ),
        )
      } else {
        pushNotice('success', t('Replay loaded.', 'Replay 已載入。'))
      }
    } catch (error) {
      pushNotice('error', t(`Replay failed: ${String(error)}`, `Replay 失敗：${String(error)}`))
    } finally {
      setDebateBusy(false)
    }
  }, [centralHome, debateBusy, pushNotice, t])

  const handleReplayDebate = useCallback(async () => {
    await replayDebateByRunId(debateRunId)
  }, [debateRunId, replayDebateByRunId])

  const handleSelectDebateRun = useCallback(
    async (runId: string) => {
      setDebateRunId(runId)
      await replayDebateByRunId(runId)
    },
    [replayDebateByRunId],
  )

  const handleSaveApiKey = useCallback(async () => {
    if (!openaiKeyDraft.trim()) {
      pushNotice('error', t('API key cannot be empty.', 'API 金鑰不可為空。'))
      return
    }

    await withBusy(async () => {
      await setOpenaiApiKey(openaiKeyDraft.trim())
      setOpenaiKeyDraft('')
      setHasOpenaiKey(true)
      pushNotice('success', t('OpenAI API key saved to Keychain.', 'OpenAI API 金鑰已儲存至 Keychain。'))
    })
  }, [openaiKeyDraft, pushNotice, t, withBusy])

  const handleClearApiKey = useCallback(async () => {
    await withBusy(async () => {
      await clearOpenaiApiKey()
      setHasOpenaiKey(false)
      pushNotice('success', t('OpenAI API key cleared.', 'OpenAI API 金鑰已清除。'))
    })
  }, [pushNotice, t, withBusy])

  const handleSaveGeminiKey = useCallback(async () => {
    if (!geminiKeyDraft.trim()) {
      pushNotice('error', t('Gemini API key cannot be empty.', 'Gemini API 金鑰不可為空。'))
      return
    }

    await withBusy(async () => {
      await setGeminiApiKey(geminiKeyDraft.trim())
      setGeminiKeyDraft('')
      setHasGeminiKey(true)
      pushNotice('success', t('Gemini API key saved to Keychain.', 'Gemini API 金鑰已儲存至 Keychain。'))
    })
  }, [geminiKeyDraft, pushNotice, t, withBusy])

  const handleClearGeminiKey = useCallback(async () => {
    await withBusy(async () => {
      await clearGeminiApiKey()
      setHasGeminiKey(false)
      pushNotice('success', t('Gemini API key cleared.', 'Gemini API 金鑰已清除。'))
    })
  }, [pushNotice, t, withBusy])

  const handleSaveClaudeKey = useCallback(async () => {
    if (!claudeKeyDraft.trim()) {
      pushNotice('error', t('Claude API key cannot be empty.', 'Claude API 金鑰不可為空。'))
      return
    }

    await withBusy(async () => {
      await setClaudeApiKey(claudeKeyDraft.trim())
      setClaudeKeyDraft('')
      setHasClaudeKey(true)
      pushNotice('success', t('Claude API key saved to Keychain.', 'Claude API 金鑰已儲存至 Keychain。'))
    })
  }, [claudeKeyDraft, pushNotice, t, withBusy])

  const handleClearClaudeKey = useCallback(async () => {
    await withBusy(async () => {
      await clearClaudeApiKey()
      setHasClaudeKey(false)
      pushNotice('success', t('Claude API key cleared.', 'Claude API 金鑰已清除。'))
    })
  }, [pushNotice, t, withBusy])

  const handleSaveNotionKey = useCallback(async () => {
    if (!notionKeyDraft.trim()) {
      pushNotice('error', t('Notion API key cannot be empty.', 'Notion API 金鑰不可為空。'))
      return
    }

    await withBusy(async () => {
      await setNotionApiKey(notionKeyDraft.trim())
      setNotionKeyDraft('')
      setHasNotionKey(true)
      pushNotice('success', t('Notion API key saved to Keychain.', 'Notion API 金鑰已儲存至 Keychain。'))
    })
  }, [notionKeyDraft, pushNotice, t, withBusy])

  const handleClearNotionKey = useCallback(async () => {
    await withBusy(async () => {
      await clearNotionApiKey()
      setHasNotionKey(false)
      pushNotice('success', t('Notion API key cleared.', 'Notion API 金鑰已清除。'))
    })
  }, [pushNotice, t, withBusy])

  const handleSyncSelectedToNotion = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }
    if (!selectedRecordPath) {
      pushNotice('error', t('Select a record first.', '請先選擇一筆紀錄。'))
      return
    }

    await withBusy(async () => {
      const result = await syncRecordBidirectional({
        centralHome,
        jsonPath: selectedRecordPath,
        databaseId: appSettings.integrations.notion.databaseId || undefined,
        conflictStrategy: notionConflictStrategy,
      })
      const data = await refreshCore(centralHome)
      setDisplayedRecords(data.records)
      setSearchMeta(null)
      setNotionSyncReport(JSON.stringify(result, null, 2))
      if (result.conflict) {
        pushNotice('error', result.notionError || t('Conflict detected.', '偵測到衝突。'))
      } else if (result.notionSyncStatus === 'SUCCESS') {
        pushNotice('success', t(`Notion sync (${result.action}) done.`, `Notion 同步（${result.action}）完成。`))
      } else {
        pushNotice('error', result.notionError || t('Notion sync failed.', 'Notion 同步失敗。'))
      }
    })
  }, [
    appSettings.integrations.notion.databaseId,
    centralHome,
    notionConflictStrategy,
    pushNotice,
    refreshCore,
    selectedRecordPath,
    t,
    withBusy,
  ])

  const handleSyncVisibleToNotion = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }

    const targets = displayedRecords
      .map((item) => item.jsonPath)
      .filter((item): item is string => Boolean(item))

    if (targets.length === 0) {
      pushNotice('error', t('No record json paths available in current view.', '目前視圖沒有可用的紀錄路徑。'))
      return
    }

    await withBusy(async () => {
      const result = await syncRecordsBidirectional({
        centralHome,
        jsonPaths: targets,
        databaseId: appSettings.integrations.notion.databaseId || undefined,
        conflictStrategy: notionConflictStrategy,
      })
      const data = await refreshCore(centralHome)
      setDisplayedRecords(data.records)
      setSearchMeta(null)
      setNotionSyncReport(JSON.stringify(result, null, 2))
      pushNotice(
        'success',
        t(
          `Notion batch sync done. success=${result.success}, failed=${result.failed}, conflicts=${result.conflicts}.`,
          `Notion 批次同步完成。成功=${result.success}，失敗=${result.failed}，衝突=${result.conflicts}。`,
        ),
      )
    })
  }, [
    appSettings.integrations.notion.databaseId,
    centralHome,
    displayedRecords,
    notionConflictStrategy,
    pushNotice,
    refreshCore,
    t,
    withBusy,
  ])

  const handlePullFromNotion = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }
    await withBusy(async () => {
      const result = await pullRecordsFromNotion({
        centralHome,
        databaseId: appSettings.integrations.notion.databaseId || undefined,
        conflictStrategy: notionConflictStrategy,
      })
      const data = await refreshCore(centralHome)
      setDisplayedRecords(data.records)
      setSearchMeta(null)
      setNotionSyncReport(JSON.stringify(result, null, 2))
      pushNotice(
        'success',
        t(
          `Pulled from Notion. success=${result.success}, failed=${result.failed}, conflicts=${result.conflicts}.`,
          `已從 Notion 拉取。成功=${result.success}，失敗=${result.failed}，衝突=${result.conflicts}。`,
        ),
      )
    })
  }, [
    appSettings.integrations.notion.databaseId,
    centralHome,
    notionConflictStrategy,
    pushNotice,
    refreshCore,
    t,
    withBusy,
  ])

  const handleNotebookHealth = useCallback(async () => {
    await withBusy(async () => {
      const result = await notebooklmHealthCheck()
      setNotebookHealthText(JSON.stringify(result, null, 2))
      pushNotice('success', t('NotebookLM health checked.', 'NotebookLM 健康檢查完成。'))
    })
  }, [pushNotice, t, withBusy])

  const handleNotebookList = useCallback(async () => {
    await withBusy(async () => {
      const notebooks = await notebooklmListNotebooks({ limit: 30 })
      setNotebookList(notebooks)
      if (!selectedNotebookId && notebooks.length > 0) {
        setSelectedNotebookId(notebooks[0].id)
      }
      pushNotice('success', t(`Loaded ${notebooks.length} notebooks.`, `已載入 ${notebooks.length} 個筆記本。`))
    })
  }, [pushNotice, selectedNotebookId, t, withBusy])

  const handleNotebookCreate = useCallback(async () => {
    await withBusy(async () => {
      const created = await notebooklmCreateNotebook({
        title: newNotebookTitle.trim() || undefined,
      })
      const notebooks = await notebooklmListNotebooks({ limit: 30 })
      setNotebookList(notebooks)
      setSelectedNotebookId(created.id)
      setNewNotebookTitle('')

      const nextSettings: AppSettings = {
        ...appSettings,
        integrations: {
          ...appSettings.integrations,
          notebooklm: {
            ...appSettings.integrations.notebooklm,
            defaultNotebookId: created.id,
          },
        },
      }
      setAppSettings(nextSettings)
      const saved = await saveAppSettings(nextSettings)
      setAppSettings(saved)
      pushNotice('success', t(`Notebook created: ${created.name}`, `已建立筆記本：${created.name}`))
    })
  }, [appSettings, newNotebookTitle, pushNotice, t, withBusy])

  const handleNotebookSetDefault = useCallback(async () => {
    if (!selectedNotebookId) {
      pushNotice('error', t('Select a notebook first.', '請先選擇一個筆記本。'))
      return
    }
    const nextSettings: AppSettings = {
      ...appSettings,
      integrations: {
        ...appSettings.integrations,
        notebooklm: {
          ...appSettings.integrations.notebooklm,
          defaultNotebookId: selectedNotebookId,
        },
      },
    }
    await withBusy(async () => {
      const saved = await saveAppSettings(nextSettings)
      setAppSettings(saved)
      pushNotice('success', t('Default NotebookLM notebook updated.', 'NotebookLM 預設筆記本已更新。'))
    })
  }, [appSettings, pushNotice, selectedNotebookId, t, withBusy])

  const handleAddSelectedRecordToNotebook = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }
    if (!selectedRecord?.jsonPath) {
      pushNotice('error', t('Select a record first.', '請先選擇一筆紀錄。'))
      return
    }
    if (!selectedNotebookId) {
      pushNotice('error', t('Select a notebook first.', '請先選擇一個筆記本。'))
      return
    }

    await withBusy(async () => {
      const jsonPath = selectedRecord.jsonPath
      if (!jsonPath) {
        pushNotice('error', t('Selected record has no jsonPath.', '選取紀錄缺少 jsonPath。'))
        return
      }
      await notebooklmAddRecordSource({
        centralHome,
        jsonPath,
        notebookId: selectedNotebookId,
      })
      pushNotice('success', t('Selected record sent to NotebookLM as text source.', '已將選取紀錄送到 NotebookLM 作為文字來源。'))
    })
  }, [centralHome, pushNotice, selectedNotebookId, selectedRecord, t, withBusy])

  const handleNotebookAsk = useCallback(async () => {
    if (!selectedNotebookId) {
      pushNotice('error', t('Select a notebook first.', '請先選擇一個筆記本。'))
      return
    }
    if (!notebookQuestion.trim()) {
      pushNotice('error', t('Question cannot be empty.', '問題不可為空。'))
      return
    }

    await withBusy(async () => {
      const result = await notebooklmAsk({
        notebookId: selectedNotebookId,
        question: notebookQuestion.trim(),
        includeCitations: true,
      })
      setNotebookAnswer(result.answer)
      setNotebookCitations(result.citations)
      pushNotice('success', t('NotebookLM answered.', 'NotebookLM 已回覆。'))
    })
  }, [notebookQuestion, pushNotice, selectedNotebookId, t, withBusy])

  const handleRebuildIndex = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }

    await withBusy(async () => {
      const result = await rebuildSearchIndex(centralHome)
      pushNotice(
        'success',
        t(
          `Indexed ${result.indexedCount} records in ${result.tookMs} ms.`,
          `已建立 ${result.indexedCount} 筆紀錄索引，耗時 ${result.tookMs} ms。`,
        ),
      )
      setHealth(await getHealthDiagnostics(centralHome))
    })
  }, [centralHome, pushNotice, t, withBusy])

  const handleExportReport = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }

    await withBusy(async () => {
      const result = await exportMarkdownReport({
        centralHome,
        outputPath: reportPath.trim() || undefined,
        title: reportTitle.trim() || undefined,
        recentDays: reportDays,
      })
      pushNotice('success', t(`Report exported: ${result.outputPath}`, `報告已匯出：${result.outputPath}`))
    })
  }, [centralHome, pushNotice, reportDays, reportPath, reportTitle, t, withBusy])

  const handleSaveSettings = useCallback(async (next: AppSettings) => {
    await withBusy(async () => {
      const saved = await saveAppSettings(next)
      setAppSettings(saved)
      if (saved.activeProfileId) {
        setSelectedProfileId(saved.activeProfileId)
      }
      pushNotice('success', t('Settings saved.', '設定已儲存。'))
    })
  }, [pushNotice, t, withBusy])

  const updateProviderRegistryEntry = useCallback((providerId: string, patch: Partial<DebateProviderConfig>) => {
    setAppSettings((prev) => {
      const normalized = buildProviderRegistrySettings(prev.providerRegistry)
      const providers = normalized.providers.map((item) => {
        if (item.id !== providerId) {
          return item
        }
        const nextCapabilities =
          patch.capabilities && patch.capabilities.length > 0
            ? patch.capabilities
            : patch.capabilities
              ? ['debate']
              : item.capabilities
        return {
          ...item,
          ...patch,
          capabilities: nextCapabilities,
        }
      })

      return {
        ...prev,
        providerRegistry: {
          providers,
        },
      }
    })
  }, [])

  const handleResetProviderRegistryDraft = useCallback(() => {
    setAppSettings((prev) => ({
      ...prev,
      providerRegistry: buildProviderRegistrySettings(),
    }))
    pushNotice(
      'info',
      t(
        'Provider defaults restored. Click "Save Provider Registry" to persist.',
        '已還原 Provider 預設值，請按「儲存 Provider 設定」完成寫入。',
      ),
    )
  }, [pushNotice, t])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase()
      const mod = event.metaKey || event.ctrlKey

      if (mod && key === 'k') {
        event.preventDefault()
        setCommandOpen(true)
        setTimeout(() => commandInputRef.current?.focus(), 20)
      }

      if (mod && key === 's' && activeTab === 'records') {
        event.preventDefault()
        void handleSaveRecord()
      }

      if (mod && /^[1-7]$/.test(key)) {
        const index = Number(key) - 1
        const nextTab = TAB_ITEMS[index]
        if (nextTab) {
          event.preventDefault()
          setActiveTab(nextTab)
        }
      }

      if (key === 'escape' && commandOpen) {
        setCommandOpen(false)
      }
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [activeTab, commandOpen, handleSaveRecord])

  const commandItems = useMemo(
    () => [
      {
        id: 'cmd-load-home',
        label: t('Load Central Home', '載入中央路徑'),
        run: () => void loadCentralHome(),
      },
      {
        id: 'cmd-choose-home',
        label: t('Choose Central Home Folder', '選擇中央路徑資料夾'),
        run: () => void handlePickCentralHome(),
      },
      {
        id: 'cmd-refresh',
        label: t('Refresh Data', '刷新資料'),
        run: () => {
          if (centralHome) {
            void withBusy(async () => {
              const data = await refreshCore(centralHome)
              setDisplayedRecords(data.records)
              setSearchMeta(null)
              pushNotice('success', t('Data refreshed.', '資料已刷新。'))
            })
          }
        },
      },
      {
        id: 'cmd-new-record',
        label: t('New Record', '新增紀錄'),
        run: () => {
          setSelectedRecordPath(null)
          setRecordForm(emptyForm())
          setActiveTab('records')
        },
      },
      {
        id: 'cmd-save-record',
        label: t('Save Record', '儲存紀錄'),
        run: () => void handleSaveRecord(),
      },
      {
        id: 'cmd-sync-record-notion',
        label: t('Bidirectional Sync Selected Record', '雙向同步選取紀錄'),
        run: () => void handleSyncSelectedToNotion(),
      },
      {
        id: 'cmd-pull-notion',
        label: t('Pull Latest from Notion Database', '從 Notion 拉取最新資料'),
        run: () => void handlePullFromNotion(),
      },
      {
        id: 'cmd-rebuild-index',
        label: t('Rebuild Search Index', '重建搜尋索引'),
        run: () => void handleRebuildIndex(),
      },
      {
        id: 'cmd-run-local-ai',
        label: t('Run Local AI Analysis', '執行本地 AI 分析'),
        run: () => {
          setAiProvider('local')
          setActiveTab('ai')
          void handleRunAi()
        },
      },
      {
        id: 'cmd-export-report',
        label: t('Export Markdown Report', '匯出 Markdown 報告'),
        run: () => void handleExportReport(),
      },
      {
        id: 'cmd-list-notebooks',
        label: t('Refresh NotebookLM List', '刷新 NotebookLM 清單'),
        run: () => void handleNotebookList(),
      },
      ...TAB_ITEMS.map((item) => ({
        id: `cmd-tab-${item}`,
        label: `${t('Go to', '前往')} ${tabLabel(item)}`,
        run: () => setActiveTab(item),
      })),
    ],
    [
      centralHome,
      handleExportReport,
      handlePickCentralHome,
      handleNotebookList,
      handlePullFromNotion,
      handleRebuildIndex,
      handleRunAi,
      handleSaveRecord,
      handleSyncSelectedToNotion,
      loadCentralHome,
      pushNotice,
      refreshCore,
      t,
      tabLabel,
      withBusy,
    ],
  )

  const filteredCommands = useMemo(() => {
    const q = commandQuery.trim().toLowerCase()
    if (!q) {
      return commandItems
    }
    return commandItems.filter((item) => item.label.toLowerCase().includes(q))
  }, [commandItems, commandQuery])

  function runCommand(id: string) {
    const found = commandItems.find((item) => item.id === id)
    if (!found) {
      return
    }
    setCommandOpen(false)
    setCommandQuery('')
    found.run()
  }

  function renderDashboard() {
    if (!stats) {
      return <div className="panel dashboard-empty">{t('Load a Central Home to see metrics.', '請先載入中央記錄路徑以查看儀表板。')}</div>
    }

    const maxTypeCount = Math.max(1, ...RECORD_TYPES.map((item) => stats.typeCounts[item] ?? 0))
    const maxDailyCount = Math.max(1, ...stats.recentDailyCounts.map((row) => row.count))

    const dominantType = RECORD_TYPES.reduce<{ type: RecordType; count: number }>(
      (acc, item) => {
        const count = stats.typeCounts[item] ?? 0
        if (count > acc.count) {
          return { type: item, count }
        }
        return acc
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

    const focusedLinkNodeIds = dashboardFocusedNodeId
      ? dashboardResolvedLinks.reduce<string[]>((acc, link) => {
          if (link.sourceId === dashboardFocusedNodeId) {
            acc.push(link.targetId)
          } else if (link.targetId === dashboardFocusedNodeId) {
            acc.push(link.sourceId)
          }
          return acc
        }, [])
      : []

    const focusedLinkedNodes = [...new Set(focusedLinkNodeIds)]
      .map((id) => dashboardGraphNodes.find((node) => node.id === id))
      .filter((node): node is DashboardGraphNode => Boolean(node))
      .slice(0, 8)

    const focusedRecordCount = (() => {
      if (!dashboardFocusedNode) {
        return allRecords.length
      }
      if (dashboardFocusedNode.kind === 'core') {
        return allRecords.length
      }
      if (dashboardFocusedNode.kind === 'type' && dashboardFocusedNode.recordType) {
        return allRecords.filter((item) => item.recordType === dashboardFocusedNode.recordType).length
      }
      if (dashboardFocusedNode.kind === 'tag' && dashboardFocusedNode.tag) {
        return allRecords.filter((item) => item.tags.includes(dashboardFocusedNode.tag || '')).length
      }
      if (dashboardFocusedNode.kind === 'record') {
        return 1
      }
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
            <p className="kpi-sub">
              {t(`${stats.pendingSyncCount} pending sync`, `${stats.pendingSyncCount} 筆待同步`)}
            </p>
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
                ref={dashboardGraphSvgRef}
                className="dashboard-force-svg"
                viewBox={`0 0 ${DASHBOARD_GRAPH_WIDTH} ${DASHBOARD_GRAPH_HEIGHT}`}
                role="img"
                aria-label={t('Knowledge graph visualization', '知識圖譜視覺化')}
                onPointerMove={handleDashboardGraphPointerMove}
                onPointerUp={releaseDashboardDrag}
                onPointerLeave={releaseDashboardDrag}
                onPointerCancel={releaseDashboardDrag}
                onClick={() => setDashboardFocusedNodeId(null)}
              >
                <defs>
                  <radialGradient id="graphGlow" cx="50%" cy="50%" r="60%">
                    <stop offset="0%" stopColor="rgba(98, 222, 255, 0.34)" />
                    <stop offset="100%" stopColor="rgba(12, 19, 40, 0)" />
                  </radialGradient>
                </defs>
                <rect x={0} y={0} width={DASHBOARD_GRAPH_WIDTH} height={DASHBOARD_GRAPH_HEIGHT} fill="url(#graphGlow)" />

                {dashboardResolvedLinks.map((link) => {
                  const sourceNode = dashboardGraphNodes.find((node) => node.id === link.sourceId)
                  const targetNode = dashboardGraphNodes.find((node) => node.id === link.targetId)
                  if (!sourceNode || !targetNode) {
                    return null
                  }
                  const active =
                    !dashboardFocusedNodeId ||
                    link.sourceId === dashboardFocusedNodeId ||
                    link.targetId === dashboardFocusedNodeId
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

                {dashboardGraphNodes.map((node) => {
                  const x = node.x ?? DASHBOARD_GRAPH_WIDTH / 2
                  const y = node.y ?? DASHBOARD_GRAPH_HEIGHT / 2
                  const focused = dashboardFocusedNodeId === node.id
                  const dimmed = Boolean(dashboardFocusedNodeId) && !dashboardFocusNeighbors.has(node.id)
                  return (
                    <g
                      key={node.id}
                      className={`graph-node kind-${node.kind}${focused ? ' focused' : ''}${dimmed ? ' dimmed' : ''}`}
                      transform={`translate(${x}, ${y})`}
                      onPointerDown={(event) => handleDashboardNodePointerDown(node.id, event)}
                      onClick={(event) => {
                        event.stopPropagation()
                        handleDashboardGraphNodeActivate(node)
                      }}
                    >
                      <circle
                        r={node.radius}
                        style={{
                          fill: node.color,
                        }}
                      />
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
                <h4>{dashboardFocusedNode ? dashboardFocusedNode.label : t('All Nodes', '所有節點')}</h4>
                <p className="muted">
                  {dashboardFocusedNode
                    ? t(
                        `${dashboardFocusedNode.kind} · ${focusedRecordCount} related records`,
                        `${dashboardFocusedNode.kind} · 關聯 ${focusedRecordCount} 筆紀錄`,
                      )
                    : t('Click a node to inspect and route to Records.', '點擊節點可查看詳情並跳轉到紀錄篩選。')}
                </p>
                {dashboardFocusedNode && (
                  <button
                    type="button"
                    className="ghost-btn"
                    onClick={() => handleDashboardGraphNodeActivate(dashboardFocusedNode)}
                  >
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
                        <button type="button" onClick={() => handleDashboardGraphNodeActivate(node)}>
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
                  onClick={() => {
                    setRecordFilterType('all')
                    setRecordKeyword(item.tag)
                    setActiveTab('records')
                  }}
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

  function renderRecords() {
    const selectedTagCount = recordForm.tagsText
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean).length
    const pendingCount = allRecords.filter((item) => item.notionSyncStatus === 'PENDING').length
    const typeNodes = RECORD_TYPES.map((type, index) => {
      const angle = (Math.PI * 2 * index) / RECORD_TYPES.length - Math.PI / 2
      const count = stats?.typeCounts[type] ?? displayedRecords.filter((item) => item.recordType === type).length
      return {
        type,
        count,
        x: 50 + Math.cos(angle) * 26,
        y: 50 + Math.sin(angle) * 24,
      }
    })
    const tagCounts = new Map<string, number>()
    for (const record of displayedRecords) {
      for (const tag of record.tags) {
        if (!tag) {
          continue
        }
        tagCounts.set(tag, (tagCounts.get(tag) ?? 0) + 1)
      }
    }
    const tagNodes = [...tagCounts.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 16)
      .map(([tag, count], index, source) => {
        const angle = (Math.PI * 2 * index) / Math.max(1, source.length) - Math.PI / 2
        const ring = index % 2 === 0 ? 41 : 33
        const anchorType = RECORD_TYPES[index % RECORD_TYPES.length]
        return {
          tag,
          count,
          anchorType,
          x: 50 + Math.cos(angle) * ring,
          y: 50 + Math.sin(angle) * ring,
        }
      })

    return (
      <div className="page-stack">
        <div className="panel section-hero records-hero">
          <div className="section-hero-content">
            <p className="eyebrow">{t('records.hero.eyebrow')}</p>
            <h3>{t('records.hero.title')}</h3>
            <p className="muted">{t('records.hero.desc')}</p>
          </div>
          <div className="section-hero-stats">
            <div className="hero-metric">
              <span>{t('records.hero.inView')}</span>
              <strong>{displayedRecords.length}</strong>
              <small>{t('records.hero.filteredRecords')}</small>
            </div>
            <div className="hero-metric">
              <span>{t('records.hero.pending')}</span>
              <strong>{pendingCount}</strong>
              <small>{t('records.hero.needSync')}</small>
            </div>
            <div className="hero-metric">
              <span>{t('records.hero.tags')}</span>
              <strong>{selectedTagCount}</strong>
              <small>{t('records.hero.commaTags')}</small>
            </div>
          </div>
        </div>

        <div className="panel panel-strong records-constellation-panel">
          <div className="panel-head-inline">
            <h3>{t('records.constellation.title')}</h3>
            <span className="muted">{t('records.constellation.subtitle')}</span>
          </div>
          <p className="constellation-hint">{t('records.constellation.hint')}</p>
          <div className="records-constellation">
            <div className="constellation-ring ring-a" />
            <div className="constellation-ring ring-b" />
            <button
              type="button"
              className="const-node core-node"
              onClick={() => {
                setRecordKeyword('')
                setRecordFilterType('all')
              }}
            >
              {t('records.constellation.core')}
            </button>
            {typeNodes.map((node) => (
              <button
                type="button"
                key={node.type}
                className="const-node type-node"
                style={{
                  left: `${node.x}%`,
                  top: `${node.y}%`,
                  borderColor: TYPE_COLORS[node.type],
                  boxShadow: `0 0 16px ${TYPE_COLORS[node.type]}55`,
                }}
                onClick={() => setRecordFilterType(node.type)}
              >
                <span>{node.type}</span>
                <strong>{node.count}</strong>
              </button>
            ))}
            {tagNodes.map((node) => (
              <button
                type="button"
                key={node.tag}
                className="const-node tag-node"
                style={{
                  left: `${node.x}%`,
                  top: `${node.y}%`,
                  borderColor: `${TYPE_COLORS[node.anchorType]}99`,
                }}
                onClick={() => setRecordKeyword(node.tag)}
              >
                <span>{node.tag}</span>
                <strong>{node.count}</strong>
              </button>
            ))}
          </div>
        </div>

        <div className="records-layout">
          <div className="panel left-panel records-left-panel">
            <div className="panel-head-inline">
              <h3>{t('records.panel.listTitle')}</h3>
              <span className="muted">{t('records.panel.listSub')}</span>
            </div>
            <div className="toolbar-row">
              <button
                type="button"
                onClick={() => {
                  setSelectedRecordPath(null)
                  setRecordForm(emptyForm())
                }}
              >
                {t('common.new')}
              </button>
              <button type="button" onClick={() => void handleSaveRecord()} disabled={busy}>
                {t('common.save')}
              </button>
              <button type="button" onClick={() => void handleDeleteRecord()} disabled={busy || !selectedRecordPath}>
                {t('common.delete')}
              </button>
            </div>

            <div className="records-filter-grid">
              <select
                value={recordFilterType}
                onChange={(event) => setRecordFilterType(event.target.value as 'all' | RecordType)}
              >
                <option value="all">{t('records.filter.all')}</option>
                {RECORD_TYPES.map((item) => (
                  <option key={item} value={item}>
                    {item}
                  </option>
                ))}
              </select>
              <input
                placeholder={t('records.filter.keyword')}
                value={recordKeyword}
                onChange={(event) => setRecordKeyword(event.target.value)}
              />
              <input type="date" value={recordDateFrom} onChange={(event) => setRecordDateFrom(event.target.value)} />
              <input type="date" value={recordDateTo} onChange={(event) => setRecordDateTo(event.target.value)} />
            </div>

            <div className="meta-row">
              <span>
                {searchMeta
                  ? t('records.meta.result', {
                      shown: displayedRecords.length,
                      total: searchMeta.total,
                      mode: searchMeta.indexed ? t('records.meta.mode.fts') : t('records.meta.mode.memory'),
                      took: searchMeta.tookMs,
                    })
                  : t('records.meta.total', { count: displayedRecords.length })}
              </span>
            </div>

            <div className="record-list">
              {visibleRecords.map((item) => {
                const selected = item.jsonPath === selectedRecordPath
                const snippet = item.jsonPath ? searchMeta?.snippets[item.jsonPath] : undefined
                return (
                  <button
                    type="button"
                    key={item.jsonPath ?? `${item.createdAt}-${item.title}`}
                    className={selected ? 'record-item selected' : 'record-item'}
                    onClick={() => {
                      setSelectedRecordPath(item.jsonPath ?? null)
                      setRecordForm(formFromRecord(item))
                    }}
                  >
                    <p>{item.createdAt.slice(0, 19)}</p>
                    <p>
                      <strong>{item.recordType}</strong> | {item.title}
                    </p>
                    {snippet ? (
                      <p className="search-snippet" dangerouslySetInnerHTML={{ __html: snippet }} />
                    ) : null}
                  </button>
                )
              })}
              {displayedRecords.length === 0 && <p className="muted">{t('common.noRecords')}</p>}
            </div>

            {visibleCount < displayedRecords.length && (
              <button
                type="button"
                className="ghost-btn"
                onClick={() => setVisibleCount((prev) => prev + 200)}
              >
                {t('records.loadMore', { remain: displayedRecords.length - visibleCount })}
              </button>
            )}
          </div>

          <div className="panel right-panel records-editor-panel">
            <div className="panel-head-inline">
              <h3>{t('records.panel.editorTitle')}</h3>
              <span className="muted">{t('records.panel.editorSub')}</span>
            </div>
            <div className="form-grid">
              <label>
                {t('records.field.type')}
                <select
                  value={recordForm.recordType}
                  onChange={(event) =>
                    setRecordForm((prev) => ({ ...prev, recordType: event.target.value as RecordType }))
                  }
                >
                  {RECORD_TYPES.map((item) => (
                    <option key={item} value={item}>
                      {item}
                    </option>
                  ))}
                </select>
              </label>

              <label>
                {t('records.field.title')}
                <input
                  value={recordForm.title}
                  onChange={(event) => setRecordForm((prev) => ({ ...prev, title: event.target.value }))}
                />
              </label>

              <label>
                {t('records.field.createdAt')}
                <input
                  value={recordForm.createdAt}
                  onChange={(event) => setRecordForm((prev) => ({ ...prev, createdAt: event.target.value }))}
                />
              </label>

              <label>
                {t('records.field.date')}
                <input
                  type="date"
                  value={recordForm.date}
                  onChange={(event) => setRecordForm((prev) => ({ ...prev, date: event.target.value }))}
                />
              </label>

              <label>
                {t('records.field.tags')}
                <input
                  value={recordForm.tagsText}
                  onChange={(event) => setRecordForm((prev) => ({ ...prev, tagsText: event.target.value }))}
                />
              </label>

              <label>
                {t('records.field.syncStatus')}
                <select
                  value={recordForm.notionSyncStatus}
                  onChange={(event) =>
                    setRecordForm((prev) => ({ ...prev, notionSyncStatus: event.target.value }))
                  }
                >
                  <option value="SUCCESS">{t('status.success')}</option>
                  <option value="PENDING">{t('status.pending')}</option>
                  <option value="FAILED">{t('status.failed')}</option>
                </select>
              </label>

              <label>
                {t('records.field.notionUrl')}
                <input
                  value={recordForm.notionUrl}
                  onChange={(event) => setRecordForm((prev) => ({ ...prev, notionUrl: event.target.value }))}
                />
              </label>

              <label>
                {t('records.field.notionPageId')}
                <input
                  value={recordForm.notionPageId}
                  onChange={(event) => setRecordForm((prev) => ({ ...prev, notionPageId: event.target.value }))}
                />
              </label>

              <label>
                {t('records.field.notionError')}
                <input
                  value={recordForm.notionError}
                  onChange={(event) => setRecordForm((prev) => ({ ...prev, notionError: event.target.value }))}
                />
              </label>
            </div>

            <label className="block-field">
              {t('records.field.finalBody')}
              <textarea
                value={recordForm.finalBody}
                rows={12}
                onChange={(event) => setRecordForm((prev) => ({ ...prev, finalBody: event.target.value }))}
              />
            </label>

            <label className="block-field">
              {t('records.field.sourceText')}
              <textarea
                value={recordForm.sourceText}
                rows={8}
                onChange={(event) => setRecordForm((prev) => ({ ...prev, sourceText: event.target.value }))}
              />
            </label>
          </div>
        </div>
      </div>
    )
  }

  function renderLogs() {
    const failedCount = logs.filter((item) => (item.status || '').toLowerCase().includes('fail')).length
    const successCount = logs.filter((item) => {
      const status = (item.status || '').toLowerCase()
      return status.includes('success') || status.includes('done')
    }).length
    const pulse = buildLogPulse(logs)
    const pulseMax = Math.max(1, ...pulse.map((item) => item.count))

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

  function renderAi() {
    const aiWordCount = aiResult.trim() ? aiResult.trim().split(/\s+/).filter(Boolean).length : 0
    const insights = extractAiInsights(aiResult)
    const debateConsensus = debateResult?.finalPacket.consensus
    const debateActions = debateResult?.finalPacket.nextActions ?? []
    const debateErrorCodes = debateResult?.errorCodes ?? []
    const replayIssues = debateReplayResult?.consistency.issues ?? []
    const isCliDebateProvider =
      debateProvider === 'codex-cli' || debateProvider === 'gemini-cli' || debateProvider === 'claude-cli'
    const debateProviderRuntimeHint =
      debateProvider === 'codex-cli'
        ? t('Provider runtime: codex exec (live).', 'Provider 執行模式：codex exec（即時）。')
        : debateProvider === 'gemini-cli'
          ? t('Provider runtime: gemini CLI one-shot (live).', 'Provider 執行模式：gemini CLI 單次執行（即時）。')
          : debateProvider === 'claude-cli'
            ? t('Provider runtime: claude CLI one-shot (live).', 'Provider 執行模式：claude CLI 單次執行（即時）。')
        : debateProvider === 'local'
          ? t('Provider runtime: local heuristic.', 'Provider 執行模式：本地 heuristic。')
          : t(
              'Provider runtime: local fallback stub (automation not wired yet).',
              'Provider 執行模式：本地 fallback stub（尚未接自動化執行）。',
            )
    const debateModelHint = isCliDebateProvider
      ? t(
          'Model is optional for CLI providers. Leave blank to use your CLI/account default.',
          'CLI provider 的模型欄位可留空；留空會使用你 CLI/帳號的預設模型。',
        )
      : t(
          'Model can be left blank to use provider default.',
          '模型欄位可留空，系統會用 provider 預設值。',
        )
    const cards: Array<{ id: 'summary' | 'risks' | 'actions'; title: string; items: string[]; empty: string }> = [
      {
        id: 'summary',
        title: t('ai.insight.summary'),
        items: insights.summary,
        empty: t('ai.insight.empty.summary'),
      },
      {
        id: 'risks',
        title: t('ai.insight.risks'),
        items: insights.risks,
        empty: t('ai.insight.empty.risks'),
      },
      {
        id: 'actions',
        title: t('ai.insight.actions'),
        items: insights.actions,
        empty: t('ai.insight.empty.actions'),
      },
    ]

    return (
      <div className="page-stack">
        <div className="panel section-hero ai-hero">
          <div className="section-hero-content">
            <p className="eyebrow">{t('ai.hero.eyebrow')}</p>
            <h3>{t('ai.hero.title')}</h3>
            <p className="muted">{t('ai.hero.desc')}</p>
          </div>
          <div className="section-hero-stats">
            <div className="hero-metric">
              <span>{t('common.provider')}</span>
              <strong>{aiProvider}</strong>
              <small>{aiModel}</small>
            </div>
            <div className="hero-metric">
              <span>{t('ai.hero.recordsWindow')}</span>
              <strong>{aiMaxRecords}</strong>
              <small>{aiIncludeLogs ? t('ai.hero.logsIncluded') : t('ai.hero.recordsOnly')}</small>
            </div>
            <div className="hero-metric">
              <span>{t('ai.hero.outputSize')}</span>
              <strong>{aiWordCount}</strong>
              <small>{t('ai.hero.words')}</small>
            </div>
          </div>
        </div>

        <div className="panel">
          <div className="panel-head-inline">
            <h3>{t('Debate History', '辯論歷史')}</h3>
            <button
              type="button"
              className="ghost-btn"
              onClick={() => void refreshDebateRuns(centralHome)}
              disabled={!centralHome || busy || debateBusy}
            >
              {t('Refresh History', '刷新歷史')}
            </button>
          </div>
          <div className="record-list">
            {debateRuns.map((item) => {
              const selected = debateRunId === item.runId
              const shortProblem =
                item.problem.length > 60 ? `${item.problem.slice(0, 60).trimEnd()}...` : item.problem
              return (
                <button
                  type="button"
                  key={item.runId}
                  className={selected ? 'record-item selected' : 'record-item'}
                  onClick={() => void handleSelectDebateRun(item.runId)}
                >
                  <span className="record-meta">
                    <strong>{item.runId}</strong>
                    <small>
                      {item.provider} · {item.outputType} · {item.createdAt}
                      {item.degraded ? ` · ${t('degraded', '降級')}` : ''}
                    </small>
                  </span>
                  <span className="record-title">{shortProblem || '-'}</span>
                </button>
              )
            })}
            {debateRuns.length === 0 ? (
              <p className="muted">{t('No debate history found.', '尚無辯論歷史。')}</p>
            ) : null}
          </div>

          <div className="panel-head-inline">
            <h3>{t('Debate Mode v0.1', '辯論模式 v0.1')}</h3>
            <span className="muted">{t('Run fixed 5-role / 3-round protocol.', '執行固定 5 角色 / 3 回合流程。')}</span>
          </div>

          <div className="form-grid two-col-grid">
            <label className="span-2">
              {t('Problem', '問題')}
              <textarea value={debateProblem} rows={4} onChange={(event) => setDebateProblem(event.target.value)} />
            </label>

            <label className="span-2">
              {t('Constraints (line or comma separated)', '約束（每行或逗號分隔）')}
              <textarea
                value={debateConstraintsText}
                rows={4}
                onChange={(event) => setDebateConstraintsText(event.target.value)}
              />
            </label>

            <label>
              {t('Output Type', '輸出類型')}
              <select
                value={debateOutputType}
                onChange={(event) => setDebateOutputType(event.target.value as DebateOutputType)}
              >
                <option value="decision">decision</option>
                <option value="writing">writing</option>
                <option value="architecture">architecture</option>
                <option value="planning">planning</option>
                <option value="evaluation">evaluation</option>
              </select>
            </label>

            <label>
              {t('Provider (all fixed roles)', 'Provider（套用到所有角色）')}
              <select value={debateProvider} onChange={(event) => setDebateProvider(event.target.value)}>
                {debateProviderOptions.map((providerId) => (
                  <option key={providerId} value={providerId}>
                    {debateProviderLabel(providerId)}
                  </option>
                ))}
              </select>
              <small className="muted">{debateProviderRuntimeHint}</small>
            </label>

            <label className="checkbox-field span-2">
              <input
                type="checkbox"
                checked={debateAdvancedMode}
                onChange={(event) => setDebateAdvancedMode(event.target.checked)}
              />
              {t('Advanced: per-role provider', '進階：每角色個別 Provider')}
            </label>

            <label>
              {t('Model', '模型')}
              <input
                value={debateModel}
                placeholder={debateModelDefault}
                onChange={(event) => setDebateModel(event.target.value)}
              />
              <small className="muted">{debateModelHint}</small>
            </label>

            {debateAdvancedMode
              ? DEBATE_ROLES.map((role) => (
                  <div key={role} className="span-2 form-grid two-col-grid">
                    <label>
                      {role}
                      <select
                        value={debatePerRoleProvider[role] || debateProvider}
                        onChange={(event) =>
                          setDebatePerRoleProvider((prev) => ({ ...prev, [role]: event.target.value }))
                        }
                      >
                        {debateProviderOptions.map((providerId) => (
                          <option key={providerId} value={providerId}>
                            {debateProviderLabel(providerId)}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      {t('Model', '模型')}
                      <input
                        value={debatePerRoleModel[role] || ''}
                        placeholder={debateModelDefault}
                        onChange={(event) =>
                          setDebatePerRoleModel((prev) => ({ ...prev, [role]: event.target.value }))
                        }
                      />
                    </label>
                  </div>
                ))
              : null}

            <label>
              {t('Writeback Type', '寫回類型')}
              <select
                value={debateWritebackType}
                onChange={(event) => setDebateWritebackType(event.target.value as 'decision' | 'worklog')}
              >
                <option value="decision">decision</option>
                <option value="worklog">worklog</option>
              </select>
            </label>

            <label>
              {t('Max Turn Seconds', '單輪秒數上限')}
              <input
                type="number"
                min={5}
                max={120}
                value={debateMaxTurnSeconds}
                onChange={(event) => setDebateMaxTurnSeconds(Number(event.target.value) || 35)}
              />
            </label>

            <label>
              {t('Max Turn Tokens', '單輪 Token 上限')}
              <input
                type="number"
                min={128}
                max={4096}
                value={debateMaxTurnTokens}
                onChange={(event) => setDebateMaxTurnTokens(Number(event.target.value) || 900)}
              />
            </label>

            <label className="span-2">
              {t('Run ID (for replay)', 'Run ID（用於 replay）')}
              <input value={debateRunId} onChange={(event) => setDebateRunId(event.target.value)} />
            </label>
          </div>

          <div className="toolbar-row three-col">
            <button type="button" onClick={() => void handleRunDebate()} disabled={busy || debateBusy}>
              {t('Run Debate', '執行辯論')}
            </button>
            <button
              type="button"
              className="ghost-btn"
              onClick={() => void handleReplayDebate()}
              disabled={busy || debateBusy}
            >
              {t('Replay Run', '重播 Run')}
            </button>
            <button
              type="button"
              className="ghost-btn"
              onClick={() => navigator.clipboard.writeText(JSON.stringify(debateResult?.finalPacket ?? {}, null, 2))}
            >
              {t('Copy Final Packet', '複製 Final Packet')}
            </button>
          </div>
          {debateBusy && debateProgress ? (
            <p className="muted">
              {t(
                `${debateProgress.round} — ${debateProgress.role} (${debateProgress.turnIndex}/${debateProgress.totalTurns})`,
                `${debateProgress.round} — ${debateProgress.role} (${debateProgress.turnIndex}/${debateProgress.totalTurns})`,
              )}
            </p>
          ) : debateBusy ? (
            <p className="muted">{t('Debate is running in background...', '辯論正在背景執行中...')}</p>
          ) : null}

          <div className="kpi-grid">
            <div className="kpi-card">
              <span>{t('Last Run', '最近 Run')}</span>
              <strong>{debateResult?.runId || '-'}</strong>
            </div>
            <div className="kpi-card">
              <span>{t('Consensus', '共識分數')}</span>
              <strong>{debateConsensus ? debateConsensus.consensusScore.toFixed(3) : '-'}</strong>
            </div>
            <div className="kpi-card">
              <span>{t('Confidence', '信心分數')}</span>
              <strong>{debateConsensus ? debateConsensus.confidenceScore.toFixed(3) : '-'}</strong>
            </div>
            <div className="kpi-card">
              <span>{t('Next Actions', '下一步行動')}</span>
              <strong>{debateActions.length}</strong>
            </div>
          </div>

          {debateResult ? (
            <pre className="json-preview">
              {JSON.stringify(
                {
                  runId: debateResult.runId,
                  degraded: debateResult.degraded,
                  artifactsRoot: debateResult.artifactsRoot,
                  writebackJsonPath: debateResult.writebackJsonPath,
                },
                null,
                2,
              )}
            </pre>
          ) : null}
          {debateErrorCodes.length > 0 ? (
            <pre className="json-preview">{JSON.stringify({ errorCodes: debateErrorCodes }, null, 2)}</pre>
          ) : null}
          {replayIssues.length > 0 ? <pre className="json-preview">{JSON.stringify(replayIssues, null, 2)}</pre> : null}
        </div>

        <div className="ai-layout">
          <div className="panel ai-config-panel">
            <div className="panel-head-inline">
              <h3>{t('ai.setup.title')}</h3>
              <span className="muted">{t('ai.setup.subtitle')}</span>
            </div>

            <div className="ai-controls-grid">
              <label>
                {t('ai.field.provider')}
                <select value={aiProvider} onChange={(event) => setAiProvider(event.target.value as AiProvider)}>
                  <option value="local">local</option>
                  <option value="openai">openai</option>
                  <option value="gemini">gemini</option>
                  <option value="claude">claude</option>
                </select>
              </label>

              <label>
                {t('ai.field.model')}
                <input value={aiModel} onChange={(event) => setAiModel(event.target.value)} />
              </label>

              <label>
                {t('ai.field.maxRecords')}
                <input
                  type="number"
                  min={1}
                  max={200}
                  value={aiMaxRecords}
                  onChange={(event) => setAiMaxRecords(Number(event.target.value) || 30)}
                />
              </label>

              <label className="checkbox-field">
                <input
                  type="checkbox"
                  checked={aiIncludeLogs}
                  onChange={(event) => setAiIncludeLogs(event.target.checked)}
                />
                {t('ai.field.includeLogs')}
              </label>
            </div>

            <label className="block-field">
              {t('ai.field.prompt')}
              <textarea value={aiPrompt} rows={8} onChange={(event) => setAiPrompt(event.target.value)} />
            </label>

            <div className="toolbar-row two-col">
              <button type="button" onClick={() => void handleRunAi()} disabled={busy}>
                {t('ai.button.run')}
              </button>
              <button type="button" className="ghost-btn" onClick={() => navigator.clipboard.writeText(aiResult || '')}>
                {t('ai.button.copy')}
              </button>
            </div>
          </div>

          <div className="ai-right-stack">
            <div className="panel panel-strong ai-insights-panel">
              <div className="panel-head-inline">
                <h3>{t('ai.insight.title')}</h3>
                <span className="muted">{t('ai.insight.subtitle')}</span>
              </div>
              <div className="ai-insights-grid">
                {cards.map((card) => (
                  <div key={card.id} className={`ai-insight-card ${card.id}`}>
                    <h4>{card.title}</h4>
                    {card.items.length > 0 ? (
                      <ul className="insight-list">
                        {card.items.map((line, index) => (
                          <li key={`${card.id}-${index}`}>{line}</li>
                        ))}
                      </ul>
                    ) : (
                      <p className="muted">{card.empty}</p>
                    )}
                  </div>
                ))}
              </div>
            </div>

            <div className="panel ai-output-panel">
              <div className="panel-head-inline">
                <h3>{t('ai.output.title')}</h3>
                <span className="muted">{t('ai.output.subtitle', { count: aiWordCount })}</span>
              </div>
              <label className="block-field">
                <textarea value={aiResult} rows={22} onChange={(event) => setAiResult(event.target.value)} />
              </label>
            </div>
          </div>
        </div>
      </div>
    )
  }

  function renderIntegrations() {
    return (
      <div className="settings-layout">
        <div className="panel left-panel">
          <h3>{t('Notion Connector', 'Notion 連接器')}</h3>
          <div className="form-grid two-col-grid">
            <label className="span-2">
              {t('Notion Database ID', 'Notion 資料庫 ID')}
              <input
                value={appSettings.integrations.notion.databaseId}
                onChange={(event) =>
                  setAppSettings((prev) => ({
                    ...prev,
                    integrations: {
                      ...prev.integrations,
                      notion: {
                        ...prev.integrations.notion,
                        databaseId: event.target.value,
                      },
                    },
                  }))
                }
                placeholder="xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
              />
            </label>

            <label className="checkbox-field">
              <input
                type="checkbox"
                checked={appSettings.integrations.notion.enabled}
                onChange={(event) =>
                  setAppSettings((prev) => ({
                    ...prev,
                    integrations: {
                      ...prev.integrations,
                      notion: {
                        ...prev.integrations.notion,
                        enabled: event.target.checked,
                      },
                    },
                  }))
                }
              />
              {t('Enable Notion sync', '啟用 Notion 同步')}
            </label>

            <label>
              {t('Key Status', '金鑰狀態')}
              <input value={hasNotionKey ? t('configured', '已設定') : t('not set', '未設定')} readOnly />
            </label>
          </div>

          <div className="form-grid">
            <label>
              {t('Notion API Key (saved to Keychain)', 'Notion API 金鑰（儲存在 Keychain）')}
              <input
                type="password"
                value={notionKeyDraft}
                onChange={(event) => setNotionKeyDraft(event.target.value)}
                placeholder={hasNotionKey ? t('Key already configured', '金鑰已設定') : 'secret_...'}
              />
            </label>
          </div>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleSaveNotionKey()}>
              {t('Save Key', '儲存金鑰')}
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleClearNotionKey()}>
              {t('Clear Key', '清除金鑰')}
            </button>
          </div>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleSaveSettings(appSettings)} disabled={busy}>
              {t('Save Connector Settings', '儲存連接器設定')}
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handlePullFromNotion()} disabled={busy}>
              {t('Pull From Notion', '從 Notion 拉取')}
            </button>
          </div>

          <div className="form-grid">
            <label>
              {t('Conflict Strategy', '衝突策略')}
              <select
                value={notionConflictStrategy}
                onChange={(event) => setNotionConflictStrategy(event.target.value as NotionConflictStrategy)}
              >
                <option value="manual">{t('manual (mark conflict)', 'manual（標記衝突）')}</option>
                <option value="local_wins">{t('local_wins (push local)', 'local_wins（本地覆蓋）')}</option>
                <option value="notion_wins">{t('notion_wins (pull notion)', 'notion_wins（Notion 覆蓋）')}</option>
              </select>
            </label>
          </div>

          <div className="toolbar-row two-col">
            <button type="button" className="ghost-btn" onClick={() => void handleSyncSelectedToNotion()} disabled={busy}>
              {t('Bidirectional Sync Selected', '雙向同步選取項')}
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleSyncVisibleToNotion()} disabled={busy}>
              {t(`Bidirectional Sync View (${displayedRecords.length})`, `雙向同步目前視圖（${displayedRecords.length}）`)}
            </button>
          </div>

          {notionSyncReport && (
            <>
              <hr className="separator" />
              <h3>{t('Notion Sync Report', 'Notion 同步報告')}</h3>
              <pre className="json-preview">{notionSyncReport}</pre>
            </>
          )}
        </div>

        <div className="panel right-panel">
          <h3>{t('NotebookLM Connector', 'NotebookLM 連接器')}</h3>

          <div className="form-grid two-col-grid">
            <label>
              {t('MCP Command', 'MCP 指令')}
              <input
                value={appSettings.integrations.notebooklm.command}
                onChange={(event) =>
                  setAppSettings((prev) => ({
                    ...prev,
                    integrations: {
                      ...prev.integrations,
                      notebooklm: {
                        ...prev.integrations.notebooklm,
                        command: event.target.value,
                      },
                    },
                  }))
                }
              />
            </label>
            <label>
              {t('MCP Args (space separated)', 'MCP 參數（以空白分隔）')}
              <input
                value={appSettings.integrations.notebooklm.args.join(' ')}
                onChange={(event) =>
                  setAppSettings((prev) => ({
                    ...prev,
                    integrations: {
                      ...prev.integrations,
                      notebooklm: {
                        ...prev.integrations.notebooklm,
                        args: event.target.value
                          .split(' ')
                          .map((item) => item.trim())
                          .filter(Boolean),
                      },
                    },
                  }))
                }
              />
            </label>
          </div>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleNotebookHealth()} disabled={busy}>
              {t('Health Check', '健康檢查')}
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleNotebookList()} disabled={busy}>
              {t('Refresh Notebooks', '刷新筆記本')}
            </button>
          </div>

          <div className="form-grid two-col-grid">
            <label>
              {t('New Notebook Title', '新筆記本標題')}
              <input
                value={newNotebookTitle}
                onChange={(event) => setNewNotebookTitle(event.target.value)}
                placeholder={t('KOF Note - Weekly Analysis', 'KOF Note - 每週分析')}
              />
            </label>
            <label>
              {t('Selected Notebook', '已選筆記本')}
              <select value={selectedNotebookId} onChange={(event) => setSelectedNotebookId(event.target.value)}>
                <option value="">{t('(choose one)', '（選擇一個）')}</option>
                {notebookList.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
            </label>
          </div>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleNotebookCreate()} disabled={busy}>
              {t('Create Notebook', '建立筆記本')}
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleNotebookSetDefault()} disabled={busy}>
              {t('Set Default Notebook', '設為預設筆記本')}
            </button>
          </div>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleAddSelectedRecordToNotebook()} disabled={busy}>
              {t('Add Selected Record Source', '加入選取紀錄來源')}
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleSaveSettings(appSettings)} disabled={busy}>
              {t('Save MCP Config', '儲存 MCP 設定')}
            </button>
          </div>

          <label className="block-field">
            {t('Ask NotebookLM', '詢問 NotebookLM')}
            <textarea
              rows={5}
              value={notebookQuestion}
              onChange={(event) => setNotebookQuestion(event.target.value)}
            />
          </label>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleNotebookAsk()} disabled={busy}>
              {t('Ask', '詢問')}
            </button>
            <button
              type="button"
              className="ghost-btn"
              onClick={() => navigator.clipboard.writeText(notebookAnswer || '')}
            >
              {t('Copy Answer', '複製回答')}
            </button>
          </div>

          <label className="block-field">
            {t('NotebookLM Answer', 'NotebookLM 回答')}
            <textarea rows={10} value={notebookAnswer} onChange={(event) => setNotebookAnswer(event.target.value)} />
          </label>

          {notebookCitations.length > 0 && (
            <div className="panel">
              <h3>{t('Citations', '引用')}</h3>
              <ul className="simple-list">
                {notebookCitations.map((item, idx) => (
                  <li key={`${item}-${idx}`}>
                    <span>{item}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {notebookHealthText && (
            <>
              <hr className="separator" />
              <h3>{t('Health Output', '健康輸出')}</h3>
              <pre className="json-preview">{notebookHealthText}</pre>
            </>
          )}
        </div>
      </div>
    )
  }

  function renderSettings() {
    return (
      <div className="settings-layout">
        <div className="panel left-panel">
          <h3>{t('Profiles', '設定檔')}</h3>
          <div className="record-list">
            {appSettings.profiles.map((profile) => (
              <button
                key={profile.id}
                type="button"
                className={selectedProfileId === profile.id ? 'record-item selected' : 'record-item'}
                onClick={() => {
                  setSelectedProfileId(profile.id)
                  setProfileDraft(profile)
                }}
              >
                <p>
                  <strong>{profile.name}</strong>
                </p>
                <p>{profile.centralHome || '-'}</p>
              </button>
            ))}
            {appSettings.profiles.length === 0 && <p className="muted">{t('No profiles yet.', '尚無設定檔。')}</p>}
          </div>

          <div className="toolbar-row two-col">
            <button
              type="button"
              className="ghost-btn"
              onClick={() => {
                const next = makeProfile(t(`Profile ${appSettings.profiles.length + 1}`, `設定檔 ${appSettings.profiles.length + 1}`))
                setProfileDraft(next)
                setSelectedProfileId(next.id)
              }}
            >
              {t('New Profile', '新增設定檔')}
            </button>
            <button
              type="button"
              onClick={() => {
                if (!selectedProfile) {
                  return
                }
                setCentralHomeInput(selectedProfile.centralHome)
                if (selectedProfile.centralHome) {
                  void loadCentralHome(selectedProfile.centralHome)
                }
              }}
            >
              {t('Apply Profile', '套用設定檔')}
            </button>
          </div>
        </div>

        <div className="panel right-panel">
          <h3>{t('General Preferences', '一般偏好')}</h3>
          <div className="form-grid two-col-grid">
            <label>
              {t('UI Language', '介面語言')}
              <select
                value={language}
                onChange={(event) => {
                  const next = event.target.value
                  if (isSupportedLanguage(next)) {
                    setLanguage(next)
                  }
                }}
              >
                {SUPPORTED_LANGUAGES.map((code) => (
                  <option key={code} value={code}>
                    {languageName(code)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t('Current Language', '目前語言')}
              <input value={languageName(language)} readOnly />
            </label>
          </div>

          <hr className="separator" />

          <h3>{t('Profile Editor', '設定檔編輯')}</h3>

          <div className="form-grid two-col-grid">
            <label>
              {t('ID', 'ID')}
              <input value={profileDraft.id} readOnly />
            </label>
            <label>
              {t('Name', '名稱')}
              <input
                value={profileDraft.name}
                onChange={(event) =>
                  setProfileDraft((prev) => ({
                    ...prev,
                    name: event.target.value,
                  }))
                }
              />
            </label>
            <label>
              {t('Central Home', '中央路徑')}
              <input
                value={profileDraft.centralHome}
                onChange={(event) =>
                  setProfileDraft((prev) => ({
                    ...prev,
                    centralHome: event.target.value,
                  }))
                }
              />
            </label>
            <label>
              {t('Default Provider', '預設供應者')}
              <select
                value={profileDraft.defaultProvider}
                onChange={(event) =>
                  setProfileDraft((prev) => ({
                    ...prev,
                    defaultProvider: event.target.value,
                  }))
                }
              >
                <option value="local">local</option>
                <option value="openai">openai</option>
              </select>
            </label>
            <label>
              {t('Default Model', '預設模型')}
              <input
                value={profileDraft.defaultModel}
                onChange={(event) =>
                  setProfileDraft((prev) => ({
                    ...prev,
                    defaultModel: event.target.value,
                  }))
                }
              />
            </label>
            <label>
              {t('Poll Interval (sec)', '輪詢間隔（秒）')}
              <input
                type="number"
                min={3}
                max={120}
                value={appSettings.pollIntervalSec}
                onChange={(event) =>
                  setAppSettings((prev) => ({
                    ...prev,
                    pollIntervalSec: Number(event.target.value) || 8,
                  }))
                }
              />
            </label>
          </div>

          <div className="toolbar-row two-col">
            <button
              type="button"
              onClick={() => {
                const existingIndex = appSettings.profiles.findIndex((item) => item.id === profileDraft.id)
                const nextProfiles = [...appSettings.profiles]
                if (existingIndex >= 0) {
                  nextProfiles[existingIndex] = profileDraft
                } else {
                  nextProfiles.push(profileDraft)
                }

                const nextSettings: AppSettings = {
                  ...appSettings,
                  profiles: nextProfiles,
                  activeProfileId: profileDraft.id,
                }
                setAppSettings(nextSettings)
                void handleSaveSettings(nextSettings)
              }}
            >
              {t('Save Profile', '儲存設定檔')}
            </button>
            <button
              type="button"
              className="ghost-btn"
              onClick={() => {
                if (!selectedProfileId) {
                  return
                }
                const nextProfiles = appSettings.profiles.filter((item) => item.id !== selectedProfileId)
                const nextSettings: AppSettings = {
                  ...appSettings,
                  profiles: nextProfiles,
                  activeProfileId: nextProfiles[0]?.id ?? null,
                }
                setSelectedProfileId(nextProfiles[0]?.id ?? '')
                setProfileDraft(nextProfiles[0] ?? makeProfile())
                setAppSettings(nextSettings)
                void handleSaveSettings(nextSettings)
              }}
            >
              {t('Delete Profile', '刪除設定檔')}
            </button>
          </div>

          <hr className="separator" />

          <h3>{t('Debate Provider Registry', 'Debate Provider 註冊表')}</h3>
          <p className="muted">
            {t(
              'Configure CLI/Web providers available to Debate Mode (config layer only).',
              '設定 Debate Mode 可用的 CLI/Web provider（僅設定層）。',
            )}
          </p>

          <div className="provider-registry-grid">
            {appSettings.providerRegistry.providers.map((provider) => (
              <div key={provider.id} className="provider-registry-card">
                <div className="panel-head-inline">
                  <h4>{provider.id}</h4>
                  <span className="muted">{provider.type.toUpperCase()}</span>
                </div>

                <div className="form-grid two-col-grid">
                  <label>
                    {t('Type', '類型')}
                    <select
                      value={provider.type}
                      onChange={(event) =>
                        updateProviderRegistryEntry(provider.id, {
                          type: event.target.value === 'web' ? 'web' : 'cli',
                        })
                      }
                    >
                      <option value="cli">cli</option>
                      <option value="web">web</option>
                    </select>
                  </label>

                  <label className="checkbox-field provider-enabled-field">
                    <input
                      type="checkbox"
                      checked={provider.enabled}
                      onChange={(event) =>
                        updateProviderRegistryEntry(provider.id, {
                          enabled: event.target.checked,
                        })
                      }
                    />
                    <span>{t('Enabled', '啟用')}</span>
                  </label>
                </div>

                <label className="block-field">
                  {t('Capabilities (comma separated)', '能力（逗號分隔）')}
                  <input
                    value={provider.capabilities.join(', ')}
                    onChange={(event) =>
                      updateProviderRegistryEntry(provider.id, {
                        capabilities: parseProviderCapabilitiesInput(event.target.value),
                      })
                    }
                  />
                </label>
              </div>
            ))}
          </div>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleSaveSettings(appSettings)} disabled={busy}>
              {t('Save Provider Registry', '儲存 Provider 設定')}
            </button>
            <button type="button" className="ghost-btn" onClick={handleResetProviderRegistryDraft} disabled={busy}>
              {t('Reset Provider Defaults', '還原 Provider 預設')}
            </button>
          </div>

          <hr className="separator" />

          <h3>{t('AI Provider Keychain', 'AI 供應商金鑰管理')}</h3>

          <div className="panel panel-strong">
            <h3>{t('OpenAI Keychain', 'OpenAI 金鑰管理')}</h3>
            <div className="form-grid two-col-grid">
              <label>
                {t('API Key (saved to Keychain)', 'API 金鑰（儲存在 Keychain）')}
                <input
                  type="password"
                  value={openaiKeyDraft}
                  onChange={(event) => setOpenaiKeyDraft(event.target.value)}
                  placeholder={hasOpenaiKey ? t('Key already configured', '金鑰已設定') : 'sk-...'}
                />
              </label>
              <label>
                {t('Key Status', '金鑰狀態')}
                <input value={hasOpenaiKey ? t('configured', '已設定') : t('not set', '未設定')} readOnly />
              </label>
            </div>
            <div className="toolbar-row two-col">
              <button type="button" onClick={() => void handleSaveApiKey()}>
                {t('Save Key', '儲存金鑰')}
              </button>
              <button type="button" className="ghost-btn" onClick={() => void handleClearApiKey()}>
                {t('Clear Key', '清除金鑰')}
              </button>
            </div>
          </div>

          <div className="panel panel-strong">
            <h3>{t('Gemini Keychain', 'Gemini 金鑰管理')}</h3>
            <div className="form-grid two-col-grid">
              <label>
                {t('API Key (saved to Keychain)', 'API 金鑰（儲存在 Keychain）')}
                <input
                  type="password"
                  value={geminiKeyDraft}
                  onChange={(event) => setGeminiKeyDraft(event.target.value)}
                  placeholder={hasGeminiKey ? t('Key already configured', '金鑰已設定') : 'AIza...'}
                />
              </label>
              <label>
                {t('Key Status', '金鑰狀態')}
                <input value={hasGeminiKey ? t('configured', '已設定') : t('not set', '未設定')} readOnly />
              </label>
            </div>
            <div className="toolbar-row two-col">
              <button type="button" onClick={() => void handleSaveGeminiKey()}>
                {t('Save Key', '儲存金鑰')}
              </button>
              <button type="button" className="ghost-btn" onClick={() => void handleClearGeminiKey()}>
                {t('Clear Key', '清除金鑰')}
              </button>
            </div>
          </div>

          <div className="panel panel-strong">
            <h3>{t('Claude Keychain', 'Claude 金鑰管理')}</h3>
            <div className="form-grid two-col-grid">
              <label>
                {t('API Key (saved to Keychain)', 'API 金鑰（儲存在 Keychain）')}
                <input
                  type="password"
                  value={claudeKeyDraft}
                  onChange={(event) => setClaudeKeyDraft(event.target.value)}
                  placeholder={hasClaudeKey ? t('Key already configured', '金鑰已設定') : 'sk-ant-...'}
                />
              </label>
              <label>
                {t('Key Status', '金鑰狀態')}
                <input value={hasClaudeKey ? t('configured', '已設定') : t('not set', '未設定')} readOnly />
              </label>
            </div>
            <div className="toolbar-row two-col">
              <button type="button" onClick={() => void handleSaveClaudeKey()}>
                {t('Save Key', '儲存金鑰')}
              </button>
              <button type="button" className="ghost-btn" onClick={() => void handleClearClaudeKey()}>
                {t('Clear Key', '清除金鑰')}
              </button>
            </div>
          </div>
        </div>
      </div>
    )
  }

  function renderHealth() {
    return (
      <div className="settings-layout">
        <div className="panel left-panel">
          <h3>{t('Health Snapshot', '健康狀態')}</h3>
          <ul className="simple-list">
            <li>
              <span>{t('Central Home', '中央路徑')}</span>
              <strong className="align-right">{health?.centralHome || '-'}</strong>
            </li>
            <li>
              <span>{t('Records / Logs', '紀錄 / 日誌')}</span>
              <strong>
                {health?.recordsCount ?? 0} / {health?.logsCount ?? 0}
              </strong>
            </li>
            <li>
              <span>{t('Index', '索引')}</span>
              <strong>
                {health?.indexExists
                  ? t(`ready (${health.indexedRecords})`, `就緒（${health.indexedRecords}）`)
                  : t('not built', '未建立')}
              </strong>
            </li>
            <li>
              <span>{t('OpenAI key', 'OpenAI 金鑰')}</span>
              <strong>{health?.hasOpenaiApiKey ? t('configured', '已設定') : t('not set', '未設定')}</strong>
            </li>
            <li>
              <span>{t('Gemini key', 'Gemini 金鑰')}</span>
              <strong>{health?.hasGeminiApiKey ? t('configured', '已設定') : t('not set', '未設定')}</strong>
            </li>
            <li>
              <span>{t('Claude key', 'Claude 金鑰')}</span>
              <strong>{health?.hasClaudeApiKey ? t('configured', '已設定') : t('not set', '未設定')}</strong>
            </li>
            <li>
              <span>{t('Profiles', '設定檔')}</span>
              <strong>{health?.profileCount ?? 0}</strong>
            </li>
          </ul>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleRebuildIndex()} disabled={!centralHome || busy}>
              {t('Rebuild Index', '重建索引')}
            </button>
            <button
              type="button"
              className="ghost-btn"
              onClick={() => {
                if (!centralHome) {
                  return
                }
                void withBusy(async () => {
                  setHealth(await getHealthDiagnostics(centralHome))
                  pushNotice('success', t('Health diagnostics refreshed.', '健康診斷已刷新。'))
                })
              }}
              disabled={!centralHome || busy}
            >
              {t('Refresh Health', '刷新健康狀態')}
            </button>
          </div>
        </div>

        <div className="panel right-panel">
          <h3>{t('Export Report', '匯出報告')}</h3>
          <div className="form-grid two-col-grid">
            <label>
              {t('Title', '標題')}
              <input value={reportTitle} onChange={(event) => setReportTitle(event.target.value)} />
            </label>
            <label>
              {t('Recent Days', '最近天數')}
              <input
                type="number"
                min={1}
                max={365}
                value={reportDays}
                onChange={(event) => setReportDays(Number(event.target.value) || 7)}
              />
            </label>
            <label className="span-2">
              {t('Output Path (optional)', '輸出路徑（選填）')}
              <input value={reportPath} onChange={(event) => setReportPath(event.target.value)} />
            </label>
          </div>
          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleExportReport()} disabled={!centralHome || busy}>
              {t('Export Markdown', '匯出 Markdown')}
            </button>
            <button
              type="button"
              className="ghost-btn"
              onClick={() => {
                setReportTitle('')
                setReportPath('')
                setReportDays(7)
              }}
            >
              {t('Reset', '重置')}
            </button>
          </div>

          <hr className="separator" />

          <h3>{t('Home Fingerprint', '路徑指紋')}</h3>
          <pre className="json-preview">{JSON.stringify(fingerprint ?? {}, null, 2)}</pre>
        </div>
      </div>
    )
  }

  return (
    <div className="workbench-root">
      <aside className="sidebar">
        <h2>KOF Note</h2>
        <p className="muted">{t('Desktop Console', '桌面主控台')}</p>

        <div className="tab-list">
          {TAB_ITEMS.map((tab) => (
            <button
              key={tab}
              type="button"
              className={activeTab === tab ? 'tab-btn active' : 'tab-btn'}
              onClick={() => setActiveTab(tab)}
            >
              {tabLabel(tab)}
            </button>
          ))}
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div
            className="home-pill"
            title={centralHomeDisplay.fullPath || t('No central log selected', '尚未選擇中央記錄路徑')}
          >
            <span className="home-pill-label">{t('Central Log', '中央記錄')}</span>
            <strong className="home-pill-name">{centralHomeDisplay.name || t('Not selected', '未選擇')}</strong>
          </div>
          <button type="button" className="ghost-btn" onClick={() => void handlePickCentralHome()} disabled={busy}>
            {t('Choose Folder', '選擇資料夾')}
          </button>
          <button type="button" onClick={() => void loadCentralHome()} disabled={busy || !centralHomeInput.trim()}>
            {t('Load', '載入')}
          </button>
          <button
            type="button"
            onClick={() => {
              if (centralHome) {
                void withBusy(async () => {
                  const data = await refreshCore(centralHome)
                  setDisplayedRecords(data.records)
                  setSearchMeta(null)
                  pushNotice('success', t('Data refreshed.', '資料已刷新。'))
                })
              }
            }}
            disabled={!centralHome || busy}
          >
            {t('Refresh', '刷新')}
          </button>
        </header>

        <main className="workspace-main">
          {activeTab === 'dashboard' && renderDashboard()}
          {activeTab === 'records' && renderRecords()}
          {activeTab === 'logs' && renderLogs()}
          {activeTab === 'ai' && renderAi()}
          {activeTab === 'integrations' && renderIntegrations()}
          {activeTab === 'settings' && renderSettings()}
          {activeTab === 'health' && renderHealth()}
        </main>
      </section>

      <div className="notice-stack">
        {notices.map((item) => (
          <div key={item.id} className={`notice ${item.type}`}>
            {item.text}
          </div>
        ))}
      </div>

      {commandOpen && (
        <div className="palette-overlay" onClick={() => setCommandOpen(false)}>
          <div className="palette" onClick={(event) => event.stopPropagation()}>
            <input
              ref={commandInputRef}
              value={commandQuery}
              onChange={(event) => setCommandQuery(event.target.value)}
              placeholder={t('Type a command...', '輸入指令...')}
            />
            <div className="palette-list">
              {filteredCommands.map((item) => (
                <button key={item.id} type="button" onClick={() => runCommand(item.id)}>
                  {item.label}
                </button>
              ))}
              {filteredCommands.length === 0 && <p className="muted">{t('No matching command.', '找不到符合的指令。')}</p>}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export default App
