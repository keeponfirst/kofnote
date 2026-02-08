export type RecordType = 'decision' | 'worklog' | 'idea' | 'backlog' | 'note'
export type AiProvider = 'local' | 'openai'

export type RecordItem = {
  recordType: RecordType
  title: string
  createdAt: string
  sourceText: string
  finalBody: string
  tags: string[]
  date?: string | null
  notionPageId?: string | null
  notionUrl?: string | null
  notionSyncStatus: string
  notionError?: string | null
  notionLastSyncedAt?: string | null
  notionLastEditedTime?: string | null
  notionLastSyncedHash?: string | null
  jsonPath?: string | null
  mdPath?: string | null
}

export type RecordPayload = {
  recordType: RecordType
  title: string
  createdAt?: string
  sourceText?: string
  finalBody?: string
  tags?: string[]
  date?: string | null
  notionPageId?: string | null
  notionUrl?: string | null
  notionSyncStatus?: string
  notionError?: string | null
  notionLastSyncedAt?: string | null
  notionLastEditedTime?: string | null
  notionLastSyncedHash?: string | null
}

export type LogEntry = {
  timestamp: string
  eventId: string
  taskIntent: string
  status: string
  title: string
  data: unknown
  raw: unknown
  jsonPath?: string | null
}

export type TagCount = {
  tag: string
  count: number
}

export type DailyCount = {
  date: string
  count: number
}

export type DashboardStats = {
  totalRecords: number
  totalLogs: number
  typeCounts: Record<string, number>
  topTags: TagCount[]
  recentDailyCounts: DailyCount[]
  pendingSyncCount: number
}

export type ResolvedHome = {
  centralHome: string
  corrected: boolean
}

export type SearchResult = {
  records: RecordItem[]
  total: number
  indexed: boolean
  tookMs: number
}

export type RebuildIndexResult = {
  indexedCount: number
  indexPath: string
  tookMs: number
}

export type AiAnalysisResponse = {
  provider: string
  model: string
  content: string
}

export type WorkspaceProfile = {
  id: string
  name: string
  centralHome: string
  defaultProvider: string
  defaultModel: string
}

export type NotionSettings = {
  enabled: boolean
  databaseId: string
}

export type NotebookLmSettings = {
  command: string
  args: string[]
  defaultNotebookId?: string | null
}

export type IntegrationsSettings = {
  notion: NotionSettings
  notebooklm: NotebookLmSettings
}

export type AppSettings = {
  profiles: WorkspaceProfile[]
  activeProfileId?: string | null
  pollIntervalSec: number
  uiPreferences: Record<string, unknown>
  integrations: IntegrationsSettings
}

export type ExportReportResult = {
  outputPath: string
  title: string
}

export type HealthDiagnostics = {
  centralHome: string
  recordsCount: number
  logsCount: number
  indexPath: string
  indexExists: boolean
  indexedRecords: number
  latestRecordAt: string
  latestLogAt: string
  hasOpenaiApiKey: boolean
  profileCount: number
}

export type HomeFingerprint = {
  token: string
  recordsCount: number
  logsCount: number
  latestRecordAt: string
  latestLogAt: string
}

export type NotionSyncResult = {
  jsonPath: string
  notionPageId?: string | null
  notionUrl?: string | null
  notionSyncStatus: string
  notionError?: string | null
  action: string
  conflict: boolean
}

export type NotionBatchSyncResult = {
  total: number
  success: number
  failed: number
  conflicts: number
  results: NotionSyncResult[]
}

export type NotionConflictStrategy = 'manual' | 'local_wins' | 'notion_wins'

export type NotebookLmConfig = {
  command?: string
  args?: string[]
}

export type NotebookSummary = {
  id: string
  name: string
  sourceCount?: number | null
  updatedAt?: string | null
}

export type NotebookLmAskResult = {
  answer: string
  citations: string[]
}
