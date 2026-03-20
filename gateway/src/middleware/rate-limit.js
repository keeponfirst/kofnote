// ============================================================
// Rate Limiting Middleware
// ============================================================
// Checks AI usage against tier limits using KV cache + Supabase
// ============================================================

import { todayKey } from '../utils.js';

// ── IP-level rate limit (before JWT) ─────────────────────────────────────────

const IP_DAILY_LIMIT = 300; // per IP per day across all endpoints

/**
 * Lightweight guard against unauthenticated hammering.
 * Uses CF-Connecting-IP + KV, resets at midnight UTC.
 */
export async function checkIpRateLimit(request, env) {
  const ip =
    request.headers.get('CF-Connecting-IP') ||
    request.headers.get('X-Forwarded-For')?.split(',')[0].trim() ||
    'unknown';
  const key = `ip:${ip}:${todayKey()}`;
  const raw = await env.USAGE_CACHE.get(key);
  const count = raw ? parseInt(raw, 10) : 0;

  if (count >= IP_DAILY_LIMIT) {
    return { error: '請求次數超過上限，請明天再試' };
  }

  await env.USAGE_CACHE.put(key, String(count + 1), { expirationTtl: 86400 });
  return { ok: true };
}

// ── Per-user AI usage rate limit ──────────────────────────────────────────────

const TIER_LIMITS = {
  free: 10,
  pro: 500,
  premium: 2000,
};

const CACHE_TTL = 60; // seconds

/**
 * Check if user has remaining AI calls for today.
 * Uses KV cache with Supabase fallback.
 * Returns { used, limit } or { error, used, limit }
 */
export async function checkRateLimit(user, env) {
  const limit = TIER_LIMITS[user.tier] || TIER_LIMITS.free;
  const cacheKey = `usage:${user.id}:${todayKey()}`;

  let used;

  // 1. Try KV cache first
  const cached = await env.USAGE_CACHE.get(cacheKey);
  if (cached !== null) {
    used = parseInt(cached, 10);
  } else {
    // 2. Cache miss → query Supabase
    used = await fetchUsageFromSupabase(user.id, env);

    // 3. Cache the result
    await env.USAGE_CACHE.put(cacheKey, String(used), {
      expirationTtl: CACHE_TTL,
    });
  }

  if (used >= limit) {
    return {
      error: '今日 AI 使用次數已達上限',
      used,
      limit,
    };
  }

  return { used, limit };
}

async function fetchUsageFromSupabase(userId, env) {
  try {
    const res = await fetch(
      `${env.SUPABASE_URL}/rest/v1/rpc/get_ai_usage_today`,
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          apikey: env.SUPABASE_SERVICE_ROLE_KEY,
          Authorization: `Bearer ${env.SUPABASE_SERVICE_ROLE_KEY}`,
        },
        body: JSON.stringify({ p_user_id: userId }),
      }
    );

    if (!res.ok) {
      console.error('Supabase usage query failed:', res.status);
      return 0; // Fail open: allow the request
    }

    const count = await res.json();
    return typeof count === 'number' ? count : 0;
  } catch (err) {
    console.error('Failed to fetch usage:', err);
    return 0; // Fail open
  }
}
