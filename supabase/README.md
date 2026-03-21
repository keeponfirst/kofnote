# KOF Notes — Supabase Setup Guide

## 前置條件

1. 在 [supabase.com](https://supabase.com) 建立新專案
2. 記下 Project URL 和 API Keys（anon key + service_role key）

## 步驟

### 1. 執行 Schema

在 Supabase Dashboard → SQL Editor，依序執行：

1. `001_initial_schema.sql` — 建立所有表、索引、RLS、functions
2. `002_seed_data.sql` — 取消註解測試用的 INSERT（開發用）

### 2. 啟用 Auth Hook

Dashboard → Auth → Hooks → **Customize Access Token (JWT)**
- 開啟此 hook
- 選擇 `public.custom_access_token_hook`
- 這會讓每個 JWT 帶上 `app_metadata.tier`

### 3. 設定 OAuth Providers

Dashboard → Auth → Providers：

| Provider | 用途 |
|----------|------|
| Email | 基本登入 |
| Google | Android / Web 登入 |
| Apple | iOS App Store 必要 |

### 4. 驗證 Realtime

Dashboard → Database → Replication：
- 確認 `records` 表已加入 `supabase_realtime` publication
- （Schema SQL 已包含 `ALTER PUBLICATION` 指令）

### 5. 環境變數

各端需要的變數：

```env
# 共用
SUPABASE_URL=https://your-project.supabase.co
SUPABASE_ANON_KEY=eyJ...

# keeponfirst-local-brain（server-side only）
SUPABASE_SERVICE_ROLE_KEY=eyJ...
KOF_USER_ID=your-user-uuid

# Cloudflare Workers
SUPABASE_URL=https://your-project.supabase.co
SUPABASE_SERVICE_ROLE_KEY=eyJ...
```

> ⚠️ `SUPABASE_SERVICE_ROLE_KEY` 繞過 RLS，**絕對不要**放在客戶端！

## 檔案結構

```
supabase/
├── migrations/
│   ├── 001_initial_schema.sql   ← Schema + RLS + Functions
│   └── 002_seed_data.sql        ← 開發測試用
└── README.md                    ← 本文件
```

## Schema 概覽

| 表 | 用途 |
|----|------|
| `profiles` | 用戶設定 + 訂閱 tier |
| `records` | 核心筆記/決策記錄 |
| `ai_usage_log` | AI 呼叫計費追蹤 |

## 關鍵 Functions

| Function | 用途 |
|----------|------|
| `handle_new_user()` | Auth trigger：新用戶自動建 profile |
| `update_updated_at()` | 更新 trigger：自動 bump version + timestamp |
| `get_ai_usage_today(uuid)` | 查今日 AI 用量 |
| `get_ai_limit(tier)` | 查 tier 上限 |
| `upsert_record(...)` | LWW 衝突解決 upsert |
| `custom_access_token_hook(jsonb)` | JWT 注入 tier |
