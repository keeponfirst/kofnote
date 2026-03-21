-- ============================================================
-- KOF Notes — Seed Data (for development/testing)
-- Run after 001_initial_schema.sql
-- ============================================================

-- Note: In development, you'll create a test user via Supabase Auth UI.
-- After signup, the handle_new_user trigger auto-creates a profile.
-- Use this script to verify the schema works.

-- Example: Upgrade a test user to 'pro' tier
-- UPDATE public.profiles SET subscription_tier = 'pro' WHERE email = 'test@example.com';

-- Example: Insert a test record (replace USER_ID with actual UUID)
-- INSERT INTO public.records (user_id, record_type, title, final_body, tags, source_platform, local_id, device_id)
-- VALUES (
--   'YOUR-USER-UUID-HERE',
--   'idea',
--   '測試想法：Supabase 整合',
--   '## Description\n這是一個測試記錄，確認 schema 運作正常。\n\n## Potential\n驗證全文搜尋和 dedup index。',
--   ARRAY['test', 'supabase'],
--   'desktop',
--   'local-test-001',
--   'dev-mac-01'
-- );

-- Verify full-text search works
-- SELECT id, title, ts_rank(search_vector, q) AS rank
-- FROM public.records, to_tsquery('simple', 'supabase') q
-- WHERE search_vector @@ q
-- ORDER BY rank DESC;

-- Verify AI usage counting
-- INSERT INTO public.ai_usage_log (user_id, action, model, tokens)
-- VALUES ('YOUR-USER-UUID-HERE', 'classify', 'gemini-flash', 150);

-- SELECT public.get_ai_usage_today('YOUR-USER-UUID-HERE');
-- Expected: 1
