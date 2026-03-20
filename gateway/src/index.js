// ============================================================
// KOF Notes API Gateway — Cloudflare Workers
// ============================================================
// Entry point: handles routing, CORS, and error handling
// ============================================================

import { handleClassify } from './handlers/classify.js';
import { handleSummarize } from './handlers/summarize.js';
import { handleAnalyze } from './handlers/analyze.js';
import { handleNotionOAuthStart, handleNotionOAuthCallback } from './handlers/notion_oauth.js';
import { verifyAuth } from './middleware/auth.js';
import { checkRateLimit, checkIpRateLimit } from './middleware/rate-limit.js';
import { corsHeaders, handleCORS } from './middleware/cors.js';
import { todayKey } from './utils.js';

export default {
  async fetch(request, env, ctx) {
    // Handle CORS preflight
    if (request.method === 'OPTIONS') {
      return handleCORS(request, env);
    }

    const url = new URL(request.url);
    const path = url.pathname;

    // Health check
    if (path === '/api/health') {
      return jsonResponse({ status: 'ok', timestamp: new Date().toISOString() }, 200, env);
    }

    // ── Notion OAuth (GET, no body, JWT auth required) ──
    if (path === '/api/notion/oauth/callback') {
      return handleNotionOAuthCallback(request, env);
    }
    if (path === '/api/notion/oauth/start') {
      const ipCheck = await checkIpRateLimit(request, env);
      if (ipCheck.error) return jsonResponse({ error: ipCheck.error }, 429, env);
      const authResult = await verifyAuth(request, env);
      if (authResult.error) return jsonResponse({ error: authResult.error }, 401, env);
      return handleNotionOAuthStart(authResult.user, env);
    }

    // Only POST allowed for AI endpoints
    if (request.method !== 'POST') {
      return jsonResponse({ error: 'Method not allowed' }, 405, env);
    }

    // Route to handler
    const routes = {
      '/api/ai/classify': handleClassify,
      '/api/ai/summarize': handleSummarize,
      '/api/ai/analyze': handleAnalyze,
    };

    const handler = routes[path];
    if (!handler) {
      return jsonResponse({ error: 'Not found' }, 404, env);
    }

    try {
      // 0. IP rate limit (blocks hammering before any JWT work)
      const ipCheck = await checkIpRateLimit(request, env);
      if (ipCheck.error) {
        return jsonResponse({ error: ipCheck.error }, 429, env);
      }

      // 1. Verify JWT & extract user info
      const authResult = await verifyAuth(request, env);
      if (authResult.error) {
        return jsonResponse({ error: authResult.error }, 401, env);
      }

      // 2. Check rate limit
      const rateLimitResult = await checkRateLimit(authResult.user, env);
      if (rateLimitResult.error) {
        return jsonResponse({
          error: rateLimitResult.error,
          limit: rateLimitResult.limit,
          used: rateLimitResult.used,
        }, 429, env);
      }

      // 3. Parse request body
      let body;
      try {
        body = await request.json();
      } catch {
        return jsonResponse({ error: 'Invalid JSON body' }, 400, env);
      }

      // 4. Execute handler (calls AI model based on tier)
      const result = await handler(body, authResult.user, env);

      // 5. Log usage (async, don't block response)
      ctx.waitUntil(logUsage(authResult.user, path, result.model, result.tokens, env));

      // 6. Invalidate usage cache
      ctx.waitUntil(
        env.USAGE_CACHE.delete(`usage:${authResult.user.id}:${todayKey()}`)
      );

      // 7. Return result
      return jsonResponse({
        success: true,
        data: result.data,
        meta: {
          model: result.model,
          tier: authResult.user.tier,
          usage: {
            used: rateLimitResult.used + 1,
            limit: rateLimitResult.limit,
          },
        },
      }, 200, env);
    } catch (err) {
      console.error('Gateway error:', err);
      const isUserError = err.message?.startsWith('Input too long') || err.message?.startsWith('Missing required');
      return jsonResponse({ error: isUserError ? err.message : 'Internal server error' }, isUserError ? 400 : 500, env);
    }
  },
};

// ─── Helpers ───

function jsonResponse(data, status = 200, env) {
  return new Response(JSON.stringify(data), {
    status,
    headers: {
      'Content-Type': 'application/json',
      ...corsHeaders,
      'Access-Control-Allow-Origin': env?.CORS_ORIGIN || '*',
    },
  });
}

async function logUsage(user, path, model, tokens, env) {
  const action = path.split('/').pop(); // 'classify' | 'summarize' | 'analyze'
  try {
    await fetch(`${env.SUPABASE_URL}/rest/v1/ai_usage_log`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        apikey: env.SUPABASE_SERVICE_ROLE_KEY,
        Authorization: `Bearer ${env.SUPABASE_SERVICE_ROLE_KEY}`,
      },
      body: JSON.stringify({
        user_id: user.id,
        action,
        model,
        tokens: tokens || 0,
      }),
    });
  } catch (err) {
    console.error('Failed to log usage:', err);
  }
}

export { jsonResponse };
