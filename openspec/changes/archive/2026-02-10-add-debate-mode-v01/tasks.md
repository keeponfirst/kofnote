## 1. Protocol and Contract Baseline

- [x] 1.1 Define `DebateModeRequest`/`DebateModeResponse` Rust + TypeScript types for v0.1 fixed protocol inputs/outputs.
- [x] 1.2 Define fixed role enum (`Proponent`, `Critic`, `Analyst`, `Synthesizer`, `Judge`) and fixed round enum (`Round1`, `Round2`, `Round3`).
- [x] 1.3 Define canonical `FinalPacket` JSON contract types in backend and frontend shared boundaries.
- [x] 1.4 Add validation helpers for required fields/enums (`output_type`, scores, risks, next_actions, timestamps).

## 2. Fixed State Machine Orchestrator

- [x] 2.1 Implement state enum and transition guard for `Intake -> Round1 -> Round2 -> Round3 -> Consensus -> Judge -> Packetize -> Writeback`.
- [x] 2.2 Implement `Intake` normalization (problem, constraints, output_type, run metadata).
- [x] 2.3 Implement `Round1` execution path for opening statements from all fixed roles.
- [x] 2.4 Implement `Round2` execution path enforcing cross-role challenge records.
- [x] 2.5 Implement `Round3` execution path enforcing revised-position records.
- [x] 2.6 Implement `Consensus` builder (consensus score, confidence score, agreements/disagreements).
- [x] 2.7 Implement `Judge` step (selected option, rejected options, risk summary, rationale).
- [x] 2.8 Implement `Packetize` step and block write when packet validation fails.

## 3. Provider Execution and Degradation

- [x] 3.1 Implement provider-routing policy for role participants (OpenAI/Gemini/Claude/local per request/config).
- [x] 3.2 Add per-turn timeout/token budget controls and usage telemetry capture.
- [x] 3.3 Implement degraded completion strategy when one participant fails (structured error artifact, no silent fallback).
- [x] 3.4 Ensure structural failures (storage write failure, schema validation failure) stop run with explicit failure reason.

## 4. Local-first Persistence and Indexing

- [x] 4.1 Implement run directory writer for `records/debates/<run_id>/request.json`.
- [x] 4.2 Implement per-round artifact writers for `round-1.json`, `round-2.json`, `round-3.json`.
- [x] 4.3 Implement consensus and packet writers (`consensus.json`, `final-packet.json`, `final-packet.md`).
- [x] 4.4 Add additive SQLite schema migration for `debate_runs`, `debate_turns`, `debate_actions` in existing index DB bootstrap path.
- [x] 4.5 Implement index upsert functions for run metadata, role turns, and action items.
- [x] 4.6 Ensure file writes are atomic and crash-safe for all debate artifacts.

## 5. Local Brain Writeback and Replay

- [x] 5.1 Implement mandatory writeback step that creates at least one linked Local Brain record (`decision` or `worklog`).
- [x] 5.2 Include `run_id` and final packet path references in writeback payload for traceability.
- [x] 5.3 Implement replay loader that reconstructs one full run from local artifacts without cloud dependency.
- [x] 5.4 Implement replay consistency checks between JSON artifacts and SQLite index rows.

## 6. Command Surface and App Integration

- [x] 6.1 Add Tauri command(s) for starting debate run and fetching replay/run detail.
- [x] 6.2 Add TypeScript invoke wrappers in `src/lib/tauri.ts` for debate run/replay APIs.
- [x] 6.3 Extend `src/types.ts` with debate request/result/final packet/replay contracts.
- [x] 6.4 Wire command-level errors to existing app notice/toast pipeline with machine-readable error codes.

## 7. Testing and Verification

- [x] 7.1 Add unit tests for state transition validator (valid/invalid transition matrix).
- [x] 7.2 Add unit tests for Final Packet validation (required fields, enum bounds, score range).
- [x] 7.3 Add integration tests for full happy-path run producing all required local artifacts.
- [x] 7.4 Add integration tests for participant failure degradation and successful run completion.
- [x] 7.5 Add integration tests for replay-from-local-only behavior (network disabled).
- [x] 7.6 Validate at least two provider-combination fixtures produce schema-identical packet structure.

## 8. Operational Readiness and Documentation

- [x] 8.1 Document Debate Mode v0.1 runtime configuration (provider mapping, budgets, limits).
- [x] 8.2 Document artifact paths and SQLite table semantics for audit/debug workflows.
- [x] 8.3 Add Definition-of-Done checklist mapping test evidence to spec requirements.
- [x] 8.4 Add rollback playbook (disable command exposure / keep additive schema dormant).
