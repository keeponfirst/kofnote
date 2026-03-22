import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import AiTab from './AiTab'
import DashboardTab from './DashboardTab'
import LogsTab from './LogsTab'
import TimelineTab from './TimelineTab'
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
  resolveCentralHome,
  runPromptService,
  saveAppSettings,
  setClaudeApiKey,
  setGeminiApiKey,
  setNotionApiKey,
  searchRecords,
  syncRecordBidirectional,
  syncRecordsBidirectional,
  setOpenaiApiKey,
  upsertRecord,
  quickCapture,
  listPromptProfiles,
  upsertPromptProfile,
  deletePromptProfile,
  listPromptTemplates,
  upsertPromptTemplate,
  deletePromptTemplate,
  seedDefaultTemplates,
  getTimeline,
  supabaseSignIn,
  supabaseSignOut,
  supabaseAuthStatus,
  supabaseFullSync,
} from '../lib/tauri'
import { getLanguageLabel, isSupportedLanguage, SUPPORTED_LANGUAGES, translate, type UiLanguage } from '../i18n'
import { buildProviderRegistrySettings } from '../lib/providerRegistry'
import {
  DEFAULT_MODEL,
  LOCAL_STORAGE_KEY,
  LOCAL_STORAGE_LANGUAGE_KEY,
  MEMORY_COLOR,
  RECORD_TYPES,
  TYPE_COLORS,
} from '../constants'
import { useNotices } from '../hooks/useNotices'
import type {
  AppSettings,
  DebateProviderConfig,
  DashboardStats,
  HealthDiagnostics,
  HomeFingerprint,
  NotionConflictStrategy,
  NotebookSummary,
  PromptProfile,
  PromptRunResponse,
  PromptTemplate,
  TemplateVariable,
  RecordItem,
  RecordPayload,
  RecordType,
  SearchResult,
  CaptureCompletePayload,
  CaptureFailedPayload,
  UnifiedMemoryItem,
  WorkspaceProfile,
} from '../types'

type TabKey = 'dashboard' | 'records' | 'timeline' | 'logs' | 'ai' | 'integrations' | 'settings' | 'health' | 'prompt'

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

const TAB_ITEMS: TabKey[] = ['dashboard', 'records', 'timeline', 'logs', 'ai', 'prompt', 'integrations', 'settings', 'health']

type TemplateValues = Record<string, string | number>

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
  const [stats, setStats] = useState<DashboardStats | null>(null)
  const [health, setHealth] = useState<HealthDiagnostics | null>(null)
  const [memoryItems, setMemoryItems] = useState<UnifiedMemoryItem[]>([])

  const [selectedRecordPath, setSelectedRecordPath] = useState<string | null>(null)
  const [recordForm, setRecordForm] = useState<RecordFormState>(emptyForm())

  const [recordFilterType, setRecordFilterType] = useState<'all' | RecordType>('all')
  const [recordKeyword, setRecordKeyword] = useState('')
  const [recordDateFrom, setRecordDateFrom] = useState('')
  const [recordDateTo, setRecordDateTo] = useState('')
  const [visibleCount, setVisibleCount] = useState(200)
  const [searchMeta, setSearchMeta] = useState<SearchMeta | null>(null)
  const [selectedRecordPaths, setSelectedRecordPaths] = useState<Set<string>>(new Set())
  const [batchMode, setBatchMode] = useState(false)

  // Prompt Service state
  const [promptProfiles, setPromptProfiles] = useState<PromptProfile[]>([])
  const [promptTemplates, setPromptTemplates] = useState<PromptTemplate[]>([])
  const [activePromptProfileId, setActivePromptProfileId] = useState<string>('')
  const [activePromptTemplateId, setActivePromptTemplateId] = useState<string>('')
  const [promptProfileDraft, setPromptProfileDraft] = useState<PromptProfile>({
    id: '', name: '', displayName: '', role: '', company: '', department: '', bio: '', createdAt: '', updatedAt: '',
  })
  const [promptTemplateDraft, setPromptTemplateDraft] = useState<PromptTemplate>({
    id: '', name: '', description: '', content: '', variables: [], createdAt: '', updatedAt: '',
  })
  const [promptVariableValues, setPromptVariableValues] = useState<Record<string, string>>({})
  const [promptProvider, setPromptProvider] = useState<string>('local')
  const [promptRunResult, setPromptRunResult] = useState<PromptRunResponse | null>(null)
  const [promptBusy, setPromptBusy] = useState(false)

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
      supabase: {
        url: '',
        anonKey: '',
        lastSyncAt: '1970-01-01T00:00:00Z',
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
  const [supabaseSignedIn, setSupabaseSignedIn] = useState(false)
  const [supabaseEmail, setSupabaseEmail] = useState('')
  const [supabaseLoginEmail, setSupabaseLoginEmail] = useState('')
  const [supabaseLoginPassword, setSupabaseLoginPassword] = useState('')
  const [supabaseSyncing, setSupabaseSyncing] = useState(false)
  const [supabaseSyncResult, setSupabaseSyncResult] = useState<string | null>(null)
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
  const [commandOpen, setCommandOpen] = useState(false)
  const [commandQuery, setCommandQuery] = useState('')
  const { notices, pushNotice } = useNotices()
  const commandInputRef = useRef<HTMLInputElement | null>(null)
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
        case 'timeline':
          return t('tab.timeline')
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
        case 'prompt':
          return t('tab.prompt')
        default:
          return key
      }
    },
    [t],
  )

  const withBusy = useCallback(async <T,>(task: () => Promise<T>) => {
    setBusy(true)
    try {
      return await task()
    } finally {
      setBusy(false)
    }
  }, [])

  const visibleRecords = useMemo(
    () => displayedRecords.slice(0, Math.min(visibleCount, displayedRecords.length)),
    [displayedRecords, visibleCount],
  )

  const selectedRecord = useMemo(
    () => allRecords.find((item) => item.jsonPath === selectedRecordPath) ?? null,
    [allRecords, selectedRecordPath],
  )

  const selectedProfile = useMemo(
    () => appSettings.profiles.find((item) => item.id === selectedProfileId) ?? null,
    [appSettings.profiles, selectedProfileId],
  )

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
      const [nextRecords, _nextLogs, nextStats, nextHealth, nextFingerprint] = await Promise.all([
        listRecords(home),
        listLogs(home),
        getDashboardStats(home),
        getHealthDiagnostics(home),
        getHomeFingerprint(home),
      ])

      setAllRecords(nextRecords)
      setStats(nextStats)
      setHealth(nextHealth)
      setFingerprint(nextFingerprint)

      return { records: nextRecords }
    },
    [],
  )

  const refreshPromptData = useCallback(async (home: string) => {
    if (!home.trim()) return
    const [profiles, templates] = await Promise.all([
      listPromptProfiles(home),
      listPromptTemplates(home),
    ])
    // 首次使用時自動建立預設模板
    if (templates.length === 0) {
      const seeded = await seedDefaultTemplates(home)
      if (seeded > 0) {
        const freshTemplates = await listPromptTemplates(home)
        setPromptProfiles(profiles)
        setPromptTemplates(freshTemplates)
        return
      }
    }
    setPromptProfiles(profiles)
    setPromptTemplates(templates)
  }, [])

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

        const [coreResult] = await Promise.all([
          refreshCore(resolved.centralHome),
          refreshPromptData(resolved.centralHome),
        ])
        setDisplayedRecords(coreResult.records)
        setSearchMeta(null)

        if (coreResult.records.length > 0) {
          const first = coreResult.records[0]
          setSelectedRecordPath(first.jsonPath ?? null)
          setRecordForm(formFromRecord(first))
        } else {
          setSelectedRecordPath(null)
          setRecordForm(emptyForm())
        }

        pushNotice(
          'success',
          resolved.corrected
            ? t(`Loaded and normalized to ${resolved.centralHome}`, `已載入並正規化為 ${resolved.centralHome}`)
            : t(`Loaded ${resolved.centralHome}`, `已載入 ${resolved.centralHome}`),
        )
      })
    },
    [centralHomeInput, pushNotice, refreshCore, refreshPromptData, t, withBusy],
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
    const [settings, hasOpenai, hasGemini, hasClaude, notionKey, sbStatus] = await Promise.all([
      getAppSettings(),
      hasOpenaiApiKey(),
      hasGeminiApiKey(),
      hasClaudeApiKey(),
      hasNotionApiKey(),
      supabaseAuthStatus(),
    ])
    const normalizedSettings: AppSettings = {
      ...settings,
      integrations: {
        ...settings.integrations,
        supabase: settings.integrations.supabase ?? { url: '', anonKey: '', lastSyncAt: '1970-01-01T00:00:00Z' },
      },
      providerRegistry: buildProviderRegistrySettings(settings.providerRegistry),
    }
    setAppSettings(normalizedSettings)
    setHasOpenaiKey(hasOpenai)
    setHasGeminiKey(hasGemini)
    setHasClaudeKey(hasClaude)
    setHasNotionKey(notionKey)
    setSupabaseSignedIn(sbStatus.signed_in)
    setSupabaseEmail(sbStatus.email)
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

  // Quick Capture: register global shortcut + listen for capture events
  useEffect(() => {
    if (!centralHome) return

    const cleanups: Array<() => void> = []
    let active = true

    void (async () => {
      try {
        const [{ register, unregister }, { readText }, { sendNotification }, { listen: listenEvent }] = await Promise.all([
          import('@tauri-apps/plugin-global-shortcut'),
          import('@tauri-apps/plugin-clipboard-manager'),
          import('@tauri-apps/plugin-notification'),
          import('@tauri-apps/api/event'),
        ])

        if (!active) return

        await register('CommandOrControl+Shift+K', async () => {
          try {
            const content = await readText()
            if (!content?.trim()) {
              pushNotice('info', t('capture.toast.empty'))
              return
            }
            pushNotice('info', t('capture.toast.captured'))
            
            // Get current active profile's default provider and model directly from state
            const currentProfile = appSettings.profiles.find((p) => p.id === appSettings.activeProfileId)
            const provider = currentProfile?.defaultProvider || 'local'
            const model = currentProfile?.defaultModel || ''
            
            await quickCapture({ centralHome, content, provider, model })
          } catch (err) {
            pushNotice('error', String(err))
          }
        })
        cleanups.push(() => { void unregister('CommandOrControl+Shift+K').catch(() => {}) })

        const unlistenComplete = await listenEvent<CaptureCompletePayload>('capture_complete', (event) => {
          const { recordType, title } = event.payload
          try { sendNotification({ title: `KOF Note — ${t('capture.notify.saved', { type: recordType })}`, body: title }) } catch { /* ignore */ }
          if (centralHome) {
            void refreshCore(centralHome).then((data) => {
              setDisplayedRecords(data.records)
              setSearchMeta(null)
            }).catch(() => {})
          }
        })
        cleanups.push(unlistenComplete)

        const unlistenFailed = await listenEvent<CaptureFailedPayload>('capture_failed', (event) => {
          const { error } = event.payload
          if (error === 'NO_AI_KEY') {
            pushNotice('info', t('capture.notify.noKey'))
            try { sendNotification({ title: `KOF Note — ${t('capture.notify.noKey')}`, body: '' }) } catch { /* ignore */ }
          } else {
            pushNotice('error', t('capture.notify.failed'))
            try { sendNotification({ title: `KOF Note — ${t('capture.notify.failed')}`, body: t('capture.notify.failedBody') }) } catch { /* ignore */ }
          }
        })
        cleanups.push(unlistenFailed)
      } catch {
        // Plugin not available (mock runtime) — skip silently
      }
    })()

    return () => {
      active = false
      for (const cleanup of cleanups) cleanup()
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [centralHome])

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

  useEffect(() => {
    if (!centralHome) return
    void getTimeline({ centralHome, groupBy: 'day', sources: ['memory'], limit: 100 })
      .then((res) => {
        const items = res.groups.flatMap((g) => g.items)
        setMemoryItems(items)
      })
      .catch(() => {})
  }, [centralHome])

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

  const handleBatchDelete = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }
    const targets = [...selectedRecordPaths].filter(Boolean)
    if (targets.length === 0) {
      pushNotice('error', t('No records selected.', '尚未選擇任何紀錄。'))
      return
    }
    if (!window.confirm(t(`Delete ${targets.length} selected record(s)?`, `要刪除 ${targets.length} 筆已選紀錄嗎？`))) {
      return
    }

    await withBusy(async () => {
      for (const jsonPath of targets) {
        await deleteRecord(centralHome, jsonPath)
      }
      const data = await refreshCore(centralHome)
      setDisplayedRecords(data.records)
      setSearchMeta(null)
      setSelectedRecordPaths(new Set())
      pushNotice('success', t(`Deleted ${targets.length} record(s).`, `已刪除 ${targets.length} 筆紀錄。`))
    })
  }, [centralHome, pushNotice, refreshCore, selectedRecordPaths, t, withBusy])

  const handleBatchSyncNotion = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', t('Load Central Home first.', '請先載入中央路徑。'))
      return
    }
    const targets = [...selectedRecordPaths].filter(Boolean)
    if (targets.length === 0) {
      pushNotice('error', t('No records selected.', '尚未選擇任何紀錄。'))
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
          `Batch sync done. success=${result.success}, failed=${result.failed}, conflicts=${result.conflicts}.`,
          `批次同步完成。成功=${result.success}，失敗=${result.failed}，衝突=${result.conflicts}。`,
        ),
      )
    })
  }, [
    appSettings.integrations.notion.databaseId,
    centralHome,
    notionConflictStrategy,
    pushNotice,
    refreshCore,
    selectedRecordPaths,
    t,
    withBusy,
  ])

  const handleBatchExport = useCallback(() => {
    const selected = allRecords.filter((item) => item.jsonPath && selectedRecordPaths.has(item.jsonPath))
    if (selected.length === 0) {
      pushNotice('error', t('No records selected.', '尚未選擇任何紀錄。'))
      return
    }

    const lines = [
      `# Batch Export (${selected.length} records)`,
      '',
      `Generated: ${new Date().toISOString()}`,
      '',
    ]
    for (const item of selected) {
      lines.push(`## [${item.recordType}] ${item.title}`)
      lines.push(`- Created: ${item.createdAt}`)
      if (item.date) {
        lines.push(`- Date: ${item.date}`)
      }
      if (item.tags.length > 0) {
        lines.push(`- Tags: ${item.tags.join(', ')}`)
      }
      lines.push('')
      lines.push(item.finalBody || '')
      lines.push('')
      lines.push('---')
      lines.push('')
    }

    const markdown = lines.join('\n')
    const blob = new Blob([markdown], { type: 'text/markdown;charset=utf-8' })
    const url = URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = url
    link.download = `kof-records-batch-${Date.now()}.md`
    document.body.appendChild(link)
    link.click()
    link.remove()
    URL.revokeObjectURL(url)
    pushNotice('success', t('Batch markdown exported.', '批次 Markdown 已匯出。'))
  }, [allRecords, pushNotice, selectedRecordPaths, t])

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
        run: () => setActiveTab('ai'),
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

  const handleDashboardNavigateToRecords = useCallback(
    (opts: { type?: RecordType; keyword?: string; jsonPath?: string }) => {
      if (opts.type != null) {
        setRecordFilterType(opts.type)
        setRecordKeyword('')
      }
      if (opts.keyword != null) {
        setRecordFilterType('all')
        setRecordKeyword(opts.keyword)
      }
      if (opts.jsonPath != null) {
        const target = allRecords.find((r) => r.jsonPath === opts.jsonPath)
        if (target) {
          setSelectedRecordPath(opts.jsonPath)
          setRecordForm(formFromRecord(target))
        }
      }
      setActiveTab('records')
    },
    [allRecords],
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
    for (const item of memoryItems) {
      for (const tag of item.tags) {
        if (!tag) continue
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

    const memorySourceCounts = new Map<string, number>()
    for (const item of memoryItems) {
      memorySourceCounts.set(item.sourceType, (memorySourceCounts.get(item.sourceType) ?? 0) + 1)
    }
    const memoryNodes = [...memorySourceCounts.entries()].map(([sourceType, count], index, source) => {
      const angle = (Math.PI * 2 * index) / Math.max(1, source.length) + Math.PI / 4
      return {
        sourceType,
        count,
        x: 50 + Math.cos(angle) * 47,
        y: 50 + Math.sin(angle) * 44,
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
            {memoryNodes.length > 0 && <div className="constellation-ring ring-c" />}
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
            {memoryNodes.map((node) => (
              <button
                type="button"
                key={node.sourceType}
                className="const-node memory-node"
                style={{
                  left: `${node.x}%`,
                  top: `${node.y}%`,
                  borderColor: MEMORY_COLOR,
                  boxShadow: `0 0 12px ${MEMORY_COLOR}44`,
                }}
                onClick={() => setRecordKeyword(node.sourceType)}
              >
                <span>{t('records.constellation.memory')}: {node.sourceType}</span>
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
              <button
                type="button"
                className="ghost-btn"
                onClick={() => {
                  setBatchMode((prev) => {
                    const next = !prev
                    if (!next) {
                      setSelectedRecordPaths(new Set())
                    }
                    return next
                  })
                }}
              >
                {batchMode ? t('Exit Batch Mode', '離開批次模式') : t('Batch Mode', '批次模式')}
              </button>
            </div>

            {batchMode ? (
              <div className="toolbar-row">
                <button
                  type="button"
                  className="ghost-btn"
                  onClick={() => {
                    const next = new Set(
                      displayedRecords.map((item) => item.jsonPath).filter((item): item is string => Boolean(item)),
                    )
                    setSelectedRecordPaths(next)
                  }}
                >
                  {t('Select All', '全選')}
                </button>
                <button type="button" className="ghost-btn" onClick={() => setSelectedRecordPaths(new Set())}>
                  {t('Deselect All', '取消全選')}
                </button>
              </div>
            ) : null}

            {batchMode && selectedRecordPaths.size > 0 ? (
              <div className="toolbar-row">
                <span className="muted">{t(`${selectedRecordPaths.size} selected`, `${selectedRecordPaths.size} 筆已選`)}</span>
                <button type="button" onClick={() => void handleBatchDelete()} disabled={busy}>
                  {t('Delete Selected', '刪除已選')}
                </button>
                <button type="button" onClick={() => void handleBatchSyncNotion()} disabled={busy}>
                  {t('Sync to Notion', '同步到 Notion')}
                </button>
                <button type="button" onClick={() => void handleBatchExport()} disabled={busy}>
                  {t('Export Markdown', '匯出 Markdown')}
                </button>
                <button type="button" className="ghost-btn" onClick={() => setSelectedRecordPaths(new Set())}>
                  {t('Deselect All', '取消全選')}
                </button>
              </div>
            ) : null}

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
                const checked = item.jsonPath ? selectedRecordPaths.has(item.jsonPath) : false
                return (
                  <div
                    key={item.jsonPath ?? `${item.createdAt}-${item.title}`}
                    style={{ display: 'grid', gridTemplateColumns: batchMode ? '20px minmax(0, 1fr)' : '1fr', gap: 8 }}
                  >
                    {batchMode ? (
                      <input
                        type="checkbox"
                        checked={checked}
                        onChange={(event) => {
                          const key = item.jsonPath ?? ''
                          if (!key) {
                            return
                          }
                          setSelectedRecordPaths((prev) => {
                            const next = new Set(prev)
                            if (event.target.checked) {
                              next.add(key)
                            } else {
                              next.delete(key)
                            }
                            return next
                          })
                        }}
                        onClick={(event) => event.stopPropagation()}
                      />
                    ) : null}
                    <button
                      type="button"
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
                  </div>
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

          <hr className="separator" />

          <h3>{t('Supabase Cloud Sync', 'Supabase 雲端同步')}</h3>

          <div className="panel panel-strong">
            <h3>{t('Supabase Connection', 'Supabase 連線設定')}</h3>
            <div className="form-grid two-col-grid">
              <label>
                {t('Supabase URL', 'Supabase URL')}
                <input
                  value={appSettings.integrations.supabase?.url ?? ''}
                  onChange={(event) =>
                    setAppSettings((prev) => ({
                      ...prev,
                      integrations: {
                        ...prev.integrations,
                        supabase: { ...prev.integrations.supabase, url: event.target.value },
                      },
                    }))
                  }
                  placeholder="https://xxx.supabase.co"
                />
              </label>
              <label>
                {t('Anon Key', 'Anon Key')}
                <input
                  type="password"
                  value={appSettings.integrations.supabase?.anonKey ?? ''}
                  onChange={(event) =>
                    setAppSettings((prev) => ({
                      ...prev,
                      integrations: {
                        ...prev.integrations,
                        supabase: { ...prev.integrations.supabase, anonKey: event.target.value },
                      },
                    }))
                  }
                  placeholder="eyJhbGci..."
                />
              </label>
            </div>
            <div className="toolbar-row">
              <button
                type="button"
                onClick={() => void handleSaveSettings(appSettings)}
                disabled={busy}
              >
                {t('Save Supabase Settings', '儲存 Supabase 設定')}
              </button>
            </div>
          </div>

          <div className="panel panel-strong">
            <h3>
              {t('Supabase Auth', 'Supabase 帳號')}
              {supabaseSignedIn && (
                <span className="muted" style={{ marginLeft: 8, fontWeight: 'normal', fontSize: '0.85em' }}>
                  ✓ {supabaseEmail}
                </span>
              )}
            </h3>
            {!supabaseSignedIn ? (
              <>
                <div className="form-grid two-col-grid">
                  <label>
                    {t('Email', 'Email')}
                    <input
                      type="email"
                      value={supabaseLoginEmail}
                      onChange={(event) => setSupabaseLoginEmail(event.target.value)}
                      placeholder="user@example.com"
                    />
                  </label>
                  <label>
                    {t('Password', '密碼')}
                    <input
                      type="password"
                      value={supabaseLoginPassword}
                      onChange={(event) => setSupabaseLoginPassword(event.target.value)}
                      placeholder="••••••••"
                    />
                  </label>
                </div>
                <div className="toolbar-row">
                  <button
                    type="button"
                    disabled={busy || !supabaseLoginEmail || !supabaseLoginPassword}
                    onClick={() =>
                      void (async () => {
                        try {
                          const result = await supabaseSignIn(supabaseLoginEmail, supabaseLoginPassword)
                          setSupabaseSignedIn(result.signed_in)
                          setSupabaseEmail(result.email)
                          setSupabaseLoginPassword('')
                          pushNotice('success', t(`Signed in as ${result.email}`, `已登入：${result.email}`))
                        } catch (e) {
                          pushNotice('error', String(e))
                        }
                      })()
                    }
                  >
                    {t('Sign In', '登入')}
                  </button>
                </div>
              </>
            ) : (
              <div className="toolbar-row two-col">
                <button
                  type="button"
                  disabled={supabaseSyncing}
                  onClick={() =>
                    void (async () => {
                      setSupabaseSyncing(true)
                      setSupabaseSyncResult(null)
                      try {
                        const stats = await supabaseFullSync()
                        setSupabaseSyncResult(
                          t(
                            `Sync done: pushed ${stats.pushed}, pulled ${stats.pulled}, failed ${stats.failed}`,
                            `同步完成：上傳 ${stats.pushed}，下載 ${stats.pulled}，失敗 ${stats.failed}`,
                          ),
                        )
                        pushNotice('success', t('Supabase sync complete', 'Supabase 同步完成'))
                      } catch (e) {
                        pushNotice('error', String(e))
                      } finally {
                        setSupabaseSyncing(false)
                      }
                    })()
                  }
                >
                  {supabaseSyncing ? t('Syncing…', '同步中…') : t('Sync Now', '立即同步')}
                </button>
                <button
                  type="button"
                  className="ghost-btn"
                  onClick={() =>
                    void (async () => {
                      await supabaseSignOut()
                      setSupabaseSignedIn(false)
                      setSupabaseEmail('')
                      pushNotice('success', t('Signed out', '已登出'))
                    })()
                  }
                >
                  {t('Sign Out', '登出')}
                </button>
              </div>
            )}
            {supabaseSyncResult && (
              <p className="muted" style={{ marginTop: 8 }}>
                {supabaseSyncResult}
              </p>
            )}
          </div>
        </div>
      </div>
    )
  }

  function renderPromptService() {
    const activeProfile = promptProfiles.find((p) => p.id === activePromptProfileId) ?? null
    const activeTemplate = promptTemplates.find((t) => t.id === activePromptTemplateId) ?? null

    function resolvePreview(): string {
      if (!activeProfile || !activeTemplate) return ''
      let resolved = activeTemplate.content
        .replace(/\{\{display_name\}\}/g, activeProfile.displayName)
        .replace(/\{\{role\}\}/g, activeProfile.role)
        .replace(/\{\{company\}\}/g, activeProfile.company)
        .replace(/\{\{department\}\}/g, activeProfile.department)
        .replace(/\{\{bio\}\}/g, activeProfile.bio)
      for (const [key, value] of Object.entries(promptVariableValues)) {
        resolved = resolved.replace(new RegExp(`\\{\\{${key}\\}\\}`, 'g'), value)
      }
      return resolved
    }

    async function handleSaveProfile() {
      if (!centralHome) return
      setPromptBusy(true)
      try {
        const saved = await upsertPromptProfile(centralHome, promptProfileDraft)
        await refreshPromptData(centralHome)
        setActivePromptProfileId(saved.id)
        setPromptProfileDraft(saved)
        pushNotice('success', t('Profile saved.', 'Profile 已儲存。'))
      } catch (e) {
        pushNotice('error', String(e))
      } finally {
        setPromptBusy(false)
      }
    }

    async function handleDeleteProfile() {
      if (!centralHome || !activePromptProfileId) return
      setPromptBusy(true)
      try {
        await deletePromptProfile(centralHome, activePromptProfileId)
        await refreshPromptData(centralHome)
        setActivePromptProfileId('')
        setPromptProfileDraft({ id: '', name: '', displayName: '', role: '', company: '', department: '', bio: '', createdAt: '', updatedAt: '' })
        pushNotice('success', t('Profile deleted.', 'Profile 已刪除。'))
      } catch (e) {
        pushNotice('error', String(e))
      } finally {
        setPromptBusy(false)
      }
    }

    async function handleSaveTemplate() {
      if (!centralHome) return
      setPromptBusy(true)
      try {
        const saved = await upsertPromptTemplate(centralHome, promptTemplateDraft)
        await refreshPromptData(centralHome)
        setActivePromptTemplateId(saved.id)
        setPromptTemplateDraft(saved)
        pushNotice('success', t('Template saved.', '模板已儲存。'))
      } catch (e) {
        pushNotice('error', String(e))
      } finally {
        setPromptBusy(false)
      }
    }

    async function handleDeleteTemplate() {
      if (!centralHome || !activePromptTemplateId) return
      setPromptBusy(true)
      try {
        await deletePromptTemplate(centralHome, activePromptTemplateId)
        await refreshPromptData(centralHome)
        setActivePromptTemplateId('')
        setPromptTemplateDraft({ id: '', name: '', description: '', content: '', variables: [], createdAt: '', updatedAt: '' })
        pushNotice('success', t('Template deleted.', '模板已刪除。'))
      } catch (e) {
        pushNotice('error', String(e))
      } finally {
        setPromptBusy(false)
      }
    }

    async function handleRunPrompt() {
      if (!centralHome) return
      if (!activePromptProfileId) { pushNotice('error', t('prompt.run.noProfile')); return }
      if (!activePromptTemplateId) { pushNotice('error', t('prompt.run.noTemplate')); return }
      setPromptBusy(true)
      setPromptRunResult(null)
      try {
        const result = await runPromptService(centralHome, {
          profileId: activePromptProfileId,
          templateId: activePromptTemplateId,
          variableValues: promptVariableValues,
          provider: promptProvider,
        })
        setPromptRunResult(result)
      } catch (e) {
        pushNotice('error', String(e))
      } finally {
        setPromptBusy(false)
      }
    }

    function addTemplateVariable() {
      setPromptTemplateDraft((prev) => ({
        ...prev,
        variables: [...prev.variables, { key: '', label: '', placeholder: '' }],
      }))
    }

    function updateTemplateVariable(index: number, field: keyof TemplateVariable, value: string) {
      setPromptTemplateDraft((prev) => {
        const vars = [...prev.variables]
        vars[index] = { ...vars[index], [field]: value }
        return { ...prev, variables: vars }
      })
    }

    function removeTemplateVariable(index: number) {
      setPromptTemplateDraft((prev) => ({
        ...prev,
        variables: prev.variables.filter((_, i) => i !== index),
      }))
    }

    const preview = resolvePreview()

    return (
      <div className="page-stack">
        <div className="panel section-hero ai-hero">
          <div className="section-hero-content">
            <p className="eyebrow">{t('prompt.hero.eyebrow')}</p>
            <h3>{t('prompt.hero.title')}</h3>
            <p className="hero-desc">{t('prompt.hero.desc')}</p>
          </div>
          <div className="section-hero-stats">
            <div className="hero-stat">
              <span className="stat-num">{promptProfiles.length}</span>
              <span className="stat-label">{t('prompt.hero.profiles')}</span>
            </div>
            <div className="hero-stat">
              <span className="stat-num">{promptTemplates.length}</span>
              <span className="stat-label">{t('prompt.hero.templates')}</span>
            </div>
          </div>
        </div>

        <div className="settings-layout" style={{ alignItems: 'flex-start' }}>
          {/* ── 左欄：身份 Profiles ── */}
          <div className="panel left-panel">
            <div className="panel-header">
              <div>
                <h3>{t('prompt.profile.title')}</h3>
                <p className="panel-sub">{t('prompt.profile.subtitle')}</p>
              </div>
              <button
                type="button"
                className="ghost-btn"
                onClick={() => {
                  setActivePromptProfileId('')
                  setPromptProfileDraft({ id: '', name: '', displayName: '', role: '', company: '', department: '', bio: '', createdAt: '', updatedAt: '' })
                }}
              >
                {t('prompt.profile.new')}
              </button>
            </div>

            {promptProfiles.length === 0 && (
              <p className="empty-hint">{t('prompt.profile.empty')}</p>
            )}
            <ul className="simple-list">
              {promptProfiles.map((p) => (
                <li
                  key={p.id}
                  className={activePromptProfileId === p.id ? 'selected' : ''}
                  style={{ cursor: 'pointer' }}
                  onClick={() => {
                    setActivePromptProfileId(p.id)
                    setPromptProfileDraft(p)
                  }}
                >
                  <span>{p.name || p.displayName || p.id}</span>
                  {activePromptProfileId === p.id && <span className="badge-ok">✓</span>}
                </li>
              ))}
            </ul>

            <div className="form-section" style={{ marginTop: '1rem' }}>
              <label className="field-label">{t('prompt.profile.name')}</label>
              <input className="text-input" value={promptProfileDraft.name} onChange={(e) => setPromptProfileDraft((prev) => ({ ...prev, name: e.target.value }))} placeholder={t('prompt.profile.name')} />
              <label className="field-label">{t('prompt.profile.displayName')}</label>
              <input className="text-input" value={promptProfileDraft.displayName} onChange={(e) => setPromptProfileDraft((prev) => ({ ...prev, displayName: e.target.value }))} placeholder="Henry Chen" />
              <label className="field-label">{t('prompt.profile.role')}</label>
              <input className="text-input" value={promptProfileDraft.role} onChange={(e) => setPromptProfileDraft((prev) => ({ ...prev, role: e.target.value }))} placeholder="Software Engineer" />
              <label className="field-label">{t('prompt.profile.company')}</label>
              <input className="text-input" value={promptProfileDraft.company} onChange={(e) => setPromptProfileDraft((prev) => ({ ...prev, company: e.target.value }))} placeholder="ACME Corp" />
              <label className="field-label">{t('prompt.profile.department')}</label>
              <input className="text-input" value={promptProfileDraft.department} onChange={(e) => setPromptProfileDraft((prev) => ({ ...prev, department: e.target.value }))} placeholder="Platform Team" />
              <label className="field-label">{t('prompt.profile.bio')}</label>
              <textarea className="text-input" rows={3} value={promptProfileDraft.bio} onChange={(e) => setPromptProfileDraft((prev) => ({ ...prev, bio: e.target.value }))} placeholder={t('prompt.profile.bio')} />
              <div className="row-actions" style={{ marginTop: '0.5rem' }}>
                <button type="button" onClick={() => void handleSaveProfile()} disabled={promptBusy || !centralHome}>{t('prompt.profile.save')}</button>
                {activePromptProfileId && (
                  <button type="button" className="danger-btn" onClick={() => void handleDeleteProfile()} disabled={promptBusy}>{t('prompt.profile.delete')}</button>
                )}
              </div>
            </div>
          </div>

          {/* ── 中欄：模板庫 ── */}
          <div className="panel left-panel">
            <div className="panel-header">
              <div>
                <h3>{t('prompt.template.title')}</h3>
                <p className="panel-sub">{t('prompt.template.subtitle')}</p>
              </div>
              <button
                type="button"
                className="ghost-btn"
                onClick={() => {
                  setActivePromptTemplateId('')
                  setPromptTemplateDraft({ id: '', name: '', description: '', content: '', variables: [], createdAt: '', updatedAt: '' })
                }}
              >
                {t('prompt.template.new')}
              </button>
            </div>

            {promptTemplates.length === 0 && (
              <p className="empty-hint">{t('prompt.template.empty')}</p>
            )}
            <ul className="simple-list">
              {promptTemplates.map((tmpl) => (
                <li
                  key={tmpl.id}
                  className={activePromptTemplateId === tmpl.id ? 'selected' : ''}
                  style={{ cursor: 'pointer' }}
                  onClick={() => {
                    setActivePromptTemplateId(tmpl.id)
                    setPromptTemplateDraft(tmpl)
                    setPromptVariableValues({})
                    setPromptRunResult(null)
                  }}
                >
                  <span>{tmpl.name || tmpl.id}</span>
                  {activePromptTemplateId === tmpl.id && <span className="badge-ok">✓</span>}
                </li>
              ))}
            </ul>

            <div className="form-section" style={{ marginTop: '1rem' }}>
              <label className="field-label">{t('prompt.template.name')}</label>
              <input className="text-input" value={promptTemplateDraft.name} onChange={(e) => setPromptTemplateDraft((prev) => ({ ...prev, name: e.target.value }))} placeholder={t('prompt.template.name')} />
              <label className="field-label">{t('prompt.template.description')}</label>
              <input className="text-input" value={promptTemplateDraft.description} onChange={(e) => setPromptTemplateDraft((prev) => ({ ...prev, description: e.target.value }))} placeholder={t('prompt.template.description')} />
              <label className="field-label">{t('prompt.template.content')}</label>
              <textarea className="text-input" rows={6} value={promptTemplateDraft.content} onChange={(e) => setPromptTemplateDraft((prev) => ({ ...prev, content: e.target.value }))} placeholder={'{{display_name}}, {{role}} at {{company}}...'} />

              <label className="field-label" style={{ marginTop: '0.75rem' }}>{t('prompt.template.variables')}</label>
              {promptTemplateDraft.variables.map((v, i) => (
                <div key={i} style={{ display: 'flex', gap: '0.5rem', marginBottom: '0.25rem', alignItems: 'center' }}>
                  <input className="text-input" style={{ flex: 1 }} value={v.key} onChange={(e) => updateTemplateVariable(i, 'key', e.target.value)} placeholder={t('prompt.template.varKey')} />
                  <input className="text-input" style={{ flex: 2 }} value={v.label} onChange={(e) => updateTemplateVariable(i, 'label', e.target.value)} placeholder={t('prompt.template.varLabel')} />
                  <input className="text-input" style={{ flex: 2 }} value={v.placeholder} onChange={(e) => updateTemplateVariable(i, 'placeholder', e.target.value)} placeholder={t('prompt.template.varPlaceholder')} />
                  <button type="button" className="ghost-btn" onClick={() => removeTemplateVariable(i)}>✕</button>
                </div>
              ))}
              <button type="button" className="ghost-btn" style={{ marginTop: '0.25rem' }} onClick={addTemplateVariable}>{t('prompt.template.addVar')}</button>

              <div className="row-actions" style={{ marginTop: '0.5rem' }}>
                <button type="button" onClick={() => void handleSaveTemplate()} disabled={promptBusy || !centralHome}>{t('prompt.template.save')}</button>
                {activePromptTemplateId && (
                  <button type="button" className="danger-btn" onClick={() => void handleDeleteTemplate()} disabled={promptBusy}>{t('prompt.template.delete')}</button>
                )}
              </div>
            </div>
          </div>

          {/* ── 右欄：組合 & 執行 ── */}
          <div className="panel" style={{ flex: 1 }}>
            <h3>{t('prompt.run.title')}</h3>
            <p className="panel-sub">{t('prompt.run.subtitle')}</p>

            <div className="form-section">
              <label className="field-label">{t('prompt.run.selectProfile')}</label>
              <select className="text-input" value={activePromptProfileId} onChange={(e) => {
                setActivePromptProfileId(e.target.value)
                const p = promptProfiles.find((x) => x.id === e.target.value)
                if (p) setPromptProfileDraft(p)
              }}>
                <option value="">— {t('prompt.run.selectProfile')} —</option>
                {promptProfiles.map((p) => <option key={p.id} value={p.id}>{p.name || p.displayName}</option>)}
              </select>

              <label className="field-label">{t('prompt.run.selectTemplate')}</label>
              <select className="text-input" value={activePromptTemplateId} onChange={(e) => {
                setActivePromptTemplateId(e.target.value)
                const tmpl = promptTemplates.find((x) => x.id === e.target.value)
                if (tmpl) { setPromptTemplateDraft(tmpl); setPromptVariableValues({}); setPromptRunResult(null) }
              }}>
                <option value="">— {t('prompt.run.selectTemplate')} —</option>
                {promptTemplates.map((tmpl) => <option key={tmpl.id} value={tmpl.id}>{tmpl.name}</option>)}
              </select>

              {activeTemplate && activeTemplate.variables.map((v) => (
                <div key={v.key}>
                  <label className="field-label">{v.label || v.key}</label>
                  <input
                    className="text-input"
                    value={promptVariableValues[v.key] ?? ''}
                    onChange={(e) => setPromptVariableValues((prev) => ({ ...prev, [v.key]: e.target.value }))}
                    placeholder={v.placeholder}
                  />
                </div>
              ))}

              <label className="field-label">{t('prompt.run.provider')}</label>
              <select className="text-input" value={promptProvider} onChange={(e) => setPromptProvider(e.target.value)}>
                <option value="local">local</option>
                <option value="openai">openai</option>
                <option value="gemini">gemini</option>
                <option value="claude">claude</option>
              </select>

              {preview && (
                <>
                  <label className="field-label" style={{ marginTop: '0.75rem' }}>{t('prompt.run.preview')}</label>
                  <pre className="code-block" style={{ whiteSpace: 'pre-wrap', maxHeight: '160px', overflowY: 'auto', fontSize: '0.8rem' }}>{preview}</pre>
                </>
              )}

              <button type="button" onClick={() => void handleRunPrompt()} disabled={promptBusy || !centralHome || !activePromptProfileId || !activePromptTemplateId} style={{ marginTop: '0.75rem' }}>
                {promptBusy ? '...' : t('prompt.run.call')}
              </button>
            </div>

            {promptRunResult && (
              <div className="form-section" style={{ marginTop: '1rem' }}>
                <div className="panel-header">
                  <label className="field-label">{t('prompt.run.result')}</label>
                  <button type="button" className="ghost-btn" onClick={() => void navigator.clipboard.writeText(promptRunResult.result)}>{t('prompt.run.copy')}</button>
                </div>
                <pre className="code-block" style={{ whiteSpace: 'pre-wrap', maxHeight: '320px', overflowY: 'auto', fontSize: '0.85rem' }}>{promptRunResult.result}</pre>
              </div>
            )}
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
          {activeTab === 'dashboard' && (
            <DashboardTab
              centralHome={centralHome}
              t={t}
              onNavigateToRecords={handleDashboardNavigateToRecords}
            />
          )}
          {activeTab === 'records' && renderRecords()}
          {activeTab === 'timeline' && <TimelineTab centralHome={centralHome} t={t} />}
          {activeTab === 'logs' && <LogsTab centralHome={centralHome} t={t} />}
          {activeTab === 'ai' && (
            <AiTab
              centralHome={centralHome}
              t={t}
              appSettings={appSettings}
              pushNotice={pushNotice}
            />
          )}
          {activeTab === 'prompt' && renderPromptService()}
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
