# KOF Note Desktop Console

A desktop control panel for `keeponfirst-local-brain` central logs.

## Documentation

- Architecture: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- SDD with OpenSpec: [`docs/SDD_WITH_OPENSPEC.md`](docs/SDD_WITH_OPENSPEC.md)
- Debate Mode v0.1 runtime: [`docs/DEBATE_MODE_V01.md`](docs/DEBATE_MODE_V01.md)
- OpenSpec example spec: [`openspec/specs/example/spec.md`](openspec/specs/example/spec.md)
- Tauri app implementation notes: [`kofnote-app/README.md`](kofnote-app/README.md)

## SDD workflow (OpenSpec)

Use OpenSpec as SSOT before implementation changes:

```bash
# one-time setup
npm install -g @fission-ai/openspec@latest
cd /Users/pershing/Documents/henry/Fun/kofnote
openspec init

# per change
openspec new change "<change-id>"
openspec status --change "<change-id>"
openspec instructions <artifact> --change "<change-id>"
openspec instructions apply --change "<change-id>"
```

## What this app does

- Select one **Central Home** directory (the same root used by `keeponfirst-local-brain`)
- Visualize existing records and central logs
- CRUD records (`idea`, `worklog`, `decision`, `backlog`, `note`)
- Show dashboard insights (type distribution, recent activity, top tags)
- Run AI analysis:
  - Local heuristic summary (no API required)
  - OpenAI analysis (optional)

## Data compatibility

This app reads/writes the same storage layout as `keeponfirst-local-brain`:

- `records/{decisions,worklogs,ideas,backlogs,other}/*.json`
- `records/{...}/*.md`
- `.agentic/logs/*.json`

## Run

```bash
cd /Users/pershing/Documents/henry/Fun/kofnote
python3 main.py
```

No third-party dependency is required for the MVP (Tkinter + stdlib only).

## Optional OpenAI setup

In AI tab:
- Fill `API Key` and model (default `gpt-4.1-mini`)
- Click `OpenAI Analysis`

Or set env before launch:

```bash
export OPENAI_API_KEY="your_key"
python3 main.py
```

## Tests

```bash
cd /Users/pershing/Documents/henry/Fun/kofnote
python3 -m unittest discover -s tests -p 'test_*.py'
```

## Notes

- First time you pick Central Home, app persists config to:
  - `~/.kofnote-desktop/config.json`
- If your central path has no existing structure, the app will create required folders.
