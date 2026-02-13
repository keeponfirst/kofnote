export type RecordType = 'decision' | 'worklog' | 'idea' | 'backlog' | 'note'
export type AiProvider = 'local' | 'openai' | 'gemini' | 'claude'

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
  snippets: Record<string, string>
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

export type DebateProviderType = 'cli' | 'web'

export type DebateProviderConfig = {
  id: string
  type: DebateProviderType
  enabled: boolean
  capabilities: string[]
}

export type DebateProviderRegistrySettings = {
  providers: DebateProviderConfig[]
}

export type AppSettings = {
  profiles: WorkspaceProfile[]
  activeProfileId?: string | null
  pollIntervalSec: number
  uiPreferences: Record<string, unknown>
  integrations: IntegrationsSettings
  providerRegistry: DebateProviderRegistrySettings
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
  hasGeminiApiKey: boolean
  hasClaudeApiKey: boolean
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

export type DebateRole = 'Proponent' | 'Critic' | 'Analyst' | 'Synthesizer' | 'Judge'
export type DebateRound = 'round-1' | 'round-2' | 'round-3'
export type DebateOutputType = 'decision' | 'writing' | 'architecture' | 'planning' | 'evaluation'

export type DebateParticipantConfig = {
  role?: DebateRole | string
  modelProvider?: string
  modelName?: string
}

export type DebateModeRequest = {
  problem: string
  constraints?: string[]
  outputType: DebateOutputType
  participants?: DebateParticipantConfig[]
  maxTurnSeconds?: number
  maxTurnTokens?: number
  writebackRecordType?: RecordType
}

export type DebateChallenge = {
  sourceRole: string
  targetRole: string
  question: string
  response: string
}

export type DebateTurn = {
  role: string
  round: DebateRound | string
  modelProvider: string
  modelName: string
  status: 'ok' | 'failed' | string
  claim: string
  rationale: string
  risks: string[]
  challenges: DebateChallenge[]
  revisions: string[]
  targetRole?: string | null
  durationMs: number
  errorCode?: string | null
  errorMessage?: string | null
  startedAt: string
  finishedAt: string
}

export type DebateRoundArtifact = {
  round: DebateRound | string
  turns: DebateTurn[]
  startedAt: string
  finishedAt: string
}

export type DebatePacketParticipant = {
  role: string
  modelProvider: string
  modelName: string
}

export type DebatePacketConsensus = {
  consensusScore: number
  confidenceScore: number
  keyAgreements: string[]
  keyDisagreements: string[]
}

export type DebateRejectedOption = {
  option: string
  reason: string
}

export type DebateDecision = {
  selectedOption: string
  whySelected: string[]
  rejectedOptions: DebateRejectedOption[]
}

export type DebateRisk = {
  risk: string
  severity: 'high' | 'medium' | 'low' | string
  mitigation: string
}

export type DebateAction = {
  id: string
  action: string
  owner: string
  due: string
}

export type DebateTrace = {
  roundRefs: string[]
  evidenceRefs: string[]
}

export type DebatePacketTimestamps = {
  startedAt: string
  finishedAt: string
}

export type DebateFinalPacket = {
  runId: string
  mode: string
  problem: string
  constraints: string[]
  outputType: DebateOutputType | string
  participants: DebatePacketParticipant[]
  consensus: DebatePacketConsensus
  decision: DebateDecision
  risks: DebateRisk[]
  nextActions: DebateAction[]
  trace: DebateTrace
  timestamps: DebatePacketTimestamps
}

export type DebateModeResponse = {
  runId: string
  mode: string
  state: string
  degraded: boolean
  finalPacket: DebateFinalPacket
  artifactsRoot: string
  writebackJsonPath?: string | null
  errorCodes: string[]
}

export type DebateReplayConsistency = {
  filesComplete: boolean
  sqlIndexed: boolean
  issues: string[]
}

export type DebateReplayResponse = {
  runId: string
  request: Record<string, unknown>
  rounds: Record<string, unknown>[]
  consensus: Record<string, unknown>
  finalPacket: DebateFinalPacket
  writebackRecord?: RecordItem | null
  consistency: DebateReplayConsistency
}

export type DebateRunSummary = {
  runId: string
  problem: string
  provider: string
  outputType: string
  degraded: boolean
  createdAt: string
  artifactsRoot: string
}
