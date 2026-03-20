// ============================================================
// JWT Authentication Middleware
// ============================================================
// Verifies Supabase JWT and extracts user ID + tier from claims
// ============================================================

import { jwtVerify, createRemoteJWKSet } from 'jose';

/**
 * Verify the Supabase JWT from Authorization header.
 * Returns { user: { id, tier, email } } or { error: string }
 */
export async function verifyAuth(request, env) {
  const authHeader = request.headers.get('Authorization');
  if (!authHeader || !authHeader.startsWith('Bearer ')) {
    return { error: 'Missing or invalid Authorization header' };
  }

  const token = authHeader.slice(7);

  try {
    // Verify JWT with Supabase JWT secret
    const secret = new TextEncoder().encode(env.SUPABASE_JWT_SECRET);
    const { payload } = await jwtVerify(token, secret, {
      issuer: `${env.SUPABASE_URL}/auth/v1`,
    });

    // Extract user info
    const userId = payload.sub;
    if (!userId) {
      return { error: 'Invalid token: missing subject' };
    }

    // Tier comes from custom_access_token_hook → app_metadata.tier
    const tier = payload.app_metadata?.tier || 'free';
    const email = payload.email || '';

    return {
      user: {
        id: userId,
        tier,
        email,
      },
    };
  } catch (err) {
    console.error('JWT verification failed:', err.message);
    if (err.code === 'ERR_JWT_EXPIRED') {
      return { error: 'Token expired' };
    }
    return { error: 'Invalid token' };
  }
}
