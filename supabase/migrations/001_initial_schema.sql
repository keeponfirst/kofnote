-- ============================================================
-- KOF Notes — Supabase Initial Schema
-- Run in Supabase SQL Editor (or via supabase db push)
-- ============================================================

-- ────────────────────────────────────────────────────────────
-- 1. EXTENSIONS
-- ────────────────────────────────────────────────────────────
CREATE EXTENSION IF NOT EXISTS "pgcrypto";     -- gen_random_uuid()
-- pgsodium is pre-installed on Supabase for Vault

-- ────────────────────────────────────────────────────────────
-- 2. PROFILES（用戶 + 訂閱狀態）
-- ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.profiles (
  id                    UUID REFERENCES auth.users ON DELETE CASCADE PRIMARY KEY,
  email                 TEXT,
  subscription_tier     TEXT NOT NULL DEFAULT 'free'
                        CHECK (subscription_tier IN ('free', 'pro', 'premium')),
  notion_access_token   TEXT,          -- Vault 加密，Premium 專用
  notion_workspace_id   TEXT,
  created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE public.profiles IS '用戶設定檔與訂閱狀態';
COMMENT ON COLUMN public.profiles.notion_access_token IS 'Encrypted via Vault. Only for Premium tier.';

-- Auto-create profile on signup
CREATE OR REPLACE FUNCTION public.handle_new_user()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER SET search_path = public
AS $$
BEGIN
  INSERT INTO public.profiles (id, email)
  VALUES (NEW.id, NEW.email);
  RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS on_auth_user_created ON auth.users;
CREATE TRIGGER on_auth_user_created
  AFTER INSERT ON auth.users
  FOR EACH ROW EXECUTE FUNCTION public.handle_new_user();

-- RLS
ALTER TABLE public.profiles ENABLE ROW LEVEL SECURITY;

CREATE POLICY "Users can view own profile"
  ON public.profiles FOR SELECT
  USING (auth.uid() = id);

CREATE POLICY "Users can update own profile"
  ON public.profiles FOR UPDATE
  USING (auth.uid() = id)
  WITH CHECK (auth.uid() = id);

-- ────────────────────────────────────────────────────────────
-- 3. RECORDS（核心資料）
-- ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.records (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id         UUID NOT NULL REFERENCES auth.users ON DELETE CASCADE,
  record_type     TEXT NOT NULL
                  CHECK (record_type IN ('decision', 'idea', 'backlog', 'worklog', 'note')),
  title           TEXT NOT NULL,
  source_text     TEXT,
  final_body      TEXT,
  tags            TEXT[] NOT NULL DEFAULT '{}',
  source_url      TEXT,
  source_platform TEXT
                  CHECK (source_platform IN ('mobile_share', 'claude_code', 'desktop', 'browser')),
  og_title        TEXT,
  og_image        TEXT,
  key_insight     TEXT,
  date            DATE,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

  -- 離線同步 & 衝突解決
  local_id        TEXT,                      -- 本機 UUID，dedup 用
  device_id       TEXT,                      -- 來源裝置
  version         INTEGER NOT NULL DEFAULT 1, -- 樂觀鎖
  is_deleted      BOOLEAN NOT NULL DEFAULT FALSE,  -- soft delete

  -- 全文搜尋（自動維護）
  search_vector   TSVECTOR GENERATED ALWAYS AS (
    to_tsvector('simple',
      coalesce(title, '') || ' ' ||
      coalesce(final_body, '') || ' ' ||
      coalesce(source_text, ''))
  ) STORED
);

COMMENT ON TABLE public.records IS '核心筆記與決策記錄';
COMMENT ON COLUMN public.records.local_id IS 'Client-generated UUID for offline dedup';
COMMENT ON COLUMN public.records.version IS 'Optimistic lock counter, increments on each update';
COMMENT ON COLUMN public.records.is_deleted IS 'Soft delete flag for conflict resolution';

-- Indexes
CREATE INDEX IF NOT EXISTS idx_records_user_created
  ON public.records (user_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_records_user_type
  ON public.records (user_id, record_type);

CREATE INDEX IF NOT EXISTS idx_records_search
  ON public.records USING GIN (search_vector);

CREATE INDEX IF NOT EXISTS idx_records_tags
  ON public.records USING GIN (tags);

CREATE UNIQUE INDEX IF NOT EXISTS idx_records_dedup
  ON public.records (user_id, local_id)
  WHERE local_id IS NOT NULL;

-- Auto-update updated_at
CREATE OR REPLACE FUNCTION public.update_updated_at()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
  NEW.updated_at = NOW();
  NEW.version = OLD.version + 1;
  RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS set_updated_at ON public.records;
CREATE TRIGGER set_updated_at
  BEFORE UPDATE ON public.records
  FOR EACH ROW EXECUTE FUNCTION public.update_updated_at();

-- RLS
ALTER TABLE public.records ENABLE ROW LEVEL SECURITY;

CREATE POLICY "Users can CRUD own records"
  ON public.records FOR ALL
  USING (auth.uid() = user_id)
  WITH CHECK (auth.uid() = user_id);

-- ────────────────────────────────────────────────────────────
-- 4. AI_USAGE_LOG（計費追蹤）
-- ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS public.ai_usage_log (
  id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id    UUID NOT NULL REFERENCES auth.users ON DELETE CASCADE,
  action     TEXT NOT NULL
             CHECK (action IN ('classify', 'summarize', 'analyze')),
  model      TEXT NOT NULL,
  tokens     INTEGER DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

COMMENT ON TABLE public.ai_usage_log IS 'AI 呼叫計費追蹤';

CREATE INDEX IF NOT EXISTS idx_ai_usage_user_date
  ON public.ai_usage_log (user_id, created_at);

-- RLS: 用戶只能看自己的 log
ALTER TABLE public.ai_usage_log ENABLE ROW LEVEL SECURITY;

CREATE POLICY "Users can view own usage"
  ON public.ai_usage_log FOR SELECT
  USING (auth.uid() = user_id);

-- Service role（Gateway）可插入任何人的 log
-- (service_role 已經繞過 RLS，無需額外 policy)

-- ────────────────────────────────────────────────────────────
-- 5. HELPER FUNCTIONS
-- ────────────────────────────────────────────────────────────

-- 查詢今日 AI 用量（Gateway 呼叫用）
CREATE OR REPLACE FUNCTION public.get_ai_usage_today(p_user_id UUID)
RETURNS INTEGER
LANGUAGE sql
STABLE
SECURITY DEFINER
AS $$
  SELECT COALESCE(COUNT(*), 0)::INTEGER
  FROM public.ai_usage_log
  WHERE user_id = p_user_id
    AND created_at >= CURRENT_DATE;
$$;

-- 取得用量上限（依 tier）
CREATE OR REPLACE FUNCTION public.get_ai_limit(p_tier TEXT)
RETURNS INTEGER
LANGUAGE sql
IMMUTABLE
AS $$
  SELECT CASE p_tier
    WHEN 'free'    THEN 10
    WHEN 'pro'     THEN 500
    WHEN 'premium' THEN 2000
    ELSE 0
  END;
$$;

-- 衝突解決 upsert（客戶端 sync 用）
CREATE OR REPLACE FUNCTION public.upsert_record(
  p_id              UUID,
  p_user_id         UUID,
  p_local_id        TEXT,
  p_device_id       TEXT,
  p_record_type     TEXT,
  p_title           TEXT,
  p_source_text     TEXT DEFAULT NULL,
  p_final_body      TEXT DEFAULT NULL,
  p_tags            TEXT[] DEFAULT '{}',
  p_source_url      TEXT DEFAULT NULL,
  p_source_platform TEXT DEFAULT NULL,
  p_og_title        TEXT DEFAULT NULL,
  p_og_image        TEXT DEFAULT NULL,
  p_key_insight     TEXT DEFAULT NULL,
  p_date            DATE DEFAULT NULL,
  p_is_deleted      BOOLEAN DEFAULT FALSE,
  p_updated_at      TIMESTAMPTZ DEFAULT NOW()
)
RETURNS public.records
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
  result public.records;
BEGIN
  INSERT INTO public.records (
    id, user_id, local_id, device_id,
    record_type, title, source_text, final_body,
    tags, source_url, source_platform,
    og_title, og_image, key_insight, date,
    is_deleted, updated_at
  ) VALUES (
    p_id, p_user_id, p_local_id, p_device_id,
    p_record_type, p_title, p_source_text, p_final_body,
    p_tags, p_source_url, p_source_platform,
    p_og_title, p_og_image, p_key_insight, p_date,
    p_is_deleted, p_updated_at
  )
  ON CONFLICT (user_id, local_id) WHERE local_id IS NOT NULL
  DO UPDATE SET
    record_type     = EXCLUDED.record_type,
    title           = EXCLUDED.title,
    source_text     = EXCLUDED.source_text,
    final_body      = EXCLUDED.final_body,
    tags            = EXCLUDED.tags,
    source_url      = EXCLUDED.source_url,
    source_platform = EXCLUDED.source_platform,
    og_title        = EXCLUDED.og_title,
    og_image        = EXCLUDED.og_image,
    key_insight     = EXCLUDED.key_insight,
    date            = EXCLUDED.date,
    is_deleted      = CASE
                        -- 編輯優先於刪除
                        WHEN EXCLUDED.is_deleted = FALSE AND records.is_deleted = TRUE
                          THEN FALSE
                        ELSE EXCLUDED.is_deleted
                      END,
    -- version 和 updated_at 由 trigger 自動處理
    device_id       = EXCLUDED.device_id
  WHERE EXCLUDED.updated_at > records.updated_at
     OR (EXCLUDED.is_deleted = FALSE AND records.is_deleted = TRUE)
  RETURNING * INTO result;

  RETURN result;
END;
$$;

COMMENT ON FUNCTION public.upsert_record IS
  'LWW upsert for offline sync. Edit beats delete. Version auto-increments via trigger.';

-- ────────────────────────────────────────────────────────────
-- 6. JWT CUSTOM CLAIMS HOOK（tier 寫入 JWT）
-- ────────────────────────────────────────────────────────────
-- 在 Supabase Dashboard → Auth → Hooks 啟用此 function
CREATE OR REPLACE FUNCTION public.custom_access_token_hook(event JSONB)
RETURNS JSONB
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
  claims    JSONB;
  user_tier TEXT;
BEGIN
  SELECT subscription_tier INTO user_tier
  FROM public.profiles
  WHERE id = (event ->> 'user_id')::UUID;

  claims := event -> 'claims';

  IF jsonb_typeof(claims -> 'app_metadata') IS NULL THEN
    claims := jsonb_set(claims, '{app_metadata}', '{}');
  END IF;

  claims := jsonb_set(
    claims,
    '{app_metadata, tier}',
    to_jsonb(COALESCE(user_tier, 'free'))
  );

  event := jsonb_set(event, '{claims}', claims);
  RETURN event;
END;
$$;

-- Grant necessary permissions for the hook
GRANT USAGE ON SCHEMA public TO supabase_auth_admin;
GRANT EXECUTE ON FUNCTION public.custom_access_token_hook TO supabase_auth_admin;
GRANT SELECT ON TABLE public.profiles TO supabase_auth_admin;
REVOKE EXECUTE ON FUNCTION public.custom_access_token_hook FROM authenticated, anon, public;

-- ────────────────────────────────────────────────────────────
-- 7. REALTIME（啟用即時同步）
-- ────────────────────────────────────────────────────────────
-- 在 Supabase Dashboard → Database → Replication 啟用 records 表
-- 或用 SQL:
ALTER PUBLICATION supabase_realtime ADD TABLE public.records;

-- ────────────────────────────────────────────────────────────
-- Done! Next steps:
-- 1. Go to Supabase Dashboard → Auth → Hooks
--    Enable "Customize Access Token" hook → select custom_access_token_hook
-- 2. Go to Auth → Providers → enable Google & Apple OAuth
-- 3. Go to Database → Replication → verify records table is enabled
-- ────────────────────────────────────────────────────────────
