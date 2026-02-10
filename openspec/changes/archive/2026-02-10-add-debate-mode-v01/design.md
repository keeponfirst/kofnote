## Context

`kofnote-app` is a local-first desktop app (React + Tauri) with Rust commands for:
- reading/writing record files under `<central_home>/records/...`
- maintaining a SQLite search index at `<central_home>/.agentic/kofnote_search.sqlite`
- running AI analysis via provider adapters
- writing local markdown/json artifacts and settings

The v0.1 debate change must stay inside KOF Note as an internal mode, not an external service.  
The core requirement is a deterministic, replayable, auditable workflow that turns one problem input into a multi-role debate, then a machine-usable Final Packet and Local Brain writeback.

Primary constraints from spec:
- fixed state machine (8 states)
- fixed 5 roles, fixed 3 rounds
- local-first persistence as source of truth
- no dynamic role growth, no self-evolving prompts, no multi-user workflow
- no UI-level design decisions in this artifact

Stakeholder:
- single operator (the user) using KOF Note as control plane for real product/architecture/writing decisions.

## Goals / Non-Goals

**Goals:**
- Add a backend debate orchestration flow that is deterministic and replay-safe.
- Define stable data contracts between frontend and backend for debate requests/results.
- Persist full run artifacts locally (json + markdown + sqlite index) for search, audit, and replay.
- Produce a strict Final Packet that downstream tooling can execute.
- Ensure graceful degradation if one participant/provider fails.

**Non-Goals:**
- Designing UI/visual graph interactions for Debate Mode.
- Implementing dynamic role counts, adaptive round counts, or autonomous prompt evolution.
- Supporting collaborative/multi-user debate sessions.
- Building a remote orchestration service.

## Decisions

### Decision 1: Debate orchestration runs in Tauri Rust backend as a first-class command

**Decision**
- Implement Debate Mode as new Rust command(s) in `src-tauri/src/main.rs` flow, exposed through `src/lib/tauri.ts`.
- Keep frontend responsible for submitting request/config and rendering results; keep backend responsible for protocol execution, model calls, and persistence.

**Why**
- Existing architecture already centralizes filesystem writes, key management, and provider execution in Rust.
- Prevents leaking local-first guarantees into UI code.
- Reuses current command/invoke pattern and error handling conventions.

**Alternatives considered**
- Orchestrate directly in React frontend: rejected due to weaker control over file integrity and replay guarantees.
- Separate local daemon/service: rejected for v0.1 complexity and operational overhead.

### Decision 2: Fixed protocol modeled as explicit state machine enum + transition validator

**Decision**
- Encode states as a strict enum:
  `Intake -> Round1 -> Round2 -> Round3 -> Consensus -> Judge -> Packetize -> Writeback`
- Persist a state transition log per run (ordered, timestamped, status) and reject illegal transitions.

**Why**
- Enforces deterministic behavior required by spec.
- Improves debuggability and replay (can see exactly where failure/degradation occurred).

**Alternatives considered**
- Ad-hoc chained function calls without transition checks: rejected; too easy to skip/duplicate states.
- Config-driven arbitrary workflow graph: rejected for v0.1 simplicity and predictability.

### Decision 3: Fixed participant roster with explicit role responsibilities

**Decision**
- Hardcode exactly 5 roles in v0.1: `Proponent`, `Critic`, `Analyst`, `Synthesizer`, `Judge`.
- Hardcode exactly 3 rounds before consensus/judgement.
- Validate round payload shape:
  - Round1: opening claim/rationale/risks
  - Round2: cross-role challenge(s)
  - Round3: revised position + deltas from earlier rounds

**Why**
- Matches v0.1 requirement to prioritize control and stable output over flexibility.
- Reduces run variance and schema drift.

**Alternatives considered**
- User-configurable roles/rounds in v0.1: rejected (scope creep, unstable packets).
- Single-model multi-prompt simulation: rejected (weak conflict realism, weaker audit semantics).

### Decision 4: Final Packet validated against strict backend schema before writeback

**Decision**
- Build packet in backend from normalized intermediate data.
- Validate required fields and enums before persisting `final-packet.json`.
- Generate companion `final-packet.md` for human review from the same source object.

**Why**
- Prevents malformed output from reaching Local Brain and downstream automation.
- Ensures one canonical machine contract and one human-readable projection.

**Alternatives considered**
- Let frontend build/format packet: rejected; contract integrity should remain close to persistence boundary.
- Persist only markdown narrative: rejected; weak machine actionability.

### Decision 5: Local persistence uses run directory + SQLite extension in existing index database

**Decision**
- Persist run artifacts under:
  - `records/debates/<run_id>/request.json`
  - `records/debates/<run_id>/rounds/round-1.json`
  - `records/debates/<run_id>/rounds/round-2.json`
  - `records/debates/<run_id>/rounds/round-3.json`
  - `records/debates/<run_id>/consensus.json`
  - `records/debates/<run_id>/final-packet.json`
  - `records/debates/<run_id>/final-packet.md`
- Reuse existing DB file `<central_home>/.agentic/kofnote_search.sqlite`, adding debate tables:
  - `debate_runs`
  - `debate_turns`
  - `debate_actions`

**Why**
- Keeps all search/report state local and co-located with existing index strategy.
- Avoids introducing another DB lifecycle in v0.1.

**Alternatives considered**
- Separate debate DB file: rejected to minimize operational footprint and migration steps.
- File-only (no SQL index): rejected because replay/search/filtering would be expensive and brittle.

### Decision 6: Local Brain writeback is mandatory and linked by run_id

**Decision**
- After packetization, always write at least one Local Brain record (`decision` or `worklog`) referencing:
  - `run_id`
  - `final-packet.json` path
  - condensed rationale/risks/actions
- Writeback executes in the final state and is logged in state transition artifacts.

**Why**
- Enforces source-of-truth requirement: debate output becomes part of durable personal memory.
- Enables downstream tooling to trace from decision record to full debate evidence.

**Alternatives considered**
- Optional/manual writeback toggle in v0.1: rejected because it breaks consistency of audit trails.

### Decision 7: Degraded completion strategy on provider failure

**Decision**
- If one participant call fails or times out:
  - mark participant turn as failed with structured error
  - continue flow with reduced evidence
  - include degradation markers in consensus/judge/final packet
- Abort only on structural failures (storage unavailable, packet validation failure).

**Why**
- Meets requirement: no black-box termination and full postmortem visibility.

**Alternatives considered**
- Immediate run termination on any role failure: rejected (too fragile for multi-provider reality).
- Silent fallback using fake/generated role output: rejected (damages trust and auditability).

## Risks / Trade-offs

- **[Provider output variability]** -> Mitigation: strict intermediate schema normalization and validation per round.
- **[Longer runtime from fixed multi-round protocol]** -> Mitigation: explicit timeout/token budgets and surfaced metrics.
- **[SQLite schema growth in existing index DB]** -> Mitigation: additive, idempotent migrations and version marker in meta table.
- **[Replay drift if markdown is edited manually]** -> Mitigation: replay from JSON artifacts as canonical source; markdown treated as projection.
- **[Single-file `main.rs` complexity]** -> Mitigation: isolate debate logic into internal Rust modules while preserving current command surface.

## Migration Plan

1. Add new debate data types and protocol enums (request, round payloads, consensus, judge summary, final packet).
2. Add additive SQLite migration for `debate_runs`, `debate_turns`, `debate_actions` in existing DB bootstrap path.
3. Add debate orchestrator with fixed transition validator and per-state persistence writers.
4. Add provider execution adapter for debate roles reusing current key/provider infrastructure.
5. Add Local Brain writeback adapter that creates linked `decision`/`worklog` records.
6. Expose command(s) via Tauri invoke and typed TS wrappers.
7. Add replay/read APIs to reconstruct one run from local artifacts and SQL index.
8. Validate with deterministic fixtures and failure-injection tests.

Rollback strategy:
- Feature-flag debate commands at runtime/config level until validation passes.
- If rollback needed, disable command exposure and keep additive schema dormant (no destructive down-migration in v0.1).

## Open Questions

- Should v0.1 default writeback type be always `decision`, or chosen by `output_type` mapping (`writing` => `worklog`)?
- For cross-provider runs, do we require one provider per role by default, or allow multiple roles to share provider/model?
- Do we need explicit run-level checksum/signature for tamper evidence in v0.1, or defer to v0.2?
