# Backend 搬遷說明

自 2026-05-17 起，KOF Note 的 shared backend 已從此 repo 抽離到：

- `github.com/keeponfirst/kofnote-gateway`

新的 backend repo 現在負責：

- Cloudflare Workers API Gateway
- Supabase schema migrations
- AI capture / chat / embedding backend
- 後續 digest、LINE 等跨 client backend 能力

本 repo 只保留 desktop app 相關程式碼。  
若要修改 Worker、Supabase migration、或部署 backend，請改到
`kofnote-gateway` 進行；不要在這個 repo 重新建立 `gateway/` 或 `supabase/`
副本。
