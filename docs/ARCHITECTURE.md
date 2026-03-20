# KOF Notes — 系統架構文件

> 版本：v2.1
> 最後更新：2026-03-20
> 狀態：Phase 0–3 ✅ Phase 4 ✅（核心命令完成）Phase 5 ✅（Gateway + Mobile UI）Phase 6 ✅（遷移腳本）

---

## 一、產品定位

KOF Notes 是一套跨裝置的個人知識捕捉系統，由三個端點組成：

| 端點 | 技術 | 用途 |
|------|------|------|
| **kofnote-mobile** | Flutter (iOS/Android) | 手機捕捉、Share Intent、瀏覽 |
| **kofnote** | Tauri 2 (Rust + React) | 桌機瀏覽、搜尋、Debate Mode |
| **keeponfirst-local-brain** | Python + Claude Code Skills | IDE 內捕捉（`/kof-cap` 等）|

三端共用同一份雲端資料，透過 Supabase 即時同步。

---

## 二、整體架構

```
┌─────────────────────────────────────────────────────────┐
│                      使用者裝置                          │
│                                                         │
│  kofnote (Tauri)    kofnote-mobile (Flutter)            │
│  Claude Code Skills                                     │
│       │                    │                            │
│  ┌────┴────┐          ┌────┴────┐                       │
│  │Local DB │          │ SQLite  │  ← Offline-First      │
│  │ (JSON)  │          │(sqflite)│                       │
│  └────┬────┘          └────┬────┘                       │
│       └────────┬───────────┘                            │
│           Sync Queue                                    │
└────────────────│────────────────────────────────────────┘
                 │ HTTPS
       ┌─────────┴──────────┐
       │                    │
       ▼                    ▼
Cloudflare Workers    Supabase
(API Gateway)         (Source of Truth)
/api/ai/*             ├── PostgreSQL (records)
- JWT 驗證            ├── Auth (users + OAuth)
- tier cache (KV)     ├── Realtime (WebSocket)
- rate limit          ├── Vault (token 加密)
- model routing       └── Edge Functions
       │
       ▼
 AI Models（依 tier 路由）
 ├── Free:    Gemini 2.0 Flash
 ├── Pro:     Claude Haiku 4.5
 └── Premium: Claude Sonnet 4.6

[Premium 選配]
Notion OAuth → 鏡像到用戶自己的 Workspace
```

---

## 三、訂閱分層

| 方案 | AI 次數/天 | 模型 | 雲端同步 | Notion 整合 |
|------|-----------|------|---------|------------|
| **Free** | 10 次 | Gemini 2.0 Flash | ✅ | ❌ |
| **Pro** | 500 次（正常用不完）| Claude Haiku 4.5 | ✅ | ❌ |
| **Premium** | 2,000 次 | Claude Sonnet 4.6 | ✅ | ✅ OAuth 雙向 |

> Free 用戶不需要輸入任何 API Key，由 Gateway 統一提供模型服務。

---

## 四、Supabase Schema

### `profiles`（用戶 + 訂閱）

```sql
CREATE TABLE profiles (
  id                    UUID REFERENCES auth.users PRIMARY KEY,
  email                 TEXT,
  subscription_tier     TEXT DEFAULT 'free'
                        CHECK (subscription_tier IN ('free','pro','premium')),
  notion_access_token   TEXT,   -- Vault 加密，Premium 專用
  notion_workspace_id   TEXT,
  created_at            TIMESTAMPTZ DEFAULT NOW()
);
```

**觸發器**：新用戶註冊時自動建立 profile（`handle_new_user` trigger）。

**JWT Hook**：`custom_access_token_hook` 將 `subscription_tier` 注入 JWT `app_metadata.tier`，Gateway 直接從 token 讀取，零 DB 查詢。

### `records`（核心資料）

```sql
CREATE TABLE records (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id         UUID REFERENCES auth.users NOT NULL,
  record_type     TEXT CHECK (record_type IN
                    ('decision','idea','backlog','worklog','note')),
  title           TEXT NOT NULL,
  source_text     TEXT,
  final_body      TEXT,
  tags            TEXT[] DEFAULT '{}',
  source_url      TEXT,
  source_platform TEXT CHECK (source_platform IN
                    ('mobile_share','claude_code','desktop','browser')),
  og_title        TEXT,
  og_image        TEXT,
  key_insight     TEXT,
  date            DATE,
  created_at      TIMESTAMPTZ DEFAULT NOW(),
  updated_at      TIMESTAMPTZ DEFAULT NOW(),
  local_id        TEXT,                        -- 本機 UUID，dedup 用
  device_id       TEXT,                        -- 來源裝置
  version         INTEGER DEFAULT 1,           -- 樂觀鎖
  is_deleted      BOOLEAN DEFAULT FALSE,       -- soft delete
  search_vector   TSVECTOR GENERATED ALWAYS AS (
    to_tsvector('simple',
      coalesce(title,'') || ' ' ||
      coalesce(final_body,'') || ' ' ||
      coalesce(source_text,''))
  ) STORED
);
```

**索引**：
- `(user_id, created_at DESC)` — 時間軸查詢
- `(user_id, record_type)` — 分類篩選
- `GIN (search_vector)` — 全文搜尋
- `GIN (tags)` — 標籤篩選
- `UNIQUE (user_id, local_id) WHERE local_id IS NOT NULL` — 防重複寫入

**RLS**：每人只能讀寫自己的 records。

### `ai_usage_log`（計費追蹤）

```sql
CREATE TABLE ai_usage_log (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id    UUID REFERENCES auth.users NOT NULL,
  action     TEXT CHECK (action IN ('classify','summarize','analyze')),
  model      TEXT,
  tokens     INTEGER DEFAULT 0,
  created_at TIMESTAMPTZ DEFAULT NOW()
);
```

用量計算：每次 Gateway 請求前 `COUNT(*) WHERE created_at >= CURRENT_DATE`（避免 race condition）。

### Helper Functions

| Function | 用途 |
|----------|------|
| `handle_new_user()` | 新用戶自動建 profile |
| `update_updated_at()` | 自動 bump version + timestamp |
| `get_ai_usage_today(uuid)` | 查今日用量 |
| `get_ai_limit(tier)` | 查 tier 上限（free:10, pro:500, premium:2000）|
| `upsert_record(...)` | LWW 衝突解決 upsert |
| `custom_access_token_hook(jsonb)` | JWT 注入 tier |

---

## 五、離線同步機制

### 本地優先策略

每個客戶端維護：
- **本地 DB**（SQLite on mobile / JSON files on desktop）— 立即可用，不依賴網路
- **sync_queue**（本地）— 記錄待同步的操作

### 同步流程

```
App 啟動 / 網路恢復
  │
  ├─ 1. Push：sync_queue 中 pending → batch upsert 到 Supabase
  │       成功 → 標記 synced
  │       失敗 → retry_count++，超過 3 次標記 failed
  │
  ├─ 2. Pull：updated_at > last_sync_at 的 records → 寫入本地
  │
  └─ 3. Subscribe：Supabase Realtime WebSocket listener
          → 即時接收其他裝置變更
          → 斷線 → exponential backoff 重連
```

### 衝突解決（Last-Write-Wins）

| 場景 | 策略 |
|------|------|
| 兩端同時編輯 | `updated_at` 較新的版本勝出 |
| A 刪除、B 編輯 | 編輯優先（`is_deleted = FALSE` 覆蓋 `TRUE`）|
| 兩端都刪除 | 保持 `is_deleted = true` |

---

## 六、API Gateway（Cloudflare Workers）

**位置**：`kofnote/gateway/`
**部署**：`npm run deploy`（需先設定 KV namespace + secrets）

### 端點

| 方法 | 路徑 | 說明 | 輸入限制 |
|------|------|------|---------|
| GET | `/api/health` | 健康檢查 | — |
| POST | `/api/ai/classify` | 分類文字 → record type + 標題 | 10,000 字 |
| POST | `/api/ai/summarize` | 長文摘要 + key insight | 20,000 字 |
| POST | `/api/ai/analyze` | 決策/想法優缺點分析 | 10,000 字 |

### 請求流程

```
Request（含 Supabase JWT）
  │
  ├─ 1. JWT 驗證（jose，使用 SUPABASE_JWT_SECRET）
  │     從 app_metadata.tier 讀取 tier（零 DB 查詢）
  │
  ├─ 2. Rate limit 檢查
  │     KV cache（TTL 60s）→ cache miss → Supabase RPC
  │
  ├─ 3. 超限 → 429；未超限 → 繼續
  │
  ├─ 4. 路由到 AI 模型
  │     Free    → Gemini 2.0 Flash
  │     Pro     → Claude Haiku 4.5
  │     Premium → Claude Sonnet 4.6
  │
  ├─ 5. async 寫入 ai_usage_log（不阻擋回傳）
  │
  ├─ 6. 刪除 KV cache（讓下次讀到最新計數）
  │
  └─ 7. 回傳結果
```

### 環境變數 / Secrets

| 變數 | 類型 | 說明 |
|------|------|------|
| `SUPABASE_URL` | var | Supabase 專案 URL |
| `CORS_ORIGIN` | var | 允許的 origin（生產環境設具體域名）|
| `SUPABASE_JWT_SECRET` | secret | JWT 驗證金鑰 |
| `SUPABASE_SERVICE_ROLE_KEY` | secret | 寫入 ai_usage_log 用 |
| `GEMINI_API_KEY` | secret | Free tier AI |
| `ANTHROPIC_API_KEY` | secret | Pro/Premium AI |

### 部署步驟

```bash
cd kofnote/gateway
npm install

# 建立 KV namespace
npx wrangler kv:namespace create USAGE_CACHE
npx wrangler kv:namespace create USAGE_CACHE --preview
# 把兩個 ID 填入 wrangler.toml

# 設定 secrets
npx wrangler secret put SUPABASE_JWT_SECRET
npx wrangler secret put SUPABASE_SERVICE_ROLE_KEY
npx wrangler secret put GEMINI_API_KEY
npx wrangler secret put ANTHROPIC_API_KEY

# 部署
npm run deploy
```

---

## 七、Auth 策略

```
Supabase Auth
  ├── Email + Password
  ├── Google OAuth    （Android / Web）
  └── Apple OAuth     （iOS App Store 必要）

Token 存放：
  ├── Flutter  → FlutterSecureStorage
  ├── Tauri    → 系統 keychain
  └── Python   → .env（service_role key，server-side only）
```

---

## 八、各端改動清單

### kofnote-mobile（Flutter）

| 檔案 | 改動 |
|------|------|
| `pubspec.yaml` | 加 `supabase_flutter` |
| `ai_service.dart` | 移除直接 API key → 改打 Gateway，帶 JWT |
| `notion_service.dart` | 降級為 Premium 選配 |
| `cloud_sync_service.dart` | 重寫：Supabase Realtime + sync queue（移除 5 分鐘輪詢）|
| `storage_service.dart` | 加 sync_queue 管理 |
| **NEW** `auth_service.dart` | Supabase 登入/登出/Apple OAuth |
| **NEW** `supabase_service.dart` | CRUD + realtime subscription |
| **NEW** `sync_queue_service.dart` | 離線 queue 管理 |

**Share Intent 新流程**：
```
分享文字/URL → 存入本地 DB（立即可見）
  → sync_queue 加 classify 任務
  → 有網路 → Gateway /api/ai/classify（帶 JWT）
  → AI 分類回傳 → 更新本地 + upsert Supabase
  → Realtime 通知其他裝置
```

### keeponfirst-local-brain（Python）

| 檔案 | 改動 |
|------|------|
| `write_record.py` | 加 Supabase 寫入（supabase-py），Notion 變選配 |
| `.env` | 加 `SUPABASE_URL`、`SUPABASE_SERVICE_ROLE_KEY`、`KOF_USER_ID` |
| `requirements.txt` | 加 `supabase` |

> Service role key 繞過 RLS，`write_record.py` 必須讀 `KOF_USER_ID` 並寫入 `user_id` 欄位。

### kofnote（Tauri Desktop）

| 模組 | 改動 |
|------|------|
| `notion_service` | 降級為 Premium 功能 |
| **NEW** `supabase_service.rs` | reqwest 呼叫 Supabase REST API |
| **NEW** `realtime.rs` | tokio-tungstenite 接 Supabase WebSocket |
| **NEW** `sync_queue.rs` | 離線 sync queue |
| AI 呼叫 | 改打 Gateway（移除直接 API key 輸入）|

> Supabase 無官方 Rust Realtime SDK。先用 REST polling（30s），後補 WebSocket。

---

## 九、Notion Premium 整合

```
Settings → Connect Notion
  → Notion OAuth 授權
  → access_token → Supabase Vault 加密存放（pgsodium）
  → Edge Function 定時 mirror records → 用戶自己的 Notion Workspace
  → 或手動觸發「匯出到 Notion」
```

Notion 是**用戶自己的 Workspace 的鏡像**，不是產品基礎設施。

---

## 十、遷移路徑

| Phase | 天數 | 內容 | 狀態 |
|-------|------|------|------|
| **0** | 1 天 | Supabase schema + RLS + Vault + JWT hook | ✅ 完成 |
| **1** | 2 天 | Cloudflare Workers Gateway | ✅ 完成 |
| **2** | 5 天 | kofnote-mobile：auth + sync + offline queue | ✅ 核心服務完成（待 UI 串接）|
| **3** | 2 天 | keeponfirst-local-brain：Supabase 寫入 | ✅ 完成 |
| **4** | 4 天 | kofnote 桌機：sync + Realtime（Rust）| ✅ 核心命令完成（REST polling，WebSocket 後補）|
| **5** | 3 天 | Notion OAuth Premium + Vault 加密 | ✅ Gateway OAuth handler + Mobile UI 完成 |
| **6** | 1 天 | 舊 Notion 資料一次性匯入 Supabase | ✅ `migrate_to_supabase.py` 完成 |

---

## 十一、Gateway 安全防護層

### 4 層防護架構

```
Layer 0  Cloudflare DDoS + Bot Fight Mode（網路層）
Layer 1  IP Rate Limit（300 req/day/IP，KV cache）  ← rate-limit.js
Layer 2  JWT 驗證（Supabase 公鑰驗簽）             ← auth.js
Layer 3  Per-user AI 每日用量（tier 限制）          ← rate-limit.js
Layer 4  Email 驗證（Supabase 新用戶必須驗信）
```

### Layer 1 實作細節

```js
// middleware/rate-limit.js
export async function checkIpRateLimit(request, env) {
  const ip = request.headers.get('CF-Connecting-IP') || '...';
  const key = `ip:${ip}:${todayKey()}`;   // 每天重置
  const count = parseInt(await env.USAGE_CACHE.get(key) || '0', 10);
  if (count >= 300) return { error: '請求次數超過上限' };
  await env.USAGE_CACHE.put(key, String(count + 1), { expirationTtl: 86400 });
  return { ok: true };
}
```

呼叫順序：`checkIpRateLimit` → `verifyAuth` → `checkRateLimit`（per-user）

### Cloudflare Dashboard 免費設定（手動配置）

部署後需在 Cloudflare Dashboard 手動開啟：

**1. Bot Fight Mode**
- 路徑：Dashboard → Security → Bots
- 操作：開啟 **Bot Fight Mode**（免費，阻擋已知爬蟲 + 掃描器）

**2. Rate Limiting Rule（WAF）**
- 路徑：Dashboard → Security → WAF → Rate Limiting Rules → Create Rule
- 設定：
  ```
  名稱：API endpoint protection
  條件：http.request.uri.path matches "^/api/"
  速率：100 requests / 10 minutes / IP
  動作：Block（時間：1 小時）
  ```
- 免費方案可設 1 條 rule

**3. Under Attack Mode（緊急用）**
- 路徑：Dashboard → Overview → Quick Actions
- 只在遭受攻擊時啟用（會影響正常用戶）

---

## 十二、費用估算

| 服務 | 免費 tier | 付費起點 | 備註 |
|------|----------|---------|------|
| Supabase | 500MB DB，200 concurrent connections | $25/月 | 100 用戶 × 3 端 ≈ 300 connections，可能需升級 |
| Cloudflare Workers | 10 萬 req/天 | $5/月 | 初期免費 tier 足夠 |
| CF Workers KV | 10 萬 read/天 | 含在 $5/月 | tier cache |
| Gemini 2.0 Flash | 免費 tier 充足 | $0.075/M tokens | Free tier 主力 |
| Claude Haiku 4.5 | — | $0.8/M tokens | Pro tier |
| Claude Sonnet 4.6 | — | $3/M tokens | Premium tier |

**早期 100 Free 用戶估計**：AI 成本 ~$2–5/月，基礎設施可全在免費 tier 內。

---

## 十二、技術風險

| 風險 | 影響 | 緩解方案 |
|------|------|---------|
| Supabase Realtime 斷線 | 資料不同步 | Exponential backoff + sync queue 補救 |
| Tauri 無官方 Rust Realtime SDK | 開發時間增加 | 先 REST polling（30s），後補 WebSocket |
| Service role key 洩漏 | 全部資料暴露 | .env 不進 git，最小權限，監控異常 |
| Notion OAuth token 過期 | Premium 功能中斷 | Edge Function 自動 refresh |
| Supabase concurrent connections 超限 | 連線失敗 | 設定監控告警，提前升級 Pro |
| `upsert_record` SECURITY DEFINER 被誤用 | 跨用戶寫入 | 函數內需加 `auth.uid()` 驗證（待修）|

---

## 十三、已知待修問題

1. ~~`upsert_record` 安全漏洞~~：✅ 已修（`003_fixes.sql`，加入 `auth.uid()` 驗證）
2. ~~缺少 `og_description` 欄位~~：✅ 已修（`003_fixes.sql`，`ALTER TABLE` 加欄位）
3. **`blueprint_v2.md` 殘留 `service_insert` policy**：文件需清理（低優先）
4. ~~kofnote-mobile Phase 2 UI 待串接~~：✅ 已完成（Login/Signup/ForgotPassword screens + authStateProvider + GoRouter redirect）
5. ~~Phase 4 待實作~~：✅ 核心命令完成（`supabase_sign_in/out/auth_status/full_sync`），Realtime WebSocket 後補
6. **kofnote-mobile Notion 深連結處理**：`io.supabase.kofnote://notion-callback` 需在 iOS/Android 設定 URL scheme（AppDelegate / AndroidManifest）
7. **Phase 4 前端 UI**：Tauri React 需加 Supabase 登入 UI（Settings 頁面）
8. **notion_connect_screen.dart 存取 `_client`**：目前直接存取 `SupabaseService._client`，應改為 public getter

---

## 十四、相關檔案索引

```
kofnote/
├── supabase/
│   ├── migrations/
│   │   ├── 001_initial_schema.sql   ← 完整 schema
│   │   └── 002_seed_data.sql        ← 開發測試
│   └── README.md                    ← 設定步驟
├── gateway/
│   ├── src/
│   │   ├── index.js                 ← 路由入口
│   │   ├── utils.js                 ← 共用工具
│   │   ├── ai/router.js             ← AI model 路由
│   │   ├── handlers/                ← classify / summarize / analyze
│   │   └── middleware/              ← auth / rate-limit / cors
│   └── wrangler.toml                ← 部署設定
└── docs/
    └── ARCHITECTURE.md              ← 本文件
```
