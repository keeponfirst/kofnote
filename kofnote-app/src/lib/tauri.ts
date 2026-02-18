import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import type {
  AiAnalysisResponse,
  AppSettings,
  DebateModeRequest,
  DebateModeResponse,
  DebateReplayResponse,
  DebateRunSummary,
  DashboardStats,
  ExportReportResult,
  HealthDiagnostics,
  HomeFingerprint,
  LogEntry,
  NotebookLmAskResult,
  NotebookLmConfig,
  NotebookSummary,
  NotionConflictStrategy,
  NotionBatchSyncResult,
  NotionSyncResult,
  PromptProfile,
  PromptRunRequest,
  PromptRunResponse,
  PromptTemplate,
  RebuildIndexResult,
  RecordItem,
  RecordPayload,
  ResolvedHome,
  SearchResult,
} from '../types'

const DEFAULT_MOCK_HOME = '/mock/keeponfirst-local-brain'
const MOCK_RUNTIME =
  import.meta.env.VITE_KOF_MOCK === '1' ||
  (typeof window !== 'undefined' && !('__TAURI_INTERNALS__' in (window as Window & { __TAURI_INTERNALS__?: unknown })))

type MockState = {
  centralHome: string
  records: RecordItem[]
  logs: LogEntry[]
  settings: AppSettings
  hasOpenaiKey: boolean
  hasGeminiKey: boolean
  hasClaudeKey: boolean
  hasNotionKey: boolean
  notebooks: NotebookSummary[]
  debateRuns: Record<string, DebateModeResponse>
  promptProfiles: PromptProfile[]
  promptTemplates: PromptTemplate[]
}

function nowMinus(hours: number): string {
  return new Date(Date.now() - hours * 3600_000).toISOString()
}

function slugify(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/(^-|-$)/g, '')
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function createMockState(): MockState {
  const centralHome = DEFAULT_MOCK_HOME
  const records: RecordItem[] = [
    {
      recordType: 'worklog',
      title: 'NotebookLM integration validation',
      createdAt: nowMinus(3),
      sourceText: 'Validated MCP bridge and workflow.',
      finalBody: 'Confirmed notebook list + ask flow. Added error handling and fallback path.',
      tags: ['notebooklm', 'mcp', 'integration'],
      date: nowMinus(3).slice(0, 10),
      notionSyncStatus: 'SUCCESS',
      jsonPath: `${centralHome}/records/worklog/notebooklm-validation.json`,
      mdPath: `${centralHome}/records/worklog/notebooklm-validation.md`,
    },
    {
      recordType: 'idea',
      title: 'Knowledge graph dashboard mode',
      createdAt: nowMinus(9),
      sourceText: 'Need a more cinematic dashboard experience.',
      finalBody: 'Use pulse timeline + constellation and action cards for better command-center feeling.',
      tags: ['dashboard', 'graph', 'ux'],
      date: nowMinus(9).slice(0, 10),
      notionSyncStatus: 'PENDING',
      jsonPath: `${centralHome}/records/idea/knowledge-graph-mode.json`,
      mdPath: `${centralHome}/records/idea/knowledge-graph-mode.md`,
    },
    {
      recordType: 'decision',
      title: 'Move desktop app to Tauri',
      createdAt: nowMinus(30),
      sourceText: 'Need stronger desktop packaging and performance.',
      finalBody: 'Adopt React + Tauri stack for UX flexibility and native distribution.',
      tags: ['tauri', 'architecture', 'decision'],
      date: nowMinus(30).slice(0, 10),
      notionSyncStatus: 'SUCCESS',
      jsonPath: `${centralHome}/records/decision/move-to-tauri.json`,
      mdPath: `${centralHome}/records/decision/move-to-tauri.md`,
    },
    {
      recordType: 'backlog',
      title: 'E2E coverage for language switching',
      createdAt: nowMinus(42),
      sourceText: 'Need tests before adding more languages.',
      finalBody: 'Playwright smoke for tab rendering and language toggle in settings.',
      tags: ['playwright', 'i18n', 'qa'],
      date: nowMinus(42).slice(0, 10),
      notionSyncStatus: 'FAILED',
      notionError: 'waiting for test harness',
      jsonPath: `${centralHome}/records/backlog/e2e-language-switch.json`,
      mdPath: `${centralHome}/records/backlog/e2e-language-switch.md`,
    },
    {
      recordType: 'note',
      title: 'MCP servers available in Codex desktop',
      createdAt: nowMinus(52),
      sourceText: 'kof-nanobanana-mcp and kof-stitch-mcp are visible in settings.',
      finalBody: 'Keep design pipeline flexible with optional MCP-generated assets.',
      tags: ['mcp', 'stitch', 'nanobanana'],
      date: nowMinus(52).slice(0, 10),
      notionSyncStatus: 'SUCCESS',
      jsonPath: `${centralHome}/records/note/mcp-availability.json`,
      mdPath: `${centralHome}/records/note/mcp-availability.md`,
    },
  ]

  const logs: LogEntry[] = [
    {
      timestamp: nowMinus(1),
      eventId: 'evt-1001',
      taskIntent: 'sync_selected',
      status: 'success',
      title: 'Synced selected record to notion',
      data: { jsonPath: records[0].jsonPath },
      raw: { result: 'ok' },
      jsonPath: `${centralHome}/logs/evt-1001.json`,
    },
    {
      timestamp: nowMinus(4),
      eventId: 'evt-1002',
      taskIntent: 'run_ai_analysis',
      status: 'done',
      title: 'Local analysis completed',
      data: { provider: 'local' },
      raw: { summary: 'analysis done' },
      jsonPath: `${centralHome}/logs/evt-1002.json`,
    },
    {
      timestamp: nowMinus(7),
      eventId: 'evt-1003',
      taskIntent: 'sync_batch',
      status: 'failed',
      title: 'Batch sync partially failed',
      data: { failed: 2 },
      raw: { error: 'mock failure' },
      jsonPath: `${centralHome}/logs/evt-1003.json`,
    },
  ]

  return {
    centralHome,
    records,
    logs,
    hasOpenaiKey: false,
    hasGeminiKey: false,
    hasClaudeKey: false,
    hasNotionKey: false,
    notebooks: [
      {
        id: 'nb-main',
        name: 'KOF Weekly Notes',
        sourceCount: 12,
        updatedAt: nowMinus(2),
      },
      {
        id: 'nb-ideas',
        name: 'Idea Radar',
        sourceCount: 7,
        updatedAt: nowMinus(20),
      },
    ],
    debateRuns: {},
    promptProfiles: [
      {
        id: 'pp-work',
        name: '工作用',
        displayName: 'Henry Chen',
        role: 'Software Engineer',
        company: 'ACME Corp',
        department: 'Platform Team',
        bio: '負責後端 API 平台開發，熟悉 Rust、TypeScript。',
        createdAt: nowMinus(10),
        updatedAt: nowMinus(10),
      },
    ],
    promptTemplates: [
      {
        id: 'pt-daily',
        name: '每日工作日報',
        description: '撰寫今日工作重點與進度的日報',
        content:
          '我是 {{display_name}}，{{role}} at {{company}}（{{department}}）。\n{{bio}}\n\n今日工作重點：\n{{focus}}\n\n請幫我撰寫一份簡潔清晰的工作日報。',
        variables: [{ key: 'focus', label: '今日重點', placeholder: '請描述今日主要工作' }],
        createdAt: nowMinus(10),
        updatedAt: nowMinus(10),
      },
    ],
    settings: {
      profiles: [
        {
          id: 'profile-main',
          name: 'Main Workspace',
          centralHome,
          defaultProvider: 'local',
          defaultModel: 'gpt-4.1-mini',
        },
      ],
      activeProfileId: 'profile-main',
      pollIntervalSec: 8,
      uiPreferences: {},
      integrations: {
        notion: {
          enabled: true,
          databaseId: 'mock-notion-db',
        },
        notebooklm: {
          command: 'uvx',
          args: ['kof-notebooklm-mcp'],
          defaultNotebookId: 'nb-main',
        },
      },
      providerRegistry: {
        providers: [
          { id: 'codex-cli', type: 'cli', enabled: true, capabilities: ['debate', 'cli-execution'] },
          { id: 'gemini-cli', type: 'cli', enabled: true, capabilities: ['debate', 'cli-execution'] },
          { id: 'claude-cli', type: 'cli', enabled: true, capabilities: ['debate', 'cli-execution'] },
          { id: 'chatgpt-web', type: 'web', enabled: true, capabilities: ['debate', 'web-automation'] },
          { id: 'gemini-web', type: 'web', enabled: true, capabilities: ['debate', 'web-automation'] },
          { id: 'claude-web', type: 'web', enabled: true, capabilities: ['debate', 'web-automation'] },
        ],
      },
    },
  }
}

const mockState = createMockState()

function summarizeByType(records: RecordItem[]): Record<string, number> {
  const counts: Record<string, number> = {
    decision: 0,
    worklog: 0,
    idea: 0,
    backlog: 0,
    note: 0,
  }
  for (const item of records) {
    counts[item.recordType] = (counts[item.recordType] ?? 0) + 1
  }
  return counts
}

function buildDailyCounts(records: RecordItem[]): Array<{ date: string; count: number }> {
  const byDate = new Map<string, number>()
  for (const item of records) {
    const date = (item.date || item.createdAt.slice(0, 10)).slice(0, 10)
    byDate.set(date, (byDate.get(date) ?? 0) + 1)
  }
  return [...byDate.entries()]
    .map(([date, count]) => ({ date, count }))
    .sort((a, b) => a.date.localeCompare(b.date))
    .slice(-7)
}

function buildTopTags(records: RecordItem[]): Array<{ tag: string; count: number }> {
  const counts = new Map<string, number>()
  for (const item of records) {
    for (const tag of item.tags) {
      counts.set(tag, (counts.get(tag) ?? 0) + 1)
    }
  }
  return [...counts.entries()]
    .map(([tag, count]) => ({ tag, count }))
    .sort((a, b) => b.count - a.count)
    .slice(0, 12)
}

function createDashboardStats(): DashboardStats {
  const records = mockState.records
  return {
    totalRecords: records.length,
    totalLogs: mockState.logs.length,
    typeCounts: summarizeByType(records),
    topTags: buildTopTags(records),
    recentDailyCounts: buildDailyCounts(records),
    pendingSyncCount: records.filter((item) => item.notionSyncStatus === 'PENDING').length,
  }
}

function createFingerprint(): HomeFingerprint {
  const latestRecordAt = mockState.records.map((item) => item.createdAt).sort().at(-1) ?? ''
  const latestLogAt = mockState.logs.map((item) => item.timestamp).sort().at(-1) ?? ''

  return {
    token: `${mockState.records.length}:${mockState.logs.length}:${latestRecordAt}:${latestLogAt}`,
    recordsCount: mockState.records.length,
    logsCount: mockState.logs.length,
    latestRecordAt,
    latestLogAt,
  }
}

function createHealth(): HealthDiagnostics {
  const fingerprint = createFingerprint()
  return {
    centralHome: mockState.centralHome,
    recordsCount: mockState.records.length,
    logsCount: mockState.logs.length,
    indexPath: `${mockState.centralHome}/.index/search.sqlite`,
    indexExists: true,
    indexedRecords: mockState.records.length,
    latestRecordAt: fingerprint.latestRecordAt,
    latestLogAt: fingerprint.latestLogAt,
    hasOpenaiApiKey: mockState.hasOpenaiKey,
    hasGeminiApiKey: mockState.hasGeminiKey,
    hasClaudeApiKey: mockState.hasClaudeKey,
    profileCount: mockState.settings.profiles.length,
  }
}

function createMockDebateResponse(request: DebateModeRequest, centralHome: string): DebateModeResponse {
  const now = new Date()
  const startedAt = now.toISOString()
  const finishedAt = new Date(now.getTime() + 1500).toISOString()
  const runId = `debate_${now.toISOString().replace(/[-:TZ.]/g, '').slice(0, 14)}_${Math.floor(Math.random() * 9999)}`
  const problem = request.problem?.trim() || '(empty problem)'
  const outputType = request.outputType || 'decision'
  const participants =
    request.participants && request.participants.length > 0
      ? request.participants
      : ['Proponent', 'Critic', 'Analyst', 'Synthesizer', 'Judge'].map((role) => ({
          role,
          modelProvider: 'local',
          modelName: 'local-heuristic-v1',
        }))

  const response: DebateModeResponse = {
    runId,
    mode: 'debate-v0.1',
    state: 'Writeback',
    degraded: false,
    artifactsRoot: `${centralHome}/records/debates/${runId}`,
    writebackJsonPath: `${centralHome}/records/${outputType === 'decision' ? 'decision' : 'worklog'}/${slugify(problem)}.json`,
    errorCodes: [],
    finalPacket: {
      runId,
      mode: 'debate-v0.1',
      problem,
      constraints: request.constraints ?? [],
      outputType,
      participants: participants.map((item) => ({
        role: item.role || 'Proponent',
        modelProvider: item.modelProvider || 'local',
        modelName: item.modelName || 'local-heuristic-v1',
      })),
      consensus: {
        consensusScore: 0.92,
        confidenceScore: 0.89,
        keyAgreements: ['Keep local-first writeback as source of truth.', 'Return executable next actions.'],
        keyDisagreements: ['Provider selection can impact cost vs latency trade-offs.'],
      },
      decision: {
        selectedOption: `Execute ${outputType} packet for: ${problem}`,
        whySelected: ['Best balance between speed, traceability, and replayability.'],
        rejectedOptions: [{ option: 'Single-model quick answer', reason: 'Too little adversarial checking.' }],
      },
      risks: [
        {
          risk: 'Provider instability can reduce debate completeness.',
          severity: 'medium',
          mitigation: 'Enable degraded completion with explicit failure traces.',
        },
      ],
      nextActions: [
        { id: 'A1', action: 'Run packet execution checklist', owner: 'me', due: new Date(now.getTime() + 86400000).toISOString().slice(0, 10) },
        { id: 'A2', action: 'Review risks and mitigation readiness', owner: 'me', due: new Date(now.getTime() + 3 * 86400000).toISOString().slice(0, 10) },
        { id: 'A3', action: 'Replay run and record final decision', owner: 'me', due: new Date(now.getTime() + 7 * 86400000).toISOString().slice(0, 10) },
      ],
      trace: {
        roundRefs: ['round-1', 'round-2', 'round-3'],
        evidenceRefs: [
          `${centralHome}/records/debates/${runId}/request.json`,
          `${centralHome}/records/debates/${runId}/consensus.json`,
        ],
      },
      timestamps: {
        startedAt,
        finishedAt,
      },
    },
  }

  return response
}

function findRecordByPath(jsonPath: string): RecordItem | undefined {
  return mockState.records.find((item) => item.jsonPath === jsonPath)
}

function recordFromPayload(centralHome: string, payload: RecordPayload): RecordItem {
  const createdAt = payload.createdAt || new Date().toISOString()
  const slug = slugify(payload.title || `${payload.recordType}-${Date.now()}`) || `record-${Date.now()}`
  const jsonPath = `${centralHome}/records/${payload.recordType}/${slug}.json`
  return {
    recordType: payload.recordType,
    title: payload.title,
    createdAt,
    sourceText: payload.sourceText ?? '',
    finalBody: payload.finalBody ?? '',
    tags: payload.tags ?? [],
    date: payload.date,
    notionPageId: payload.notionPageId,
    notionUrl: payload.notionUrl,
    notionSyncStatus: payload.notionSyncStatus || 'SUCCESS',
    notionError: payload.notionError,
    notionLastSyncedAt: payload.notionLastSyncedAt,
    notionLastEditedTime: payload.notionLastEditedTime,
    notionLastSyncedHash: payload.notionLastSyncedHash,
    jsonPath,
    mdPath: jsonPath.replace(/\.json$/, '.md'),
  }
}

async function mockInvoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  switch (command) {
    case 'resolve_central_home': {
      const inputPath = String(args.inputPath ?? '').trim() || DEFAULT_MOCK_HOME
      mockState.centralHome = inputPath
      return {
        centralHome: inputPath,
        corrected: inputPath !== String(args.inputPath ?? ''),
      } as T
    }
    case 'list_records': {
      return clone(
        [...mockState.records].sort((a, b) => String(b.createdAt).localeCompare(String(a.createdAt))),
      ) as T
    }
    case 'list_logs': {
      return clone([...mockState.logs].sort((a, b) => String(b.timestamp).localeCompare(String(a.timestamp)))) as T
    }
    case 'get_dashboard_stats': {
      return createDashboardStats() as T
    }
    case 'upsert_record': {
      const centralHome = String(args.centralHome ?? mockState.centralHome)
      const payload = args.payload as RecordPayload
      const previousJsonPath = (args.previousJsonPath as string | null | undefined) ?? null
      if (previousJsonPath) {
        const existing = findRecordByPath(previousJsonPath)
        if (existing) {
          existing.recordType = payload.recordType
          existing.title = payload.title
          existing.createdAt = payload.createdAt ?? existing.createdAt
          existing.sourceText = payload.sourceText ?? ''
          existing.finalBody = payload.finalBody ?? ''
          existing.tags = payload.tags ?? []
          existing.date = payload.date
          existing.notionPageId = payload.notionPageId
          existing.notionUrl = payload.notionUrl
          existing.notionSyncStatus = payload.notionSyncStatus || existing.notionSyncStatus
          existing.notionError = payload.notionError
          return clone(existing) as T
        }
      }
      const created = recordFromPayload(centralHome, payload)
      mockState.records.unshift(created)
      return clone(created) as T
    }
    case 'delete_record': {
      const jsonPath = String(args.jsonPath ?? '')
      mockState.records = mockState.records.filter((item) => item.jsonPath !== jsonPath)
      return undefined as T
    }
    case 'rebuild_search_index': {
      const result: RebuildIndexResult = {
        indexedCount: mockState.records.length,
        indexPath: `${mockState.centralHome}/.index/search.sqlite`,
        tookMs: 24,
      }
      return result as T
    }
    case 'search_records': {
      const query = String(args.query ?? '').toLowerCase().trim()
      const recordType = String(args.recordType ?? '').trim()
      const dateFrom = String(args.dateFrom ?? '').trim()
      const dateTo = String(args.dateTo ?? '').trim()
      const limit = Number(args.limit ?? 1000)
      const offset = Number(args.offset ?? 0)
      let filtered = [...mockState.records]
      if (recordType) {
        filtered = filtered.filter((item) => item.recordType === recordType)
      }
      if (dateFrom) {
        filtered = filtered.filter((item) => (item.date || item.createdAt.slice(0, 10)) >= dateFrom)
      }
      if (dateTo) {
        filtered = filtered.filter((item) => (item.date || item.createdAt.slice(0, 10)) <= dateTo)
      }
      if (query) {
        filtered = filtered.filter((item) => {
          const joined = `${item.title}\n${item.sourceText}\n${item.finalBody}\n${item.tags.join(' ')}`.toLowerCase()
          return joined.includes(query)
        })
      }
      const page = filtered.slice(offset, offset + limit)
      const snippets: Record<string, string> = {}
      if (query) {
        for (const item of page) {
          if (!item.jsonPath) {
            continue
          }
          const merged = `${item.title} ${item.finalBody} ${item.sourceText}`
          const lower = merged.toLowerCase()
          const idx = lower.indexOf(query)
          if (idx < 0) {
            continue
          }
          const start = Math.max(0, idx - 48)
          const end = Math.min(merged.length, idx + query.length + 96)
          const excerpt = merged.slice(start, end)
          const escaped = query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
          snippets[item.jsonPath] = excerpt.replace(new RegExp(escaped, 'ig'), '<mark>$&</mark>')
        }
      }
      const result: SearchResult = {
        records: clone(page),
        total: filtered.length,
        indexed: true,
        tookMs: 9,
        snippets,
      }
      return result as T
    }
    case 'run_ai_analysis': {
      const prompt = String(args.prompt ?? '').trim()
      const provider = String(args.provider ?? 'local')
      const model = String(args.model ?? 'gpt-4.1-mini')
      const content = [
        'Summary: momentum is strongest on integration and UX upgrades.',
        'Risk: sync reliability can regress if conflict handling is not covered by tests.',
        'Risk: language growth will be expensive without key-based dictionaries.',
        'Action: finish key-based i18n migration and add smoke E2E checks.',
        'Action: ship timeline pulse + constellation view for stronger situational awareness.',
        prompt ? `Context Prompt: ${prompt}` : '',
      ]
        .filter(Boolean)
        .join('\n')
      const result: AiAnalysisResponse = {
        provider,
        model,
        content,
      }
      return result as T
    }
    case 'run_debate_mode': {
      const centralHome = String(args.centralHome ?? mockState.centralHome)
      const request = (args.request ?? {}) as DebateModeRequest
      const response = createMockDebateResponse(request, centralHome)
      mockState.debateRuns[response.runId] = clone(response)
      return clone(response) as T
    }
    case 'replay_debate_mode': {
      const runId = String(args.runId ?? '')
      const hit = mockState.debateRuns[runId]
      const response: DebateReplayResponse = hit
        ? {
            runId: hit.runId,
            request: {
              problem: hit.finalPacket.problem,
              constraints: hit.finalPacket.constraints,
              outputType: hit.finalPacket.outputType,
            },
            rounds: [
              { round: 'round-1', turns: [] },
              { round: 'round-2', turns: [] },
              { round: 'round-3', turns: [] },
            ],
            consensus: hit.finalPacket.consensus as unknown as Record<string, unknown>,
            finalPacket: hit.finalPacket,
            writebackRecord: hit.writebackJsonPath
              ? mockState.records.find((item) => item.jsonPath === hit.writebackJsonPath) ?? null
              : null,
            consistency: {
              filesComplete: true,
              sqlIndexed: true,
              issues: [],
            },
          }
        : {
            runId,
            request: {},
            rounds: [],
            consensus: {},
            finalPacket: createMockDebateResponse(
              {
                problem: 'Unknown run',
                outputType: 'decision',
              },
              mockState.centralHome,
            ).finalPacket,
            writebackRecord: null,
            consistency: {
              filesComplete: false,
              sqlIndexed: false,
              issues: [`Missing run in mock runtime: ${runId}`],
            },
      }
      return clone(response) as T
    }
    case 'list_debate_runs': {
      const summaries: DebateRunSummary[] = Object.values(mockState.debateRuns).map((run) => ({
        runId: run.runId,
        problem: run.finalPacket.problem.slice(0, 120),
        provider: run.finalPacket.participants[0]?.modelProvider ?? 'local',
        outputType: run.finalPacket.outputType,
        degraded: run.degraded,
        createdAt: run.finalPacket.timestamps.startedAt,
        artifactsRoot: run.artifactsRoot,
      }))
      summaries.sort((a, b) => b.createdAt.localeCompare(a.createdAt))
      return clone(summaries) as T
    }
    case 'export_markdown_report': {
      const title = String(args.title ?? 'KOF Note Report')
      const outputPath = String(args.outputPath ?? `${mockState.centralHome}/reports/kof-report.md`)
      const result: ExportReportResult = {
        outputPath,
        title,
      }
      return result as T
    }
    case 'get_home_fingerprint': {
      return createFingerprint() as T
    }
    case 'get_health_diagnostics': {
      return createHealth() as T
    }
    case 'get_app_settings': {
      return clone(mockState.settings) as T
    }
    case 'save_app_settings': {
      const settings = args.settings as AppSettings
      mockState.settings = clone(settings)
      return clone(mockState.settings) as T
    }
    case 'set_openai_api_key': {
      mockState.hasOpenaiKey = true
      return true as T
    }
    case 'has_openai_api_key': {
      return mockState.hasOpenaiKey as T
    }
    case 'clear_openai_api_key': {
      mockState.hasOpenaiKey = false
      return true as T
    }
    case 'set_gemini_api_key': {
      mockState.hasGeminiKey = true
      return true as T
    }
    case 'has_gemini_api_key': {
      return mockState.hasGeminiKey as T
    }
    case 'clear_gemini_api_key': {
      mockState.hasGeminiKey = false
      return true as T
    }
    case 'set_claude_api_key': {
      mockState.hasClaudeKey = true
      return true as T
    }
    case 'has_claude_api_key': {
      return mockState.hasClaudeKey as T
    }
    case 'clear_claude_api_key': {
      mockState.hasClaudeKey = false
      return true as T
    }
    case 'set_notion_api_key': {
      mockState.hasNotionKey = true
      return true as T
    }
    case 'has_notion_api_key': {
      return mockState.hasNotionKey as T
    }
    case 'clear_notion_api_key': {
      mockState.hasNotionKey = false
      return true as T
    }
    case 'sync_record_to_notion':
    case 'sync_record_bidirectional': {
      const jsonPath = String(args.jsonPath ?? '')
      const existing = findRecordByPath(jsonPath)
      if (existing) {
        existing.notionSyncStatus = 'SUCCESS'
        existing.notionError = null
      }
      const result: NotionSyncResult = {
        jsonPath,
        notionPageId: existing?.notionPageId ?? `page_${Date.now()}`,
        notionUrl: existing?.notionUrl ?? 'https://notion.so/mock',
        notionSyncStatus: 'SUCCESS',
        notionError: null,
        action: 'upserted',
        conflict: false,
      }
      return result as T
    }
    case 'sync_records_to_notion':
    case 'sync_records_bidirectional':
    case 'pull_records_from_notion': {
      const paths = ((args.jsonPaths as string[] | undefined) ?? mockState.records.map((item) => item.jsonPath || '')).filter(
        Boolean,
      )
      const results: NotionSyncResult[] = paths.map((path) => ({
        jsonPath: path,
        notionPageId: `page_${slugify(path).slice(0, 10)}`,
        notionUrl: 'https://notion.so/mock',
        notionSyncStatus: 'SUCCESS',
        notionError: null,
        action: 'upserted',
        conflict: false,
      }))
      for (const path of paths) {
        const existing = findRecordByPath(path)
        if (existing) {
          existing.notionSyncStatus = 'SUCCESS'
          existing.notionError = null
        }
      }
      const result: NotionBatchSyncResult = {
        total: results.length,
        success: results.length,
        failed: 0,
        conflicts: 0,
        results,
      }
      return result as T
    }
    case 'notebooklm_health_check': {
      return {
        status: 'healthy',
        auth: 'ok',
        latencyMs: 45,
      } as T
    }
    case 'notebooklm_list_notebooks': {
      const limit = Number(args.limit ?? 50)
      return clone(mockState.notebooks.slice(0, limit)) as T
    }
    case 'notebooklm_create_notebook': {
      const title = String(args.title ?? '').trim() || `Notebook ${mockState.notebooks.length + 1}`
      const created: NotebookSummary = {
        id: `nb-${Date.now()}`,
        name: title,
        sourceCount: 0,
        updatedAt: new Date().toISOString(),
      }
      mockState.notebooks.unshift(created)
      return clone(created) as T
    }
    case 'notebooklm_add_record_source': {
      const notebookId = String(args.notebookId ?? '')
      const target = mockState.notebooks.find((item) => item.id === notebookId)
      if (target) {
        target.sourceCount = (target.sourceCount ?? 0) + 1
        target.updatedAt = new Date().toISOString()
      }
      return { ok: true } as T
    }
    case 'notebooklm_ask': {
      const question = String(args.question ?? '').trim()
      const result: NotebookLmAskResult = {
        answer: `Mock answer for: ${question || '(empty)'}`,
        citations: ['Record: NotebookLM integration validation', 'Record: Knowledge graph dashboard mode'],
      }
      return result as T
    }
    case 'list_prompt_profiles': {
      return clone(mockState.promptProfiles) as T
    }
    case 'upsert_prompt_profile': {
      const profile = args.profile as PromptProfile
      const now = new Date().toISOString()
      const id = profile.id?.trim() || `pp-${Date.now()}`
      const saved: PromptProfile = { ...profile, id, updatedAt: now, createdAt: profile.createdAt || now }
      const idx = mockState.promptProfiles.findIndex((p) => p.id === id)
      if (idx >= 0) {
        mockState.promptProfiles[idx] = saved
      } else {
        mockState.promptProfiles.push(saved)
      }
      return clone(saved) as T
    }
    case 'delete_prompt_profile': {
      const id = String(args.id ?? '')
      mockState.promptProfiles = mockState.promptProfiles.filter((p) => p.id !== id)
      return undefined as T
    }
    case 'list_prompt_templates': {
      return clone(mockState.promptTemplates) as T
    }
    case 'upsert_prompt_template': {
      const template = args.template as PromptTemplate
      const now = new Date().toISOString()
      const id = template.id?.trim() || `pt-${Date.now()}`
      const saved: PromptTemplate = { ...template, id, updatedAt: now, createdAt: template.createdAt || now }
      const idx = mockState.promptTemplates.findIndex((t) => t.id === id)
      if (idx >= 0) {
        mockState.promptTemplates[idx] = saved
      } else {
        mockState.promptTemplates.push(saved)
      }
      return clone(saved) as T
    }
    case 'delete_prompt_template': {
      const id = String(args.id ?? '')
      mockState.promptTemplates = mockState.promptTemplates.filter((t) => t.id !== id)
      return undefined as T
    }
    case 'run_prompt_service': {
      const request = args.request as PromptRunRequest
      const profile = mockState.promptProfiles.find((p) => p.id === request.profileId)
      const template = mockState.promptTemplates.find((t) => t.id === request.templateId)
      let resolved = template?.content ?? ''
      if (profile) {
        resolved = resolved
          .replace(/\{\{display_name\}\}/g, profile.displayName)
          .replace(/\{\{role\}\}/g, profile.role)
          .replace(/\{\{company\}\}/g, profile.company)
          .replace(/\{\{department\}\}/g, profile.department)
          .replace(/\{\{bio\}\}/g, profile.bio)
      }
      for (const [key, value] of Object.entries(request.variableValues ?? {})) {
        resolved = resolved.replace(new RegExp(`\\{\\{${key}\\}\\}`, 'g'), value)
      }
      const provider = request.provider ?? 'local'
      const result: PromptRunResponse = {
        result: `[Mock AI 回覆 · ${provider}]\n\n${resolved}`,
        resolvedPrompt: resolved,
        provider,
      }
      return result as T
    }
    default:
      throw new Error(`Mock runtime: unsupported command ${command}`)
  }
}

async function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (MOCK_RUNTIME) {
    return mockInvoke<T>(command, args)
  }
  return invoke<T>(command, args)
}

export async function resolveCentralHome(inputPath: string): Promise<ResolvedHome> {
  return invokeCommand<ResolvedHome>('resolve_central_home', { inputPath })
}

export async function listRecords(centralHome: string): Promise<RecordItem[]> {
  return invokeCommand<RecordItem[]>('list_records', { centralHome })
}

export async function listLogs(centralHome: string): Promise<LogEntry[]> {
  return invokeCommand<LogEntry[]>('list_logs', { centralHome })
}

export async function getDashboardStats(centralHome: string): Promise<DashboardStats> {
  return invokeCommand<DashboardStats>('get_dashboard_stats', { centralHome })
}

export async function upsertRecord(
  centralHome: string,
  payload: RecordPayload,
  previousJsonPath?: string | null,
): Promise<RecordItem> {
  return invokeCommand<RecordItem>('upsert_record', {
    centralHome,
    payload,
    previousJsonPath: previousJsonPath ?? null,
  })
}

export async function deleteRecord(centralHome: string, jsonPath: string): Promise<void> {
  return invokeCommand<void>('delete_record', { centralHome, jsonPath })
}

export async function rebuildSearchIndex(centralHome: string): Promise<RebuildIndexResult> {
  return invokeCommand<RebuildIndexResult>('rebuild_search_index', { centralHome })
}

export async function searchRecords(args: {
  centralHome: string
  query?: string
  recordType?: string
  dateFrom?: string
  dateTo?: string
  limit?: number
  offset?: number
}): Promise<SearchResult> {
  return invokeCommand<SearchResult>('search_records', args)
}

export async function runAiAnalysis(args: {
  centralHome: string
  provider?: string
  model?: string
  prompt: string
  apiKey?: string
  includeLogs?: boolean
  maxRecords?: number
}): Promise<AiAnalysisResponse> {
  return invokeCommand<AiAnalysisResponse>('run_ai_analysis', args)
}

export async function exportMarkdownReport(args: {
  centralHome: string
  outputPath?: string
  title?: string
  recentDays?: number
}): Promise<ExportReportResult> {
  return invokeCommand<ExportReportResult>('export_markdown_report', args)
}

export async function getHomeFingerprint(centralHome: string): Promise<HomeFingerprint> {
  return invokeCommand<HomeFingerprint>('get_home_fingerprint', { centralHome })
}

export async function getHealthDiagnostics(centralHome: string): Promise<HealthDiagnostics> {
  return invokeCommand<HealthDiagnostics>('get_health_diagnostics', { centralHome })
}

export async function getAppSettings(): Promise<AppSettings> {
  return invokeCommand<AppSettings>('get_app_settings')
}

export async function saveAppSettings(settings: AppSettings): Promise<AppSettings> {
  return invokeCommand<AppSettings>('save_app_settings', { settings })
}

export async function setOpenaiApiKey(apiKey: string): Promise<boolean> {
  return invokeCommand<boolean>('set_openai_api_key', { apiKey })
}

export async function hasOpenaiApiKey(): Promise<boolean> {
  return invokeCommand<boolean>('has_openai_api_key')
}

export async function clearOpenaiApiKey(): Promise<boolean> {
  return invokeCommand<boolean>('clear_openai_api_key')
}

export async function setGeminiApiKey(apiKey: string): Promise<boolean> {
  return invokeCommand<boolean>('set_gemini_api_key', { apiKey })
}

export async function hasGeminiApiKey(): Promise<boolean> {
  return invokeCommand<boolean>('has_gemini_api_key')
}

export async function clearGeminiApiKey(): Promise<boolean> {
  return invokeCommand<boolean>('clear_gemini_api_key')
}

export async function setClaudeApiKey(apiKey: string): Promise<boolean> {
  return invokeCommand<boolean>('set_claude_api_key', { apiKey })
}

export async function hasClaudeApiKey(): Promise<boolean> {
  return invokeCommand<boolean>('has_claude_api_key')
}

export async function clearClaudeApiKey(): Promise<boolean> {
  return invokeCommand<boolean>('clear_claude_api_key')
}

export async function setNotionApiKey(apiKey: string): Promise<boolean> {
  return invokeCommand<boolean>('set_notion_api_key', { apiKey })
}

export async function hasNotionApiKey(): Promise<boolean> {
  return invokeCommand<boolean>('has_notion_api_key')
}

export async function clearNotionApiKey(): Promise<boolean> {
  return invokeCommand<boolean>('clear_notion_api_key')
}

export async function syncRecordToNotion(args: {
  centralHome: string
  jsonPath: string
  databaseId?: string
  conflictStrategy?: NotionConflictStrategy
}): Promise<NotionSyncResult> {
  return invokeCommand<NotionSyncResult>('sync_record_to_notion', args)
}

export async function syncRecordsToNotion(args: {
  centralHome: string
  jsonPaths: string[]
  databaseId?: string
  conflictStrategy?: NotionConflictStrategy
}): Promise<NotionBatchSyncResult> {
  return invokeCommand<NotionBatchSyncResult>('sync_records_to_notion', args)
}

export async function syncRecordBidirectional(args: {
  centralHome: string
  jsonPath: string
  databaseId?: string
  conflictStrategy?: NotionConflictStrategy
}): Promise<NotionSyncResult> {
  return invokeCommand<NotionSyncResult>('sync_record_bidirectional', args)
}

export async function syncRecordsBidirectional(args: {
  centralHome: string
  jsonPaths: string[]
  databaseId?: string
  conflictStrategy?: NotionConflictStrategy
}): Promise<NotionBatchSyncResult> {
  return invokeCommand<NotionBatchSyncResult>('sync_records_bidirectional', args)
}

export async function pullRecordsFromNotion(args: {
  centralHome: string
  databaseId?: string
  conflictStrategy?: NotionConflictStrategy
}): Promise<NotionBatchSyncResult> {
  return invokeCommand<NotionBatchSyncResult>('pull_records_from_notion', args)
}

export async function notebooklmHealthCheck(config?: NotebookLmConfig): Promise<unknown> {
  return invokeCommand<unknown>('notebooklm_health_check', { config })
}

export async function notebooklmListNotebooks(args?: {
  limit?: number
  config?: NotebookLmConfig
}): Promise<NotebookSummary[]> {
  return invokeCommand<NotebookSummary[]>('notebooklm_list_notebooks', args ?? {})
}

export async function notebooklmCreateNotebook(args?: {
  title?: string
  config?: NotebookLmConfig
}): Promise<NotebookSummary> {
  return invokeCommand<NotebookSummary>('notebooklm_create_notebook', args ?? {})
}

export async function notebooklmAddRecordSource(args: {
  centralHome: string
  jsonPath: string
  notebookId: string
  title?: string
  config?: NotebookLmConfig
}): Promise<unknown> {
  return invokeCommand<unknown>('notebooklm_add_record_source', args)
}

export async function notebooklmAsk(args: {
  notebookId: string
  question: string
  includeCitations?: boolean
  config?: NotebookLmConfig
}): Promise<NotebookLmAskResult> {
  return invokeCommand<NotebookLmAskResult>('notebooklm_ask', args)
}

export async function runDebateMode(args: {
  centralHome: string
  request: DebateModeRequest
}): Promise<DebateModeResponse> {
  return invokeCommand<DebateModeResponse>('run_debate_mode', args)
}

export async function replayDebateMode(args: {
  centralHome: string
  runId: string
}): Promise<DebateReplayResponse> {
  return invokeCommand<DebateReplayResponse>('replay_debate_mode', args)
}

export async function listDebateRuns(args: { centralHome: string }): Promise<DebateRunSummary[]> {
  return invokeCommand<DebateRunSummary[]>('list_debate_runs', args)
}

export async function listPromptProfiles(centralHome: string): Promise<PromptProfile[]> {
  return invokeCommand<PromptProfile[]>('list_prompt_profiles', { centralHome })
}

export async function upsertPromptProfile(centralHome: string, profile: PromptProfile): Promise<PromptProfile> {
  return invokeCommand<PromptProfile>('upsert_prompt_profile', { centralHome, profile })
}

export async function deletePromptProfile(centralHome: string, id: string): Promise<void> {
  return invokeCommand<void>('delete_prompt_profile', { centralHome, id })
}

export async function listPromptTemplates(centralHome: string): Promise<PromptTemplate[]> {
  return invokeCommand<PromptTemplate[]>('list_prompt_templates', { centralHome })
}

export async function upsertPromptTemplate(centralHome: string, template: PromptTemplate): Promise<PromptTemplate> {
  return invokeCommand<PromptTemplate>('upsert_prompt_template', { centralHome, template })
}

export async function deletePromptTemplate(centralHome: string, id: string): Promise<void> {
  return invokeCommand<void>('delete_prompt_template', { centralHome, id })
}

export async function runPromptService(centralHome: string, request: PromptRunRequest): Promise<PromptRunResponse> {
  return invokeCommand<PromptRunResponse>('run_prompt_service', { centralHome, request })
}

export async function pickCentralHomeDirectory(defaultPath?: string): Promise<string | null> {
  if (MOCK_RUNTIME) {
    return defaultPath || mockState.centralHome
  }
  const selected = await open({
    directory: true,
    multiple: false,
    defaultPath,
    title: 'Select Central Home',
  })
  return typeof selected === 'string' ? selected : null
}
