# KOF Notes API Gateway

Cloudflare Workers-based API Gateway for AI features.

## Architecture

```
Client (JWT) → CF Worker → Auth → Rate Limit → AI Model → Response
                                                    │
                                              Usage Log (async)
```

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/ai/classify` | 分類文字 → record_type + title |
| POST | `/api/ai/summarize` | 長文摘要 + key insight |
| POST | `/api/ai/analyze` | 決策/想法優缺點分析 |
| GET | `/api/health` | Health check |

## Setup

### 1. Install

```bash
cd gateway
npm install
```

### 2. Create KV Namespace

```bash
npx wrangler kv:namespace create USAGE_CACHE
npx wrangler kv:namespace create USAGE_CACHE --preview
```

Update `wrangler.toml` with the returned namespace IDs.

### 3. Set Secrets

```bash
npx wrangler secret put SUPABASE_JWT_SECRET
npx wrangler secret put SUPABASE_SERVICE_ROLE_KEY
npx wrangler secret put GEMINI_API_KEY
npx wrangler secret put ANTHROPIC_API_KEY
```

### 4. Dev

```bash
npm run dev
```

### 5. Deploy

```bash
npm run deploy
```

## Request Format

```bash
curl -X POST https://your-worker.workers.dev/api/ai/classify \
  -H "Authorization: Bearer <supabase-jwt>" \
  -H "Content-Type: application/json" \
  -d '{"text": "明天要記得買咖啡豆"}'
```

## Response Format

```json
{
  "success": true,
  "data": {
    "record_type": "backlog",
    "title": "買咖啡豆",
    "tags": ["shopping", "coffee"],
    "confidence": 0.9
  },
  "meta": {
    "model": "gemini-1.5-flash",
    "tier": "free",
    "usage": { "used": 3, "limit": 10 }
  }
}
```

## Tier → Model Mapping

| Tier | Model |
|------|-------|
| free | Gemini 1.5 Flash |
| pro | Gemini 1.5 Pro |
| premium | Claude Sonnet |

## File Structure

```
gateway/
├── package.json
├── wrangler.toml
├── README.md
└── src/
    ├── index.js              ← Entry point + routing
    ├── ai/
    │   └── router.js         ← Tier → model routing
    ├── handlers/
    │   ├── classify.js       ← /api/ai/classify
    │   ├── summarize.js      ← /api/ai/summarize
    │   └── analyze.js        ← /api/ai/analyze
    └── middleware/
        ├── auth.js           ← JWT verification
        ├── rate-limit.js     ← KV cache + rate limiting
        └── cors.js           ← CORS headers
```
