import { invoke } from '@tauri-apps/api/core'
import type {
  AiAnalysisResponse,
  AppSettings,
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
  RebuildIndexResult,
  RecordItem,
  RecordPayload,
  ResolvedHome,
  SearchResult,
} from '../types'

export async function resolveCentralHome(inputPath: string): Promise<ResolvedHome> {
  return invoke<ResolvedHome>('resolve_central_home', { inputPath })
}

export async function listRecords(centralHome: string): Promise<RecordItem[]> {
  return invoke<RecordItem[]>('list_records', { centralHome })
}

export async function listLogs(centralHome: string): Promise<LogEntry[]> {
  return invoke<LogEntry[]>('list_logs', { centralHome })
}

export async function getDashboardStats(centralHome: string): Promise<DashboardStats> {
  return invoke<DashboardStats>('get_dashboard_stats', { centralHome })
}

export async function upsertRecord(
  centralHome: string,
  payload: RecordPayload,
  previousJsonPath?: string | null,
): Promise<RecordItem> {
  return invoke<RecordItem>('upsert_record', {
    centralHome,
    payload,
    previousJsonPath: previousJsonPath ?? null,
  })
}

export async function deleteRecord(centralHome: string, jsonPath: string): Promise<void> {
  return invoke<void>('delete_record', { centralHome, jsonPath })
}

export async function rebuildSearchIndex(centralHome: string): Promise<RebuildIndexResult> {
  return invoke<RebuildIndexResult>('rebuild_search_index', { centralHome })
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
  return invoke<SearchResult>('search_records', args)
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
  return invoke<AiAnalysisResponse>('run_ai_analysis', args)
}

export async function exportMarkdownReport(args: {
  centralHome: string
  outputPath?: string
  title?: string
  recentDays?: number
}): Promise<ExportReportResult> {
  return invoke<ExportReportResult>('export_markdown_report', args)
}

export async function getHomeFingerprint(centralHome: string): Promise<HomeFingerprint> {
  return invoke<HomeFingerprint>('get_home_fingerprint', { centralHome })
}

export async function getHealthDiagnostics(centralHome: string): Promise<HealthDiagnostics> {
  return invoke<HealthDiagnostics>('get_health_diagnostics', { centralHome })
}

export async function getAppSettings(): Promise<AppSettings> {
  return invoke<AppSettings>('get_app_settings')
}

export async function saveAppSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke<AppSettings>('save_app_settings', { settings })
}

export async function setOpenaiApiKey(apiKey: string): Promise<boolean> {
  return invoke<boolean>('set_openai_api_key', { apiKey })
}

export async function hasOpenaiApiKey(): Promise<boolean> {
  return invoke<boolean>('has_openai_api_key')
}

export async function clearOpenaiApiKey(): Promise<boolean> {
  return invoke<boolean>('clear_openai_api_key')
}

export async function setNotionApiKey(apiKey: string): Promise<boolean> {
  return invoke<boolean>('set_notion_api_key', { apiKey })
}

export async function hasNotionApiKey(): Promise<boolean> {
  return invoke<boolean>('has_notion_api_key')
}

export async function clearNotionApiKey(): Promise<boolean> {
  return invoke<boolean>('clear_notion_api_key')
}

export async function syncRecordToNotion(args: {
  centralHome: string
  jsonPath: string
  databaseId?: string
  conflictStrategy?: NotionConflictStrategy
}): Promise<NotionSyncResult> {
  return invoke<NotionSyncResult>('sync_record_to_notion', args)
}

export async function syncRecordsToNotion(args: {
  centralHome: string
  jsonPaths: string[]
  databaseId?: string
  conflictStrategy?: NotionConflictStrategy
}): Promise<NotionBatchSyncResult> {
  return invoke<NotionBatchSyncResult>('sync_records_to_notion', args)
}

export async function syncRecordBidirectional(args: {
  centralHome: string
  jsonPath: string
  databaseId?: string
  conflictStrategy?: NotionConflictStrategy
}): Promise<NotionSyncResult> {
  return invoke<NotionSyncResult>('sync_record_bidirectional', args)
}

export async function syncRecordsBidirectional(args: {
  centralHome: string
  jsonPaths: string[]
  databaseId?: string
  conflictStrategy?: NotionConflictStrategy
}): Promise<NotionBatchSyncResult> {
  return invoke<NotionBatchSyncResult>('sync_records_bidirectional', args)
}

export async function pullRecordsFromNotion(args: {
  centralHome: string
  databaseId?: string
  conflictStrategy?: NotionConflictStrategy
}): Promise<NotionBatchSyncResult> {
  return invoke<NotionBatchSyncResult>('pull_records_from_notion', args)
}

export async function notebooklmHealthCheck(config?: NotebookLmConfig): Promise<unknown> {
  return invoke<unknown>('notebooklm_health_check', { config })
}

export async function notebooklmListNotebooks(args?: {
  limit?: number
  config?: NotebookLmConfig
}): Promise<NotebookSummary[]> {
  return invoke<NotebookSummary[]>('notebooklm_list_notebooks', args ?? {})
}

export async function notebooklmCreateNotebook(args?: {
  title?: string
  config?: NotebookLmConfig
}): Promise<NotebookSummary> {
  return invoke<NotebookSummary>('notebooklm_create_notebook', args ?? {})
}

export async function notebooklmAddRecordSource(args: {
  centralHome: string
  jsonPath: string
  notebookId: string
  title?: string
  config?: NotebookLmConfig
}): Promise<unknown> {
  return invoke<unknown>('notebooklm_add_record_source', args)
}

export async function notebooklmAsk(args: {
  notebookId: string
  question: string
  includeCitations?: boolean
  config?: NotebookLmConfig
}): Promise<NotebookLmAskResult> {
  return invoke<NotebookLmAskResult>('notebooklm_ask', args)
}
