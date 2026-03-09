# Codex 功能改進任務（F7 → F5，依優先順序）

依序執行，每完成一個跑驗證後再繼續下一個。

---

## F7：DebateLock panic 安全修正

### 問題

`kofnote-app/src-tauri/src/types.rs` L1536 的 `run_debate_mode`：如果 `spawn_blocking` 內部 panic（回傳 `JoinError`），L1553 的 `.map_err` 會把它轉成 `Err`，然後函式直接 return。L1557-1559 的 lock 清除不會被執行，導致 `DebateLock` 永久鎖住，之後所有 debate 都會被拒絕直到 app 重啟。

### 修正

把 lock 清除邏輯改成無論成功或失敗都執行。在 `run_debate_mode` 函式中（types.rs L1536-1562），改為：

```rust
pub(crate) async fn run_debate_mode(
    lock: tauri::State<'_, DebateLock>,
    central_home: String,
    request: DebateModeRequest,
) -> Result<DebateModeResponse, String> {
    // Check lock
    {
        let guard = lock.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        if guard.is_some() {
            return Err("Another debate is already running".to_string());
        }
    }
    // Set lock
    {
        let mut guard = lock.0.lock().map_err(|e| format!("Lock poisoned: {e}"))?;
        *guard = Some("pending".to_string());
    }

    let home = normalized_home(&central_home)?;
    let result = tauri::async_runtime::spawn_blocking(move || run_debate_mode_internal(&home, request))
        .await
        .map_err(|error| format!("Debate worker join error: {error}"));

    // ALWAYS clear lock, even on error
    if let Ok(mut guard) = lock.0.lock() {
        *guard = None;
    }

    // Flatten: Result<Result<T, E>, E> -> Result<T, E>
    result?
}
```

### 驗證

```bash
PATH="/Users/pershing/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" cargo check --manifest-path kofnote-app/src-tauri/Cargo.toml
```

---

## F2：AI Analysis 支援 Gemini 和 Claude provider

### 問題

`run_ai_analysis`（types.rs L1018）目前只支援 `local` 和 `openai`。但 Gemini 和 Claude 的 API key 管理（Keychain）和 provider 呼叫函式都已存在。

### Rust 端修改

在 `types.rs` 的 `run_ai_analysis` 函式中（L1039-1042），把 match 擴充：

```rust
let content = match provider.as_str() {
    "openai" => run_openai_analysis(&model, &prompt, api_key, &records, &logs, include_logs, max_records)?,
    "gemini" => run_gemini_analysis(&model, &prompt, &records, &logs, include_logs, max_records)?,
    "claude" => run_claude_analysis(&model, &prompt, &records, &logs, include_logs, max_records)?,
    "local" => run_local_analysis(&prompt, &records, &logs),
    _ => return Err(format!("Unsupported provider: {provider}")),
};
```

在 `run_openai_analysis` 附近（L1801 之後）新增兩個函式，結構和 `run_openai_analysis` 相同但呼叫不同 provider：

```rust
fn run_gemini_analysis(
    model: &str,
    prompt: &str,
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> Result<String, String> {
    let context = build_context_digest(records, logs, include_logs, max_records);
    let user_prompt = if prompt.trim().is_empty() {
        "Summarize patterns, risks, and a practical 7-day plan.".to_string()
    } else {
        prompt.trim().to_string()
    };
    let merged = format!(
        "You are analyzing a local-first productivity brain system.\nReturn concise markdown sections: Summary, Patterns, Risks, Action Plan (7 days).\n\nUser request:\n{}\n\nContext:\n{}",
        user_prompt, context
    );
    let model = if model.trim().is_empty() { "gemini-2.0-flash" } else { model.trim() };
    crate::providers::gemini::run_gemini_text_completion(model, &merged, 60, 4096)
}

fn run_claude_analysis(
    model: &str,
    prompt: &str,
    records: &[Record],
    logs: &[LogEntry],
    include_logs: bool,
    max_records: usize,
) -> Result<String, String> {
    let context = build_context_digest(records, logs, include_logs, max_records);
    let user_prompt = if prompt.trim().is_empty() {
        "Summarize patterns, risks, and a practical 7-day plan.".to_string()
    } else {
        prompt.trim().to_string()
    };
    let merged = format!(
        "You are analyzing a local-first productivity brain system.\nReturn concise markdown sections: Summary, Patterns, Risks, Action Plan (7 days).\n\nUser request:\n{}\n\nContext:\n{}",
        user_prompt, context
    );
    let model = if model.trim().is_empty() { "claude-3-5-sonnet-latest" } else { model.trim() };
    crate::providers::claude::run_claude_text_completion(model, &merged, 60, 4096)
}
```

### 前端修改

在 `kofnote-app/src/components/AppLegacy.tsx` 的 AI controls `<select>` 中（搜尋 `<option value="openai">openai</option>`），在 openai 之後加：

```tsx
<option value="gemini">gemini</option>
<option value="claude">claude</option>
```

在 `kofnote-app/src/types.ts` L2，`AiProvider` type 已經包含了 `'gemini' | 'claude'`，不需要改。

### 驗證

```bash
PATH="/Users/pershing/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" cargo check --manifest-path kofnote-app/src-tauri/Cargo.toml
cd kofnote-app && npm run lint && npm run build
```

---

## F1：Debate 歷史瀏覽器

### 問題

查看過去的 debate run 需要手動輸入 run ID，沒有歷史列表。

### Rust 端

在 `types.rs` 中：

1. 新增 struct：

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DebateRunSummary {
    run_id: String,
    problem: String,
    provider: String,
    output_type: String,
    degraded: bool,
    created_at: String,
    artifacts_root: String,
}
```

2. 新增 tauri command：

```rust
#[tauri::command]
fn list_debate_runs(central_home: String) -> Result<Vec<DebateRunSummary>, String> {
    let home = normalized_home(&central_home)?;
    list_debate_runs_internal(&home)
}

fn list_debate_runs_internal(central_home: &Path) -> Result<Vec<DebateRunSummary>, String> {
    let debates_dir = central_home.join("records").join("debates");
    if !debates_dir.exists() {
        return Ok(vec![]);
    }

    let mut runs = Vec::new();
    let entries = fs::read_dir(&debates_dir).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let run_id = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let request_path = path.join("request.json");

        if !request_path.exists() {
            continue;
        }

        let request_text = fs::read_to_string(&request_path).unwrap_or_default();
        let request_value: Value = serde_json::from_str(&request_text).unwrap_or_default();

        let problem = request_value.get("problem")
            .and_then(Value::as_str)
            .unwrap_or("")
            .chars().take(120).collect::<String>();
        let provider = request_value.get("participants")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(|p| p.get("modelProvider"))
            .and_then(Value::as_str)
            .unwrap_or("local")
            .to_string();
        let output_type = request_value.get("outputType")
            .and_then(Value::as_str)
            .unwrap_or("decision")
            .to_string();

        // Check if degraded by looking for consensus
        let consensus_path = path.join("consensus.json");
        let degraded = if consensus_path.exists() {
            let c_text = fs::read_to_string(&consensus_path).unwrap_or_default();
            let c_val: Value = serde_json::from_str(&c_text).unwrap_or_default();
            c_val.get("degraded").and_then(Value::as_bool).unwrap_or(false)
        } else {
            false
        };

        let created_at = file_mtime_iso(&request_path);

        runs.push(DebateRunSummary {
            run_id,
            problem,
            provider,
            output_type,
            degraded,
            created_at,
            artifacts_root: path.to_string_lossy().to_string(),
        });
    }

    runs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(runs)
}
```

3. 在 `build_app()` 的 `generate_handler!` 中加入 `list_debate_runs`。

4. 在 `commands/debate.rs` 中加入對應的 pub wrapper：

```rust
#[tauri::command]
pub fn list_debate_runs(central_home: String) -> Result<Vec<crate::types::DebateRunSummary>, String> {
    crate::types::list_debate_runs(central_home)
}
```

並在 `main.rs` 的 `generate_handler!` 中加入 `commands::debate::list_debate_runs`。

### 前端

1. 在 `types.ts` 加：

```typescript
export type DebateRunSummary = {
  runId: string
  problem: string
  provider: string
  outputType: string
  degraded: boolean
  createdAt: string
  artifactsRoot: string
}
```

2. 在 `lib/tauri.ts` 加 invoke wrapper：

```typescript
export async function listDebateRuns(args: { centralHome: string }): Promise<DebateRunSummary[]> {
  return invoke<DebateRunSummary[]>('list_debate_runs', args)
}
```

3. 在 `AppLegacy.tsx` 的 AI tab 區塊中，在 Debate Mode 表單之前加一個「Debate History」區塊：

- 新增 state：`const [debateRuns, setDebateRuns] = useState<DebateRunSummary[]>([])`
- 在 centralHome 載入時呼叫 `listDebateRuns({ centralHome })` 填充列表
- 在 `renderAi()` 的 debate 區塊底部，渲染一個歷史列表：
  - 每行顯示：run ID、問題前 60 字、provider、outputType、degraded 標記、時間
  - 點擊行自動填入 `debateRunId` 並觸發 replay
- 加一個「Refresh History」按鈕

### 驗證

```bash
PATH="/Users/pershing/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" cargo check --manifest-path kofnote-app/src-tauri/Cargo.toml
cd kofnote-app && npm run lint && npm run build
```

---

## F6：搜尋結果 snippet 與 highlight

### 問題

搜尋結果只顯示 title，看不出為什麼匹配。FTS5 有 `snippet()` 函式可以回傳帶標記的匹配片段。

### Rust 端

1. 在 `types.rs` 的 `SearchResult` struct 中加一個欄位：

```rust
struct SearchResult {
    records: Vec<Record>,
    total: usize,
    indexed: bool,
    took_ms: u128,
    snippets: HashMap<String, String>,  // json_path -> snippet HTML
}
```

2. 在 `storage/index.rs` 的 `search_records_in_index` 函式中，修改 SQL 查詢，增加 `snippet()` 呼叫：

把現有的 SELECT 語句改為：
```sql
SELECT json_path, snippet(records_fts, 2, '<mark>', '</mark>', '...', 32) as snippet FROM records_fts WHERE ...
```

回傳值改為 `Result<(Vec<Record>, usize, HashMap<String, String>), String>`，多回傳 snippets map。

3. 在 `types.rs` 的 `search_records` 函式中，接收 snippets 並放入 `SearchResult`。

### 前端

1. 在 `types.ts` 的 `SearchResult` 加：

```typescript
export type SearchResult = {
  records: RecordItem[]
  total: number
  indexed: boolean
  tookMs: number
  snippets: Record<string, string>  // jsonPath -> snippet HTML
}
```

2. 在 `AppLegacy.tsx` 的 Records tab 搜尋結果列表中，如果 `searchMeta` 有 snippets 且當前記錄的 jsonPath 有對應 snippet，顯示在 title 下方：

```tsx
{snippet && (
  <p className="search-snippet" dangerouslySetInnerHTML={{ __html: snippet }} />
)}
```

3. 在 `index.css` 加：

```css
.search-snippet {
  font-size: 0.82rem;
  color: var(--text-soft);
  margin: 2px 0 0;
  line-height: 1.4;
}
.search-snippet mark {
  background: rgba(24, 230, 255, 0.25);
  color: var(--accent-cyan);
  border-radius: 2px;
  padding: 0 2px;
}
```

### 驗證

```bash
PATH="/Users/pershing/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" cargo check --manifest-path kofnote-app/src-tauri/Cargo.toml
cd kofnote-app && npm run lint && npm run build
```

---

## F3：Debate 多 Provider 混合模式

### 問題

5 個角色全用同一個 provider，缺少觀點多樣性。後端 `DebateModeRequest.participants` 已支援 per-role provider/model，但前端強制對所有角色設同一個。

### 前端修改（只改前端）

在 `AppLegacy.tsx` 的 `renderAi()` debate 表單中：

1. 新增 state：

```typescript
const [debateAdvancedMode, setDebateAdvancedMode] = useState(false)
const [debatePerRoleProvider, setDebatePerRoleProvider] = useState<Record<string, string>>({})
const [debatePerRoleModel, setDebatePerRoleModel] = useState<Record<string, string>>({})
```

2. 在 provider `<select>` 下方加一個 checkbox：

```tsx
<label className="checkbox-field">
  <input
    type="checkbox"
    checked={debateAdvancedMode}
    onChange={(e) => setDebateAdvancedMode(e.target.checked)}
  />
  {t('Advanced: per-role provider', '進階：每角色個別 Provider')}
</label>
```

3. 當 `debateAdvancedMode` 為 true 時，顯示 5 行（每個 role 一行），各有自己的 provider `<select>` 和 model `<input>`：

```tsx
{debateAdvancedMode && (
  <div className="form-grid two-col-grid">
    {DEBATE_ROLES.map((role) => (
      <label key={role}>
        {role}
        <select
          value={debatePerRoleProvider[role] || debateProvider}
          onChange={(e) => setDebatePerRoleProvider(prev => ({ ...prev, [role]: e.target.value }))}
        >
          {debateProviderOptions.map((id) => (
            <option key={id} value={id}>{debateProviderLabel(id)}</option>
          ))}
        </select>
      </label>
    ))}
  </div>
)}
```

4. 在 `handleRunDebate` 的 `participants` 建構中：

```typescript
const participants = DEBATE_ROLES.map((role) => ({
  role,
  modelProvider: debateAdvancedMode
    ? (debatePerRoleProvider[role] || debateProvider)
    : debateProvider,
  modelName: debateAdvancedMode
    ? (debatePerRoleModel[role] || providerModel)
    : providerModel,
}))
```

### 驗證

```bash
cd kofnote-app && npm run lint && npm run build
```

---

## F4：Debate 即時進度回報

### 問題

Debate 跑 15 個 turn 可能 2-8 分鐘，期間只顯示「辯論正在背景執行中...」。

### Rust 端

1. 在 `types.rs` 新增 struct：

```rust
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DebateProgress {
    run_id: String,
    round: String,
    role: String,
    turn_index: usize,
    total_turns: usize,
    status: String,  // "started" | "completed" | "failed"
}
```

2. 修改 `run_debate_mode` 使其接收 `app: tauri::AppHandle` 參數：

```rust
pub(crate) async fn run_debate_mode(
    app: tauri::AppHandle,
    lock: tauri::State<'_, DebateLock>,
    central_home: String,
    request: DebateModeRequest,
) -> Result<DebateModeResponse, String> {
```

3. 把 `app` clone 傳入 `spawn_blocking`，再傳入 `run_debate_mode_internal`。

4. 在 `run_debate_mode_internal` 的 `execute_debate_turn` 呼叫前後，用 `app.emit("debate-progress", DebateProgress { ... })` 發送事件。

5. 同步修改 `commands/debate.rs` wrapper 的簽名，加上 `app: tauri::AppHandle`。

### 前端

1. 在 `AppLegacy.tsx` 加一個 state：

```typescript
const [debateProgress, setDebateProgress] = useState<{ round: string; role: string; turnIndex: number; totalTurns: number } | null>(null)
```

2. 加 `useEffect` 監聽 Tauri event：

```typescript
useEffect(() => {
  let unlisten: (() => void) | undefined
  import('@tauri-apps/api/event').then(({ listen }) => {
    listen<DebateProgress>('debate-progress', (event) => {
      setDebateProgress(event.payload)
    }).then((fn) => { unlisten = fn })
  })
  return () => { unlisten?.() }
}, [])
```

3. 在 debate busy 區塊顯示進度：

```tsx
{debateBusy && debateProgress ? (
  <p className="muted">
    {t(`${debateProgress.round} — ${debateProgress.role} (${debateProgress.turnIndex}/${debateProgress.totalTurns})`,
       `${debateProgress.round} — ${debateProgress.role} (${debateProgress.turnIndex}/${debateProgress.totalTurns})`)}
  </p>
) : debateBusy ? (
  <p className="muted">{t('Debate is running in background...', '辯論正在背景執行中...')}</p>
) : null}
```

4. debate 完成時清除 progress：在 `handleRunDebate` 的 `finally` 中加 `setDebateProgress(null)`。

### 驗證

```bash
PATH="/Users/pershing/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" cargo check --manifest-path kofnote-app/src-tauri/Cargo.toml
cd kofnote-app && npm run lint && npm run build
```

---

## F5：Records 批次操作

### 問題

Records 只能一筆一筆操作，刪除多筆或批次同步很繁瑣。

### 前端修改

1. 新增 state：

```typescript
const [selectedRecordPaths, setSelectedRecordPaths] = useState<Set<string>>(new Set())
const [batchMode, setBatchMode] = useState(false)
```

2. 在 Records tab 的工具列加一個「Batch Mode」toggle 按鈕。

3. batch mode 開啟時，record 列表的每一行左側顯示 checkbox：

```tsx
{batchMode && (
  <input
    type="checkbox"
    checked={selectedRecordPaths.has(item.jsonPath ?? '')}
    onChange={(e) => {
      const next = new Set(selectedRecordPaths)
      if (e.target.checked) next.add(item.jsonPath ?? '')
      else next.delete(item.jsonPath ?? '')
      setSelectedRecordPaths(next)
    }}
    onClick={(e) => e.stopPropagation()}
  />
)}
```

4. batch mode 時顯示批次操作工具列：

```tsx
{batchMode && selectedRecordPaths.size > 0 && (
  <div className="toolbar-row">
    <span className="muted">{selectedRecordPaths.size} selected</span>
    <button onClick={handleBatchDelete}>Delete Selected</button>
    <button onClick={handleBatchSyncNotion}>Sync to Notion</button>
    <button onClick={handleBatchExport}>Export Markdown</button>
    <button className="ghost-btn" onClick={() => setSelectedRecordPaths(new Set())}>Deselect All</button>
  </div>
)}
```

5. 實作 `handleBatchDelete`：迴圈呼叫 `deleteRecord`，完成後刷新列表。

6. 實作 `handleBatchSyncNotion`：呼叫現有的 `syncRecordsBidirectional`（已支援批次 `json_paths`）。

7. 「Select All」/「Deselect All」快捷操作。

### 驗證

```bash
cd kofnote-app && npm run lint && npm run build
```

---

## 執行順序總覽

```
F7 (DebateLock fix, ~10 行) → commit
F2 (AI Gemini/Claude, ~80 行) → commit
F1 (Debate History, ~200 行) → commit
F6 (Search snippet, ~100 行) → commit
F3 (Multi-provider debate, ~80 行前端) → commit
F4 (Debate progress, ~150 行) → commit
F5 (Batch operations, ~200 行前端) → commit
```

每步完成後跑對應的驗證指令。
