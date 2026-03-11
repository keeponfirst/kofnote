import { useCallback, useEffect, useMemo, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import {
  listDebateRuns,
  replayDebateMode,
  runAiAnalysis,
  runDebateMode,
} from '../lib/tauri'
import { buildProviderRegistrySettings, ProviderRegistry } from '../lib/providerRegistry'
import {
  DEBATE_ROLES,
  DEFAULT_DEBATE_MODEL_BY_PROVIDER,
  DEFAULT_MODEL,
} from '../constants'
import type {
  AiProvider,
  AppSettings,
  DebateModeResponse,
  DebateOutputType,
  DebateProgress,
  DebateReplayResponse,
  DebateRunSummary,
} from '../types'

type AiTabProps = {
  centralHome: string
  t: (key: string, fallbackOrValues?: string | Record<string, string | number>) => string
  appSettings: AppSettings
  pushNotice: (type: 'success' | 'error' | 'info', msg: string) => void
}

function cleanInsightLine(line: string): string {
  return line.replace(/^[\s\-*•\d.)]+/, '').trim()
}

type AiInsights = {
  summary: string[]
  risks: string[]
  actions: string[]
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

function getDebateModelDefault(providerId: string): string {
  return DEFAULT_DEBATE_MODEL_BY_PROVIDER[providerId] ?? 'auto'
}

export default function AiTab({ centralHome, t, appSettings, pushNotice }: AiTabProps) {
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
  const [busy, setBusy] = useState(false)
  const [debateBusy, setDebateBusy] = useState(false)

  const debateProviderRegistry = useMemo(
    () => new ProviderRegistry(buildProviderRegistrySettings(appSettings.providerRegistry)),
    [appSettings.providerRegistry],
  )
  const debateProviderOptions = useMemo(() => {
    const enabled = debateProviderRegistry.list({ enabledOnly: true }).map((p) => p.id)
    return ['local', ...enabled.filter((id) => id !== 'local')]
  }, [debateProviderRegistry])
  const debateModelDefault = useMemo(
    () => getDebateModelDefault(debateProvider),
    [debateProvider],
  )
  const debateProviderLabel = useCallback(
    (providerId: string) => {
      if (providerId === 'local') return 'local'
      const provider = debateProviderRegistry.get(providerId)
      return provider ? `${provider.id} (${provider.type})` : providerId
    },
    [debateProviderRegistry],
  )
  const debateProviderRuntimeHint = useMemo(() => {
    if (debateProvider === 'codex-cli')
      return t('Provider runtime: codex exec (live).', 'Provider 執行模式：codex exec（即時）。')
    if (debateProvider === 'gemini-cli')
      return t('Provider runtime: gemini CLI one-shot (live).', 'Provider 執行模式：gemini CLI 單次執行（即時）。')
    if (debateProvider === 'claude-cli')
      return t('Provider runtime: claude CLI one-shot (live).', 'Provider 執行模式：claude CLI 單次執行（即時）。')
    if (debateProvider === 'local')
      return t('Provider runtime: local heuristic.', 'Provider 執行模式：本地 heuristic。')
    return t(
      'Provider runtime: local fallback stub (automation not wired yet).',
      'Provider 執行模式：本地 fallback stub（尚未接自動化執行）。',
    )
  }, [debateProvider, t])
  const isCliDebateProvider =
    debateProvider === 'codex-cli' || debateProvider === 'gemini-cli' || debateProvider === 'claude-cli'
  const debateModelHint = useMemo(
    () =>
      isCliDebateProvider
        ? t(
            'Model is optional for CLI providers. Leave blank to use your CLI/account default.',
            'CLI provider 的模型欄位可留空；留空會使用你 CLI/帳號的預設模型。',
          )
        : t(
            'Model can be left blank to use provider default.',
            '模型欄位可留空，系統會用 provider 預設值。',
          ),
    [isCliDebateProvider, t],
  )
  const insights = useMemo(() => extractAiInsights(aiResult), [aiResult])
  const cards: Array<{ id: 'summary' | 'risks' | 'actions'; title: string; items: string[]; empty: string }> =
    useMemo(
      () => [
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
      ],
      [insights, t],
    )

  const withBusy = useCallback(async <T,>(task: () => Promise<T>) => {
    setBusy(true)
    try {
      return await task()
    } finally {
      setBusy(false)
    }
  }, [])

  const refreshDebateRuns = useCallback(async (home: string) => {
    if (!home.trim()) {
      setDebateRuns([])
      return []
    }
    const runs = await listDebateRuns({ centralHome: home })
    setDebateRuns(runs)
    return runs
  }, [])

  useEffect(() => {
    if (!centralHome.trim()) {
      setDebateRuns([])
      return
    }
    listDebateRuns({ centralHome }).then(setDebateRuns).catch(() => setDebateRuns([]))
  }, [centralHome])

  useEffect(() => {
    let unlisten: (() => void) | undefined
    let cancelled = false
    listen<DebateProgress>('debate-progress', (event) => {
      const payload = event.payload
      setDebateProgress({
        round: payload.round,
        role: payload.role,
        turnIndex: payload.turnIndex,
        totalTurns: payload.totalTurns,
      })
    })
      .then((fn) => {
        if (cancelled) {
          fn()
          return
        }
        unlisten = fn
      })
      .catch(() => {})
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  useEffect(() => {
    if (!debateProviderOptions.includes(debateProvider)) {
      setDebateProvider('local')
    }
  }, [debateProvider, debateProviderOptions])

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
  }, [centralHome, aiProvider, aiModel, aiPrompt, aiIncludeLogs, aiMaxRecords, pushNotice, t, withBusy])

  const replayDebateByRunId = useCallback(
    async (runId: string) => {
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
    },
    [centralHome, debateBusy, pushNotice, t],
  )

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

  const aiWordCount = aiResult.trim() ? aiResult.trim().split(/\s+/).filter(Boolean).length : 0
  const debateConsensus = debateResult?.finalPacket.consensus
  const debateActions = debateResult?.finalPacket.nextActions ?? []
  const debateErrorCodes = debateResult?.errorCodes ?? []
  const replayIssues = debateReplayResult?.consistency.issues ?? []

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
            <textarea value={debateProblem} rows={4} onChange={(e) => setDebateProblem(e.target.value)} />
          </label>

          <label className="span-2">
            {t('Constraints (line or comma separated)', '約束（每行或逗號分隔）')}
            <textarea
              value={debateConstraintsText}
              rows={4}
              onChange={(e) => setDebateConstraintsText(e.target.value)}
            />
          </label>

          <label>
            {t('Output Type', '輸出類型')}
            <select
              value={debateOutputType}
              onChange={(e) => setDebateOutputType(e.target.value as DebateOutputType)}
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
            <select value={debateProvider} onChange={(e) => setDebateProvider(e.target.value)}>
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
              onChange={(e) => setDebateAdvancedMode(e.target.checked)}
            />
            {t('Advanced: per-role provider', '進階：每角色個別 Provider')}
          </label>

          <label>
            {t('Model', '模型')}
            <input
              value={debateModel}
              placeholder={debateModelDefault}
              onChange={(e) => setDebateModel(e.target.value)}
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
                      onChange={(e) =>
                        setDebatePerRoleProvider((prev) => ({ ...prev, [role]: e.target.value }))
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
                      onChange={(e) =>
                        setDebatePerRoleModel((prev) => ({ ...prev, [role]: e.target.value }))
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
              onChange={(e) => setDebateWritebackType(e.target.value as 'decision' | 'worklog')}
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
              onChange={(e) => setDebateMaxTurnSeconds(Number(e.target.value) || 35)}
            />
          </label>

          <label>
            {t('Max Turn Tokens', '單輪 Token 上限')}
            <input
              type="number"
              min={128}
              max={4096}
              value={debateMaxTurnTokens}
              onChange={(e) => setDebateMaxTurnTokens(Number(e.target.value) || 900)}
            />
          </label>

          <label className="span-2">
            {t('Run ID (for replay)', 'Run ID（用於 replay）')}
            <input value={debateRunId} onChange={(e) => setDebateRunId(e.target.value)} />
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
            onClick={() =>
              navigator.clipboard.writeText(JSON.stringify(debateResult?.finalPacket ?? {}, null, 2))
            }
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
        {replayIssues.length > 0 ? (
          <pre className="json-preview">{JSON.stringify(replayIssues, null, 2)}</pre>
        ) : null}
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
              <select value={aiProvider} onChange={(e) => setAiProvider(e.target.value as AiProvider)}>
                <option value="local">local</option>
                <option value="openai">openai</option>
                <option value="gemini">gemini</option>
                <option value="claude">claude</option>
              </select>
            </label>

            <label>
              {t('ai.field.model')}
              <input value={aiModel} onChange={(e) => setAiModel(e.target.value)} />
            </label>

            <label>
              {t('ai.field.maxRecords')}
              <input
                type="number"
                min={1}
                max={200}
                value={aiMaxRecords}
                onChange={(e) => setAiMaxRecords(Number(e.target.value) || 30)}
              />
            </label>

            <label className="checkbox-field">
              <input
                type="checkbox"
                checked={aiIncludeLogs}
                onChange={(e) => setAiIncludeLogs(e.target.checked)}
              />
              {t('ai.field.includeLogs')}
            </label>
          </div>

          <label className="block-field">
            {t('ai.field.prompt')}
            <textarea value={aiPrompt} rows={8} onChange={(e) => setAiPrompt(e.target.value)} />
          </label>

          <div className="toolbar-row two-col">
            <button type="button" onClick={() => void handleRunAi()} disabled={busy}>
              {t('ai.button.run')}
            </button>
            <button
              type="button"
              className="ghost-btn"
              onClick={() => navigator.clipboard.writeText(aiResult || '')}
            >
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
              <textarea value={aiResult} rows={22} onChange={(e) => setAiResult(e.target.value)} />
            </label>
          </div>
        </div>
      </div>
    </div>
  )
}
