// ============================================================
// Notion OAuth Handler (Phase 5 — Premium)
// ============================================================
// GET  /api/notion/oauth/start  → redirect to Notion OAuth consent page
// GET  /api/notion/oauth/callback → exchange code, store token in Supabase Vault
// ============================================================

/**
 * Redirect user to Notion OAuth consent page.
 * Requires: JWT auth (Premium tier only).
 * Injects state = base64(user_id) for CSRF protection.
 */
export function handleNotionOAuthStart(user, env) {
  if (user.tier !== 'premium') {
    return new Response(
      JSON.stringify({ error: 'Notion 整合需要 Premium 方案' }),
      { status: 403, headers: { 'Content-Type': 'application/json' } }
    );
  }

  const state = btoa(user.id);
  const params = new URLSearchParams({
    client_id: env.NOTION_CLIENT_ID,
    response_type: 'code',
    owner: 'user',
    redirect_uri: `${env.GATEWAY_BASE_URL}/api/notion/oauth/callback`,
    state,
  });

  return Response.redirect(
    `https://api.notion.com/v1/oauth/authorize?${params}`,
    302
  );
}

/**
 * Handle Notion OAuth callback.
 * Exchanges code for access_token, stores in Supabase Vault via profiles table.
 */
export async function handleNotionOAuthCallback(request, env) {
  const url = new URL(request.url);
  const code = url.searchParams.get('code');
  const state = url.searchParams.get('state');
  const error = url.searchParams.get('error');

  if (error) {
    return _redirectWithError('Notion 授權被拒絕', env);
  }

  if (!code || !state) {
    return _redirectWithError('缺少授權參數', env);
  }

  let userId;
  try {
    userId = atob(state);
  } catch {
    return _redirectWithError('無效的 state 參數', env);
  }

  // Exchange code for access token
  const credentials = btoa(`${env.NOTION_CLIENT_ID}:${env.NOTION_CLIENT_SECRET}`);
  const tokenRes = await fetch('https://api.notion.com/v1/oauth/token', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Basic ${credentials}`,
    },
    body: JSON.stringify({
      grant_type: 'authorization_code',
      code,
      redirect_uri: `${env.GATEWAY_BASE_URL}/api/notion/oauth/callback`,
    }),
  });

  if (!tokenRes.ok) {
    console.error('[Notion OAuth] token exchange failed:', tokenRes.status);
    return _redirectWithError('無法取得 Notion 授權 token', env);
  }

  const tokenData = await tokenRes.json();
  const accessToken = tokenData.access_token;
  const workspaceId = tokenData.workspace_id;
  const workspaceName = tokenData.workspace_name;

  // Store in Supabase profiles table (service role bypasses RLS)
  const updateRes = await fetch(
    `${env.SUPABASE_URL}/rest/v1/profiles?id=eq.${userId}`,
    {
      method: 'PATCH',
      headers: {
        'Content-Type': 'application/json',
        apikey: env.SUPABASE_SERVICE_ROLE_KEY,
        Authorization: `Bearer ${env.SUPABASE_SERVICE_ROLE_KEY}`,
        Prefer: 'return=minimal',
      },
      body: JSON.stringify({
        notion_access_token: accessToken,
        notion_workspace_id: workspaceId,
      }),
    }
  );

  if (!updateRes.ok) {
    console.error('[Notion OAuth] Supabase update failed:', updateRes.status);
    return _redirectWithError('儲存 Notion token 失敗', env);
  }

  // Redirect back to app with success
  const appCallback = `io.supabase.kofnote://notion-callback?success=true&workspace=${encodeURIComponent(workspaceName || '')}`;
  return Response.redirect(appCallback, 302);
}

function _redirectWithError(message, env) {
  const appCallback = `io.supabase.kofnote://notion-callback?error=${encodeURIComponent(message)}`;
  return Response.redirect(appCallback, 302);
}
