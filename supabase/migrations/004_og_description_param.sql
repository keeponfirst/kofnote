-- ============================================================
-- KOF Notes — Migration 004: Add p_og_description to upsert_record
-- ============================================================
-- 003_fixes.sql added og_description column to records table and
-- referenced EXCLUDED.og_description in ON CONFLICT, but the
-- upsert_record function signature was missing p_og_description,
-- so the column was always written as NULL. This migration fixes
-- both the function signature and the INSERT/UPDATE body.
-- ============================================================

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
  p_og_description  TEXT DEFAULT NULL,
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
  caller_id UUID;
BEGIN
  -- Security: caller must be the record owner OR service_role (uid = NULL)
  caller_id := auth.uid();
  IF caller_id IS NOT NULL AND caller_id != p_user_id THEN
    RAISE EXCEPTION 'unauthorized: user_id mismatch (caller: %, requested: %)',
      caller_id, p_user_id;
  END IF;

  INSERT INTO public.records (
    id, user_id, local_id, device_id,
    record_type, title, source_text, final_body,
    tags, source_url, source_platform,
    og_title, og_image, og_description, key_insight, date,
    is_deleted, updated_at
  ) VALUES (
    p_id, p_user_id, p_local_id, p_device_id,
    p_record_type, p_title, p_source_text, p_final_body,
    p_tags, p_source_url, p_source_platform,
    p_og_title, p_og_image, p_og_description, p_key_insight, p_date,
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
    og_description  = EXCLUDED.og_description,
    key_insight     = EXCLUDED.key_insight,
    date            = EXCLUDED.date,
    is_deleted      = CASE
                        WHEN EXCLUDED.is_deleted = FALSE AND records.is_deleted = TRUE
                          THEN FALSE
                        ELSE EXCLUDED.is_deleted
                      END,
    device_id       = EXCLUDED.device_id
  WHERE EXCLUDED.updated_at > records.updated_at
     OR (EXCLUDED.is_deleted = FALSE AND records.is_deleted = TRUE)
  RETURNING * INTO result;

  RETURN result;
END;
$$;

COMMENT ON FUNCTION public.upsert_record IS
  'LWW upsert for offline sync. Edit beats delete. Includes og_description. Security: validates caller uid.';
