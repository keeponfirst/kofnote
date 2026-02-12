import type { RecordType } from './types'

export const LOCAL_STORAGE_KEY = 'kofnote.centralHome'
export const LOCAL_STORAGE_LANGUAGE_KEY = 'kofnote.uiLanguage'
export const DEFAULT_MODEL = 'gpt-4.1-mini'
export const DEBATE_ROLES = ['Proponent', 'Critic', 'Analyst', 'Synthesizer', 'Judge'] as const
export const RECORD_TYPES: RecordType[] = ['decision', 'worklog', 'idea', 'backlog', 'note']
export const TYPE_COLORS: Record<RecordType, string> = {
  decision: '#20d6ff',
  worklog: '#6d78ff',
  idea: '#ff4cab',
  backlog: '#ffb35e',
  note: '#43e29f',
}

export const DEFAULT_DEBATE_MODEL_BY_PROVIDER: Record<string, string> = {
  local: 'local-heuristic-v1',
  openai: 'gpt-4.1-mini',
  gemini: 'gemini-2.0-flash',
  claude: 'claude-3-5-sonnet-latest',
  'codex-cli': 'auto',
  'gemini-cli': 'auto',
  'claude-cli': 'auto',
  'chatgpt-web': 'auto',
  'gemini-web': 'auto',
  'claude-web': 'auto',
}

export const DASHBOARD_GRAPH_WIDTH = 980
export const DASHBOARD_GRAPH_HEIGHT = 480
