# KOF Note Debate Mode v0.1 Runtime Guide

## Purpose

Debate Mode v0.1 is an internal KOF Note cognitive engine that executes a fixed protocol:

`Intake -> Round1 -> Round2 -> Round3 -> Consensus -> Judge -> Packetize -> Writeback`

This mode is local-first and replay-first. Cloud models are participants, not memory.

## Runtime Configuration

### Request Contract

Tauri command: `run_debate_mode`

Required fields:
- `problem`: debate problem statement
- `outputType`: `decision | writing | architecture | planning | evaluation`

Optional fields:
- `constraints: string[]`
- `participants: { role, modelProvider, modelName }[]`
- `maxTurnSeconds` (default `35`, range `5..120`)
- `maxTurnTokens` (default `900`, range `128..4096`)
- `writebackRecordType` (`decision` or `worklog`)

### Role and Round Rules

- Fixed roles: `Proponent`, `Critic`, `Analyst`, `Synthesizer`, `Judge`
- Fixed rounds: `round-1`, `round-2`, `round-3`
- Round-2 enforces cross-role challenge mapping
- Round-3 enforces revised positions

### Provider Routing Policy

Supported provider labels:
- `local`
- `openai`
- `gemini`
- `claude`

Notes:
- Unknown provider values are normalized to `local`
- Provider execution failures produce structured error codes and degraded completion (not silent abort)
- Structural failures (write failure, packet validation failure) stop the run

## Local-first Artifacts

For each run (`<run_id>`):

- `records/debates/<run_id>/request.json`
- `records/debates/<run_id>/rounds/round-1.json`
- `records/debates/<run_id>/rounds/round-2.json`
- `records/debates/<run_id>/rounds/round-3.json`
- `records/debates/<run_id>/consensus.json`
- `records/debates/<run_id>/final-packet.json`
- `records/debates/<run_id>/final-packet.md`

Writeback:
- At least one Local Brain record is created (`decision` or `worklog`)
- Final packet trace includes `writeback:<json_path>` reference

## SQLite Index Semantics

SQLite file:
- `<central_home>/.agentic/kofnote_search.sqlite`

Debate tables:
- `debate_runs`
  - Run metadata, scores, selected option, timestamps, artifact roots
- `debate_turns`
  - Turn-level role outputs, status, challenges/revisions JSON, error codes
- `debate_actions`
  - Final packet action list (`id`, `action`, `owner`, `due`, `status`)

Replay consistency checks compare:
- File artifacts vs. `debate_turns` count
- Final packet `nextActions` vs. `debate_actions` count

## Definition of Done Checklist

### Protocol and Contract
- [x] Fixed state machine transition guard implemented and tested
- [x] Fixed role/round enums implemented
- [x] Final packet schema validation implemented

### Local-first Persistence
- [x] Run artifacts written to local files
- [x] SQLite indexing for runs/turns/actions implemented
- [x] Writeback record linked via `run_id`/path

### Replay and Observability
- [x] `replay_debate_mode` reconstructs run from local artifacts
- [x] Replay consistency issues are surfaced explicitly

### Tests
- [x] Transition matrix unit test
- [x] Final packet bounds validation test
- [x] Happy-path run integration test
- [x] Provider-failure degraded completion test
- [x] Replay-from-local-only test
- [x] Provider-combination packet-shape consistency test

## Rollback Playbook

If Debate Mode must be rolled back:

1. Disable command exposure in Tauri invoke handler:
   - remove `run_debate_mode`
   - remove `replay_debate_mode`
2. Keep additive SQLite schema as dormant (no destructive migration required).
3. Keep existing artifacts on disk for audit purposes.
4. Re-enable only after failed scenario is reproduced in tests and fixed.

This rollback is low-risk because v0.1 adds isolated artifacts/tables without mutating existing record formats.
