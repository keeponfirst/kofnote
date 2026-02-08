import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  clearNotionApiKey,
  clearOpenaiApiKey,
  deleteRecord,
  exportMarkdownReport,
  getAppSettings,
  getDashboardStats,
  getHealthDiagnostics,
  getHomeFingerprint,
  hasNotionApiKey,
  hasOpenaiApiKey,
  listLogs,
  listRecords,
  notebooklmAddRecordSource,
  notebooklmAsk,
  notebooklmCreateNotebook,
  notebooklmHealthCheck,
  notebooklmListNotebooks,
  pullRecordsFromNotion,
  rebuildSearchIndex,
  resolveCentralHome,
  runAiAnalysis,
  saveAppSettings,
  setNotionApiKey,
  searchRecords,
  syncRecordBidirectional,
  syncRecordsBidirectional,
  setOpenaiApiKey,
  upsertRecord,
} from './lib/tauri'
import type {
  AiProvider,
  AppSettings,
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
} from './types'

type TabKey = 'dashboard' | 'records' | 'logs' | 'ai' | 'integrations' | 'settings' | 'health'

type Notice = {
  id: number
  type: 'info' | 'success' | 'error'
  text: string
}

type SearchMeta = {
  indexed: boolean
  total: number
  tookMs: number
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

const LOCAL_STORAGE_KEY = 'kofnote.centralHome'
const DEFAULT_MODEL = 'gpt-4.1-mini'
const RECORD_TYPES: RecordType[] = ['decision', 'worklog', 'idea', 'backlog', 'note']
const TAB_ITEMS: Array<{ key: TabKey; label: string }> = [
  { key: 'dashboard', label: 'Dashboard' },
  { key: 'records', label: 'Records' },
  { key: 'logs', label: 'Logs' },
  { key: 'ai', label: 'AI' },
  { key: 'integrations', label: 'Integrations' },
  { key: 'settings', label: 'Settings' },
  { key: 'health', label: 'Health' },
]

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
  })
  const [selectedProfileId, setSelectedProfileId] = useState<string>('')
  const [profileDraft, setProfileDraft] = useState<WorkspaceProfile>(makeProfile())

  const [openaiKeyDraft, setOpenaiKeyDraft] = useState('')
  const [hasOpenaiKey, setHasOpenaiKey] = useState(false)
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
  const [commandOpen, setCommandOpen] = useState(false)
  const [commandQuery, setCommandQuery] = useState('')
  const [notices, setNotices] = useState<Notice[]>([])

  const commandInputRef = useRef<HTMLInputElement | null>(null)

  const pushNotice = useCallback((type: Notice['type'], text: string) => {
    const id = Date.now() + Math.floor(Math.random() * 1000)
    setNotices((prev) => [...prev, { id, type, text }])
    window.setTimeout(() => {
      setNotices((prev) => prev.filter((item) => item.id !== id))
    }, 3200)
  }, [])

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
    setSearchMeta({ indexed: result.indexed, total: result.total, tookMs: result.tookMs })
    setVisibleCount(200)
  }, [allRecords, centralHome, recordDateFrom, recordDateTo, recordFilterType, recordKeyword])

  const loadCentralHome = useCallback(
    async (input = centralHomeInput) => {
      if (!input.trim()) {
        pushNotice('error', 'Central Home path is required.')
        return
      }

      await withBusy(async () => {
        const resolved = await resolveCentralHome(input.trim())
        setCentralHome(resolved.centralHome)
        setCentralHomeInput(resolved.centralHome)
        localStorage.setItem(LOCAL_STORAGE_KEY, resolved.centralHome)

        const data = await refreshCore(resolved.centralHome)
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
            ? `Loaded and normalized to ${resolved.centralHome}`
            : `Loaded ${resolved.centralHome}`,
        )
      })
    },
    [centralHomeInput, pushNotice, refreshCore, withBusy],
  )

  const loadSettings = useCallback(async () => {
    const [settings, hasKey, notionKey] = await Promise.all([
      getAppSettings(),
      hasOpenaiApiKey(),
      hasNotionApiKey(),
    ])
    setAppSettings(settings)
    setHasOpenaiKey(hasKey)
    setHasNotionKey(notionKey)
    setSelectedNotebookId(settings.integrations.notebooklm.defaultNotebookId ?? '')

    if (settings.activeProfileId) {
      setSelectedProfileId(settings.activeProfileId)
      const active = settings.profiles.find((item) => item.id === settings.activeProfileId)
      if (active) {
        setProfileDraft(active)
      }
    }

    return settings
  }, [])

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
        pushNotice('error', String(error))
      }
    })()
  }, [loadCentralHome, loadSettings, pushNotice])

  useEffect(() => {
    if (!centralHome) {
      return
    }

    const timer = window.setTimeout(() => {
      void applySearch().catch((error) => pushNotice('error', String(error)))
    }, 250)

    return () => window.clearTimeout(timer)
  }, [applySearch, centralHome, pushNotice])

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
            pushNotice('info', 'Auto refreshed after external updates.')
          }
          setFingerprint(next)
        } catch {
          // Ignore polling failures.
        }
      })()
    }, interval)

    return () => window.clearInterval(handle)
  }, [appSettings.pollIntervalSec, centralHome, fingerprint, pushNotice, refreshCore])

  const handleSaveRecord = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', 'Load Central Home first.')
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

      pushNotice('success', 'Record saved.')
    })
  }, [centralHome, pushNotice, recordForm, refreshCore, selectedRecordPath, withBusy])

  const handleDeleteRecord = useCallback(async () => {
    if (!centralHome || !selectedRecordPath) {
      pushNotice('error', 'Select a record first.')
      return
    }

    if (!window.confirm('Delete selected record?')) {
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

      pushNotice('success', 'Record deleted.')
    })
  }, [centralHome, pushNotice, refreshCore, selectedRecordPath, withBusy])

  const handleRunAi = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', 'Load Central Home first.')
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
      pushNotice('success', `${result.provider} analysis completed.`)
    })
  }, [aiIncludeLogs, aiMaxRecords, aiModel, aiPrompt, aiProvider, centralHome, pushNotice, withBusy])

  const handleSaveApiKey = useCallback(async () => {
    if (!openaiKeyDraft.trim()) {
      pushNotice('error', 'API key cannot be empty.')
      return
    }

    await withBusy(async () => {
      await setOpenaiApiKey(openaiKeyDraft.trim())
      setOpenaiKeyDraft('')
      setHasOpenaiKey(true)
      pushNotice('success', 'OpenAI API key saved to Keychain.')
    })
  }, [openaiKeyDraft, pushNotice, withBusy])

  const handleClearApiKey = useCallback(async () => {
    await withBusy(async () => {
      await clearOpenaiApiKey()
      setHasOpenaiKey(false)
      pushNotice('success', 'OpenAI API key cleared.')
    })
  }, [pushNotice, withBusy])

  const handleSaveNotionKey = useCallback(async () => {
    if (!notionKeyDraft.trim()) {
      pushNotice('error', 'Notion API key cannot be empty.')
      return
    }

    await withBusy(async () => {
      await setNotionApiKey(notionKeyDraft.trim())
      setNotionKeyDraft('')
      setHasNotionKey(true)
      pushNotice('success', 'Notion API key saved to Keychain.')
    })
  }, [notionKeyDraft, pushNotice, withBusy])

  const handleClearNotionKey = useCallback(async () => {
    await withBusy(async () => {
      await clearNotionApiKey()
      setHasNotionKey(false)
      pushNotice('success', 'Notion API key cleared.')
    })
  }, [pushNotice, withBusy])

  const handleSyncSelectedToNotion = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', 'Load Central Home first.')
      return
    }
    if (!selectedRecordPath) {
      pushNotice('error', 'Select a record first.')
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
        pushNotice('error', result.notionError || 'Conflict detected.')
      } else if (result.notionSyncStatus === 'SUCCESS') {
        pushNotice('success', `Notion sync (${result.action}) done.`)
      } else {
        pushNotice('error', result.notionError || 'Notion sync failed.')
      }
    })
  }, [
    appSettings.integrations.notion.databaseId,
    centralHome,
    notionConflictStrategy,
    pushNotice,
    refreshCore,
    selectedRecordPath,
    withBusy,
  ])

  const handleSyncVisibleToNotion = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', 'Load Central Home first.')
      return
    }

    const targets = displayedRecords
      .map((item) => item.jsonPath)
      .filter((item): item is string => Boolean(item))

    if (targets.length === 0) {
      pushNotice('error', 'No record json paths available in current view.')
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
        `Notion batch sync done. success=${result.success}, failed=${result.failed}, conflicts=${result.conflicts}.`,
      )
    })
  }, [
    appSettings.integrations.notion.databaseId,
    centralHome,
    displayedRecords,
    notionConflictStrategy,
    pushNotice,
    refreshCore,
    withBusy,
  ])

  const handlePullFromNotion = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', 'Load Central Home first.')
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
        `Pulled from Notion. success=${result.success}, failed=${result.failed}, conflicts=${result.conflicts}.`,
      )
    })
  }, [
    appSettings.integrations.notion.databaseId,
    centralHome,
    notionConflictStrategy,
    pushNotice,
    refreshCore,
    withBusy,
  ])

  const handleNotebookHealth = useCallback(async () => {
    await withBusy(async () => {
      const result = await notebooklmHealthCheck()
      setNotebookHealthText(JSON.stringify(result, null, 2))
      pushNotice('success', 'NotebookLM health checked.')
    })
  }, [pushNotice, withBusy])

  const handleNotebookList = useCallback(async () => {
    await withBusy(async () => {
      const notebooks = await notebooklmListNotebooks({ limit: 30 })
      setNotebookList(notebooks)
      if (!selectedNotebookId && notebooks.length > 0) {
        setSelectedNotebookId(notebooks[0].id)
      }
      pushNotice('success', `Loaded ${notebooks.length} notebooks.`)
    })
  }, [pushNotice, selectedNotebookId, withBusy])

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
      pushNotice('success', `Notebook created: ${created.name}`)
    })
  }, [appSettings, newNotebookTitle, pushNotice, withBusy])

  const handleNotebookSetDefault = useCallback(async () => {
    if (!selectedNotebookId) {
      pushNotice('error', 'Select a notebook first.')
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
      pushNotice('success', 'Default NotebookLM notebook updated.')
    })
  }, [appSettings, pushNotice, selectedNotebookId, withBusy])

  const handleAddSelectedRecordToNotebook = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', 'Load Central Home first.')
      return
    }
    if (!selectedRecord?.jsonPath) {
      pushNotice('error', 'Select a record first.')
      return
    }
    if (!selectedNotebookId) {
      pushNotice('error', 'Select a notebook first.')
      return
    }

    await withBusy(async () => {
      const jsonPath = selectedRecord.jsonPath
      if (!jsonPath) {
        pushNotice('error', 'Selected record has no jsonPath.')
        return
      }
      await notebooklmAddRecordSource({
        centralHome,
        jsonPath,
        notebookId: selectedNotebookId,
      })
      pushNotice('success', 'Selected record sent to NotebookLM as text source.')
    })
  }, [centralHome, pushNotice, selectedNotebookId, selectedRecord, withBusy])

  const handleNotebookAsk = useCallback(async () => {
    if (!selectedNotebookId) {
      pushNotice('error', 'Select a notebook first.')
      return
    }
    if (!notebookQuestion.trim()) {
      pushNotice('error', 'Question cannot be empty.')
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
      pushNotice('success', 'NotebookLM answered.')
    })
  }, [notebookQuestion, pushNotice, selectedNotebookId, withBusy])

  const handleRebuildIndex = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', 'Load Central Home first.')
      return
    }

    await withBusy(async () => {
      const result = await rebuildSearchIndex(centralHome)
      pushNotice(
        'success',
        `Indexed ${result.indexedCount} records in ${result.tookMs} ms.`,
      )
      setHealth(await getHealthDiagnostics(centralHome))
    })
  }, [centralHome, pushNotice, withBusy])

  const handleExportReport = useCallback(async () => {
    if (!centralHome) {
      pushNotice('error', 'Load Central Home first.')
      return
    }

    await withBusy(async () => {
      const result = await exportMarkdownReport({
        centralHome,
        outputPath: reportPath.trim() || undefined,
        title: reportTitle.trim() || undefined,
        recentDays: reportDays,
      })
      pushNotice('success', `Report exported: ${result.outputPath}`)
    })
  }, [centralHome, pushNotice, reportDays, reportPath, reportTitle, withBusy])

  const handleSaveSettings = useCallback(async (next: AppSettings) => {
    await withBusy(async () => {
      const saved = await saveAppSettings(next)
      setAppSettings(saved)
      if (saved.activeProfileId) {
        setSelectedProfileId(saved.activeProfileId)
      }
      pushNotice('success', 'Settings saved.')
    })
  }, [pushNotice, withBusy])

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
          setActiveTab(nextTab.key)
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
        label: 'Load Central Home',
        run: () => void loadCentralHome(),
      },
      {
        id: 'cmd-refresh',
        label: 'Refresh Data',
        run: () => {
          if (centralHome) {
            void withBusy(async () => {
              const data = await refreshCore(centralHome)
              setDisplayedRecords(data.records)
              setSearchMeta(null)
              pushNotice('success', 'Data refreshed.')
            })
          }
        },
      },
      {
        id: 'cmd-new-record',
        label: 'New Record',
        run: () => {
          setSelectedRecordPath(null)
          setRecordForm(emptyForm())
          setActiveTab('records')
        },
      },
      {
        id: 'cmd-save-record',
        label: 'Save Record',
        run: () => void handleSaveRecord(),
      },
      {
        id: 'cmd-sync-record-notion',
        label: 'Bidirectional Sync Selected Record',
        run: () => void handleSyncSelectedToNotion(),
      },
      {
        id: 'cmd-pull-notion',
        label: 'Pull Latest from Notion Database',
        run: () => void handlePullFromNotion(),
      },
      {
        id: 'cmd-rebuild-index',
        label: 'Rebuild Search Index',
        run: () => void handleRebuildIndex(),
      },
      {
        id: 'cmd-run-local-ai',
        label: 'Run Local AI Analysis',
        run: () => {
          setAiProvider('local')
          setActiveTab('ai')
          void handleRunAi()
        },
      },
      {
        id: 'cmd-export-report',
        label: 'Export Markdown Report',
        run: () => void handleExportReport(),
      },
      {
        id: 'cmd-list-notebooks',
        label: 'Refresh NotebookLM List',
        run: () => void handleNotebookList(),
      },
      ...TAB_ITEMS.map((item) => ({
        id: `cmd-tab-${item.key}`,
        label: `Go to ${item.label}`,
        run: () => setActiveTab(item.key),
      })),
    ],
    [
      centralHome,
      handleExportReport,
      handleNotebookList,
      handlePullFromNotion,
      handleRebuildIndex,
      handleRunAi,
      handleSaveRecord,
      handleSyncSelectedToNotion,
      loadCentralHome,
      pushNotice,
      refreshCore,
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
      return <div className="panel">Load a Central Home to see metrics.</div>
    }

    return (
      <div className="dashboard-grid">
        <div className="kpi-row">
          <div className="kpi-card">
            <p className="kpi-label">Records</p>
            <p className="kpi-value">{stats.totalRecords}</p>
          </div>
          <div className="kpi-card">
            <p className="kpi-label">Logs</p>
            <p className="kpi-value">{stats.totalLogs}</p>
          </div>
          <div className="kpi-card">
            <p className="kpi-label">Pending Sync</p>
            <p className="kpi-value">{stats.pendingSyncCount}</p>
          </div>
        </div>

        <div className="panel-grid-2">
          <div className="panel">
            <h3>Type Distribution</h3>
            <ul className="simple-list">
              {RECORD_TYPES.map((item) => (
                <li key={item}>
                  <span>{item}</span>
                  <strong>{stats.typeCounts[item] ?? 0}</strong>
                </li>
              ))}
            </ul>
          </div>

          <div className="panel">
            <h3>Recent 7 Days</h3>
            <ul className="simple-list">
              {stats.recentDailyCounts.map((row) => (
                <li key={row.date}>
                  <span>{row.date}</span>
                  <strong>{row.count}</strong>
                </li>
              ))}
            </ul>
          </div>
        </div>

        <div className="panel">
          <h3>Top Tags</h3>
          {stats.topTags.length === 0 ? (
            <p className="muted">No tags yet.</p>
          ) : (
            <div className="tag-grid">
              {stats.topTags.map((item) => (
                <span key={item.tag} className="tag-chip">
                  {item.tag} ({item.count})
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    )
  }

  function renderRecords() {
    return (
      <div className="records-layout">
        <div className="panel left-panel">
          <div className="toolbar-row">
            <button
              type="button"
              onClick={() => {
                setSelectedRecordPath(null)
                setRecordForm(emptyForm())
              }}
            >
              New
            </button>
            <button type="button" onClick={() => void handleSaveRecord()} disabled={busy}>
              Save
            </button>
            <button type="button" onClick={() => void handleDeleteRecord()} disabled={busy || !selectedRecordPath}>
              Delete
            </button>
          </div>

          <div className="records-filter-grid">
            <select
              value={recordFilterType}
              onChange={(event) => setRecordFilterType(event.target.value as 'all' | RecordType)}
            >
              <option value="all">all</option>
              {RECORD_TYPES.map((item) => (
                <option key={item} value={item}>
                  {item}
                </option>
              ))}
            </select>
            <input
              placeholder="Keyword (title/body/tags)"
              value={recordKeyword}
              onChange={(event) => setRecordKeyword(event.target.value)}
            />
            <input type="date" value={recordDateFrom} onChange={(event) => setRecordDateFrom(event.target.value)} />
            <input type="date" value={recordDateTo} onChange={(event) => setRecordDateTo(event.target.value)} />
          </div>

          <div className="meta-row">
            <span>
              {searchMeta
                ? `Result ${displayedRecords.length}/${searchMeta.total} · ${searchMeta.indexed ? 'FTS' : 'memory'} · ${searchMeta.tookMs}ms`
                : `Total ${displayedRecords.length}`}
            </span>
          </div>

          <div className="record-list">
            {visibleRecords.map((item) => {
              const selected = item.jsonPath === selectedRecordPath
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
                </button>
              )
            })}
            {displayedRecords.length === 0 && <p className="muted">No records.</p>}
          </div>

          {visibleCount < displayedRecords.length && (
            <button
              type="button"
              className="ghost-btn"
              onClick={() => setVisibleCount((prev) => prev + 200)}
            >
              Load more ({displayedRecords.length - visibleCount} remaining)
            </button>
          )}
        </div>

        <div className="panel right-panel">
          <div className="form-grid">
            <label>
              Type
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
              Title
              <input
                value={recordForm.title}
                onChange={(event) => setRecordForm((prev) => ({ ...prev, title: event.target.value }))}
              />
            </label>

            <label>
              Created At (ISO)
              <input
                value={recordForm.createdAt}
                onChange={(event) => setRecordForm((prev) => ({ ...prev, createdAt: event.target.value }))}
              />
            </label>

            <label>
              Date
              <input
                type="date"
                value={recordForm.date}
                onChange={(event) => setRecordForm((prev) => ({ ...prev, date: event.target.value }))}
              />
            </label>

            <label>
              Tags (comma)
              <input
                value={recordForm.tagsText}
                onChange={(event) => setRecordForm((prev) => ({ ...prev, tagsText: event.target.value }))}
              />
            </label>

            <label>
              Sync Status
              <select
                value={recordForm.notionSyncStatus}
                onChange={(event) =>
                  setRecordForm((prev) => ({ ...prev, notionSyncStatus: event.target.value }))
                }
              >
                <option value="SUCCESS">SUCCESS</option>
                <option value="PENDING">PENDING</option>
                <option value="FAILED">FAILED</option>
              </select>
            </label>

            <label>
              Notion URL
              <input
                value={recordForm.notionUrl}
                onChange={(event) => setRecordForm((prev) => ({ ...prev, notionUrl: event.target.value }))}
              />
            </label>

            <label>
              Notion Page ID
              <input
                value={recordForm.notionPageId}
                onChange={(event) => setRecordForm((prev) => ({ ...prev, notionPageId: event.target.value }))}
              />
            </label>

            <label>
              Notion Error
              <input
                value={recordForm.notionError}
                onChange={(event) => setRecordForm((prev) => ({ ...prev, notionError: event.target.value }))}
              />
            </label>
          </div>

          <label className="block-field">
            Final Body
            <textarea
              value={recordForm.finalBody}
              rows={12}
              onChange={(event) => setRecordForm((prev) => ({ ...prev, finalBody: event.target.value }))}
            />
          </label>

          <label className="block-field">
            Source Text
            <textarea
              value={recordForm.sourceText}
              rows={8}
              onChange={(event) => setRecordForm((prev) => ({ ...prev, sourceText: event.target.value }))}
            />
          </label>
        </div>
      </div>
    )
  }

  function renderLogs() {
    return (
      <div className="logs-layout">
        <div className="panel left-panel">
          <div className="record-list">
            {logs.map((item, index) => (
              <button
                type="button"
                key={item.jsonPath ?? `${item.timestamp}-${item.eventId}-${index}`}
                className={selectedLogIndex === index ? 'record-item selected' : 'record-item'}
                onClick={() => setSelectedLogIndex(index)}
              >
                <p>{item.timestamp.slice(0, 19)}</p>
                <p>
                  <strong>{item.taskIntent || '-'}</strong> | {item.status || '-'}
                </p>
                <p>{item.title || '(no title)'}</p>
              </button>
            ))}
            {logs.length === 0 && <p className="muted">No log entries.</p>}
          </div>
        </div>

        <div className="panel right-panel">
          <h3>Log Detail</h3>
          <pre className="json-preview">{selectedLog ? JSON.stringify(selectedLog.raw, null, 2) : '{}'}</pre>
        </div>
      </div>
    )
  }

  function renderAi() {
    return (
      <div className="panel settings-panel">
        <h3>AI Analysis</h3>

        <div className="ai-controls-grid">
          <label>
            Provider
            <select value={aiProvider} onChange={(event) => setAiProvider(event.target.value as AiProvider)}>
              <option value="local">local</option>
              <option value="openai">openai</option>
            </select>
          </label>

          <label>
            Model
            <input value={aiModel} onChange={(event) => setAiModel(event.target.value)} />
          </label>

          <label>
            Max Records
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
            include logs
          </label>
        </div>

        <label className="block-field">
          Prompt
          <textarea value={aiPrompt} rows={6} onChange={(event) => setAiPrompt(event.target.value)} />
        </label>

        <div className="toolbar-row two-col">
          <button type="button" onClick={() => void handleRunAi()} disabled={busy}>
            Run Analysis
          </button>
          <button type="button" className="ghost-btn" onClick={() => navigator.clipboard.writeText(aiResult || '')}>
            Copy Result
          </button>
        </div>

        <label className="block-field">
          Output
          <textarea value={aiResult} rows={16} onChange={(event) => setAiResult(event.target.value)} />
        </label>
      </div>
    )
  }

  function renderIntegrations() {
    return (
      <div className="settings-layout">
        <div className="panel left-panel">
          <h3>Notion Connector</h3>
          <div className="form-grid two-col-grid">
            <label className="span-2">
              Notion Database ID
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
              Enable Notion sync
            </label>

            <label>
              Key Status
              <input value={hasNotionKey ? 'configured' : 'not set'} readOnly />
            </label>
          </div>

          <div className="form-grid">
            <label>
              Notion API Key (saved to Keychain)
              <input
                type="password"
                value={notionKeyDraft}
                onChange={(event) => setNotionKeyDraft(event.target.value)}
                placeholder={hasNotionKey ? 'Key already configured' : 'secret_...'}
              />
            </label>
          </div>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleSaveNotionKey()}>
              Save Key
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleClearNotionKey()}>
              Clear Key
            </button>
          </div>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleSaveSettings(appSettings)} disabled={busy}>
              Save Connector Settings
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handlePullFromNotion()} disabled={busy}>
              Pull From Notion
            </button>
          </div>

          <div className="form-grid">
            <label>
              Conflict Strategy
              <select
                value={notionConflictStrategy}
                onChange={(event) => setNotionConflictStrategy(event.target.value as NotionConflictStrategy)}
              >
                <option value="manual">manual (mark conflict)</option>
                <option value="local_wins">local_wins (push local)</option>
                <option value="notion_wins">notion_wins (pull notion)</option>
              </select>
            </label>
          </div>

          <div className="toolbar-row two-col">
            <button type="button" className="ghost-btn" onClick={() => void handleSyncSelectedToNotion()} disabled={busy}>
              Bidirectional Sync Selected
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleSyncVisibleToNotion()} disabled={busy}>
              Bidirectional Sync View ({displayedRecords.length})
            </button>
          </div>

          {notionSyncReport && (
            <>
              <hr className="separator" />
              <h3>Notion Sync Report</h3>
              <pre className="json-preview">{notionSyncReport}</pre>
            </>
          )}
        </div>

        <div className="panel right-panel">
          <h3>NotebookLM Connector</h3>

          <div className="form-grid two-col-grid">
            <label>
              MCP Command
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
              MCP Args (space separated)
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
              Health Check
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleNotebookList()} disabled={busy}>
              Refresh Notebooks
            </button>
          </div>

          <div className="form-grid two-col-grid">
            <label>
              New Notebook Title
              <input
                value={newNotebookTitle}
                onChange={(event) => setNewNotebookTitle(event.target.value)}
                placeholder="KOF Note - Weekly Analysis"
              />
            </label>
            <label>
              Selected Notebook
              <select value={selectedNotebookId} onChange={(event) => setSelectedNotebookId(event.target.value)}>
                <option value="">(choose one)</option>
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
              Create Notebook
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleNotebookSetDefault()} disabled={busy}>
              Set Default Notebook
            </button>
          </div>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleAddSelectedRecordToNotebook()} disabled={busy}>
              Add Selected Record Source
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleSaveSettings(appSettings)} disabled={busy}>
              Save MCP Config
            </button>
          </div>

          <label className="block-field">
            Ask NotebookLM
            <textarea
              rows={5}
              value={notebookQuestion}
              onChange={(event) => setNotebookQuestion(event.target.value)}
            />
          </label>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleNotebookAsk()} disabled={busy}>
              Ask
            </button>
            <button
              type="button"
              className="ghost-btn"
              onClick={() => navigator.clipboard.writeText(notebookAnswer || '')}
            >
              Copy Answer
            </button>
          </div>

          <label className="block-field">
            NotebookLM Answer
            <textarea rows={10} value={notebookAnswer} onChange={(event) => setNotebookAnswer(event.target.value)} />
          </label>

          {notebookCitations.length > 0 && (
            <div className="panel">
              <h3>Citations</h3>
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
              <h3>Health Output</h3>
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
          <h3>Profiles</h3>
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
            {appSettings.profiles.length === 0 && <p className="muted">No profiles yet.</p>}
          </div>

          <div className="toolbar-row two-col">
            <button
              type="button"
              className="ghost-btn"
              onClick={() => {
                const next = makeProfile(`Profile ${appSettings.profiles.length + 1}`)
                setProfileDraft(next)
                setSelectedProfileId(next.id)
              }}
            >
              New Profile
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
              Apply Profile
            </button>
          </div>
        </div>

        <div className="panel right-panel">
          <h3>Profile Editor</h3>

          <div className="form-grid two-col-grid">
            <label>
              ID
              <input value={profileDraft.id} readOnly />
            </label>
            <label>
              Name
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
              Central Home
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
              Default Provider
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
              Default Model
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
              Poll Interval (sec)
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
              Save Profile
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
              Delete Profile
            </button>
          </div>

          <hr className="separator" />

          <h3>OpenAI Keychain</h3>
          <div className="form-grid two-col-grid">
            <label>
              API Key (saved to Keychain)
              <input
                type="password"
                value={openaiKeyDraft}
                onChange={(event) => setOpenaiKeyDraft(event.target.value)}
                placeholder={hasOpenaiKey ? 'Key already configured' : 'sk-...'}
              />
            </label>
            <label>
              Key Status
              <input value={hasOpenaiKey ? 'configured' : 'not set'} readOnly />
            </label>
          </div>
          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleSaveApiKey()}>
              Save Key
            </button>
            <button type="button" className="ghost-btn" onClick={() => void handleClearApiKey()}>
              Clear Key
            </button>
          </div>
        </div>
      </div>
    )
  }

  function renderHealth() {
    return (
      <div className="settings-layout">
        <div className="panel left-panel">
          <h3>Health Snapshot</h3>
          <ul className="simple-list">
            <li>
              <span>Central Home</span>
              <strong className="align-right">{health?.centralHome || '-'}</strong>
            </li>
            <li>
              <span>Records / Logs</span>
              <strong>
                {health?.recordsCount ?? 0} / {health?.logsCount ?? 0}
              </strong>
            </li>
            <li>
              <span>Index</span>
              <strong>
                {health?.indexExists ? `ready (${health.indexedRecords})` : 'not built'}
              </strong>
            </li>
            <li>
              <span>OpenAI key</span>
              <strong>{health?.hasOpenaiApiKey ? 'configured' : 'not set'}</strong>
            </li>
            <li>
              <span>Profiles</span>
              <strong>{health?.profileCount ?? 0}</strong>
            </li>
          </ul>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleRebuildIndex()} disabled={!centralHome || busy}>
              Rebuild Index
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
                  pushNotice('success', 'Health diagnostics refreshed.')
                })
              }}
              disabled={!centralHome || busy}
            >
              Refresh Health
            </button>
          </div>
        </div>

        <div className="panel right-panel">
          <h3>Export Report</h3>
          <div className="form-grid two-col-grid">
            <label>
              Title
              <input value={reportTitle} onChange={(event) => setReportTitle(event.target.value)} />
            </label>
            <label>
              Recent Days
              <input
                type="number"
                min={1}
                max={365}
                value={reportDays}
                onChange={(event) => setReportDays(Number(event.target.value) || 7)}
              />
            </label>
            <label className="span-2">
              Output Path (optional)
              <input value={reportPath} onChange={(event) => setReportPath(event.target.value)} />
            </label>
          </div>
          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleExportReport()} disabled={!centralHome || busy}>
              Export Markdown
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
              Reset
            </button>
          </div>

          <hr className="separator" />

          <h3>Home Fingerprint</h3>
          <pre className="json-preview">{JSON.stringify(fingerprint ?? {}, null, 2)}</pre>
        </div>
      </div>
    )
  }

  return (
    <div className="workbench-root">
      <aside className="sidebar">
        <h2>KOF Note</h2>
        <p className="muted">Desktop Console</p>

        <div className="tab-list">
          {TAB_ITEMS.map((tab) => (
            <button
              key={tab.key}
              type="button"
              className={activeTab === tab.key ? 'tab-btn active' : 'tab-btn'}
              onClick={() => setActiveTab(tab.key)}
            >
              {tab.label}
            </button>
          ))}
        </div>

        <div className="sidebar-meta">
          <p>
            <strong>Active Home</strong>
          </p>
          <code>{centralHome || '-'}</code>
        </div>

        <button type="button" className="ghost-btn" onClick={() => setCommandOpen(true)}>
          Command Palette (⌘K)
        </button>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <input
            value={centralHomeInput}
            onChange={(event) => setCentralHomeInput(event.target.value)}
            placeholder="Enter central home path"
          />
          <button type="button" onClick={() => void loadCentralHome()} disabled={busy}>
            Load
          </button>
          <button
            type="button"
            onClick={() => {
              if (centralHome) {
                void withBusy(async () => {
                  const data = await refreshCore(centralHome)
                  setDisplayedRecords(data.records)
                  setSearchMeta(null)
                  pushNotice('success', 'Data refreshed.')
                })
              }
            }}
            disabled={!centralHome || busy}
          >
            Refresh
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
              placeholder="Type a command..."
            />
            <div className="palette-list">
              {filteredCommands.map((item) => (
                <button key={item.id} type="button" onClick={() => runCommand(item.id)}>
                  {item.label}
                </button>
              ))}
              {filteredCommands.length === 0 && <p className="muted">No matching command.</p>}
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export default App
