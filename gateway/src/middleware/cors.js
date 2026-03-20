// ============================================================
// CORS Middleware
// ============================================================

// Base headers without origin (origin is set dynamically from env.CORS_ORIGIN)
export const corsHeaders = {
  'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
  'Access-Control-Allow-Headers': 'Content-Type, Authorization',
  'Access-Control-Max-Age': '86400',
};

export function handleCORS(request, env) {
  const origin = env?.CORS_ORIGIN || '*';
  return new Response(null, {
    status: 204,
    headers: {
      ...corsHeaders,
      'Access-Control-Allow-Origin': origin,
    },
  });
}
