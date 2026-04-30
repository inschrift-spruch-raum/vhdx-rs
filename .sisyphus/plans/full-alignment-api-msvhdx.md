# Full Alignment to API Plan and MS-VHDX

## TL;DR
> **Summary**: Drive implementation from current state to strict, evidence-backed full alignment against `docs/plan/API.md` (primary) and `misc/MS-VHDX.md` normative MUST/MUST NOT requirements.
> **Deliverables**:
> - Requirement-to-evidence conformance matrix (100% closed)
> - Code/test updates to eliminate all functional and semantic gaps
> - Reproducible verification artifacts for every requirement row
> **Effort**: Large
> **Parallel**: YES - 4 waves
> **Critical Path**: T1/T2 → T3/T4/T5 → T6/T7/T8/T9 → T10/T11/T12

## Context
### Original Request
- 检查实际实现与 `docs/plan/API.md` 和 `misc/MS-VHDX.md` 计划差别，以计划为准。
- 要求“完全对齐”，不接受“基本对齐”表述。

### Interview Summary
- User standard is binary: either fully aligned or not aligned.
- Prior audit found hard blockers:
  - `IO::write_sectors` 未实现。
  - Log replay / active sequence / read-only replay semantics require strict equivalence validation.
  - Test evidence not yet closure-grade for “full alignment” claim.

### Metis Review (gaps addressed)
- Added hard release gate: zero open MUST/MUST NOT rows.
- Added explicit anti-scope-creep guardrails.
- Added requirement matrix with deterministic evidence artifacts.
- Added negative-path QA requirements (MUST NOT cases).

## Work Objectives
### Core Objective
Achieve strict full alignment to planned API behavior and MS-VHDX normative requirements with executable, per-requirement evidence.

### Deliverables
- `.sisyphus/evidence/conformance-matrix.md` (generated during execution)
- Code changes closing all identified gaps
- Added/updated tests proving compliance and non-compliance rejection paths
- CLI behavior verification report

### Definition of Done (verifiable conditions with commands)
- `cargo test --workspace` exits 0
- `cargo clippy --workspace` exits 0
- `cargo fmt --check` exits 0
- Matrix has no `OPEN`, `PARTIAL`, or `UNKNOWN` rows for MUST/MUST NOT
- `vhdx-tool` critical commands pass defined conformance checks

### Must Have
- 1:1 traceability: requirement → implementation locus → executable proof → artifact
- All MUST/MUST NOT clauses relevant to scope have binary pass/fail status
- Log replay semantics proven by scenario tests (including failure paths)

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- Must NOT claim completion with “mostly”, “basically”, or “appears compliant”
- Must NOT close requirement rows without evidence artifact
- Must NOT perform unrelated refactor/perf/cleanup outside conformance scope
- Must NOT remove or weaken tests to make CI pass

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + existing Rust test framework (`cargo test`)
- QA policy: Every task includes executable happy + failure/edge scenarios
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`

## Execution Strategy
### Parallel Execution Waves
Wave 1: conformance baseline and matrix foundations
- T1, T2

Wave 2: blocker remediation and high-risk semantics
- T3, T4, T5

Wave 3: coverage closure for API/CLI/spec-critical paths
- T6, T7, T8, T9

Wave 4: hard-gate verification and gap burn-down
- T10, T11, T12

### Dependency Matrix (full, all tasks)
- T1 blocks T2-T12
- T2 blocks T3-T12
- T3 blocks T10-T12
- T4 blocks T10-T12
- T5 blocks T10-T12
- T6 blocks T10-T12
- T7 blocks T10-T12
- T8 blocks T10-T12
- T9 blocks T10-T12
- T10 blocks T11,T12
- T11 blocks T12

### Agent Dispatch Summary (wave → task count → categories)
- Wave1 → 2 tasks → deep/unspecified-high
- Wave2 → 3 tasks → ultrabrain/deep
- Wave3 → 4 tasks → unspecified-high/quick
- Wave4 → 3 tasks → deep/unspecified-high

## TODOs

- [ ] 1. Build Conformance Matrix Backbone

  **What to do**: Create a requirement matrix covering all relevant rows from `docs/plan/API.md` and normative MUST/MUST NOT from `misc/MS-VHDX.md` within current implementation scope. Include fields: source clause, requirement text, code locus, test locus, command, expected output, evidence path, status.
  **Must NOT do**: Must not leave unnamed or untraceable rows.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: clause mapping accuracy is foundational.
  - Skills: `[]` - no additional skills required.
  - Omitted: `[]` - n/a.

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: T2-T12 | Blocked By: none

  **References**:
  - Pattern: `docs/plan/API.md` - planned API baseline.
  - External: `misc/MS-VHDX.md` - normative requirement source.
  - Test: `tests/integration_test.rs` - existing behavior evidence anchor.

  **Acceptance Criteria**:
  - [ ] Matrix file exists and includes all scoped API and MUST/MUST NOT rows.
  - [ ] Every row includes command + expected result + evidence path.

  **QA Scenarios**:
  ```
  Scenario: Matrix completeness pass
    Tool: Bash
    Steps: Open matrix; run consistency checks for empty required fields
    Expected: Zero rows missing source/command/evidence path
    Evidence: .sisyphus/evidence/task-1-matrix-completeness.txt

  Scenario: Matrix completeness fail detection
    Tool: Bash
    Steps: Run same checker against intentionally incomplete sample row
    Expected: Checker reports failure and row identifier
    Evidence: .sisyphus/evidence/task-1-matrix-fail.txt
  ```

  **Commit**: NO | Message: `chore(conformance): create requirement matrix scaffold` | Files: conformance matrix artifact

- [ ] 2. Baseline Gap Registration and Severity Tagging

  **What to do**: Populate matrix statuses from current codebase and classify each gap using strict taxonomy: A(plan mismatch), B(spec violation risk), C(behavior/test gap), D(API surface mismatch). Tag severity critical/high/medium/low.
  **Must NOT do**: Must not mark unknown rows as pass.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: broad classification workload.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: T3-T12 | Blocked By: T1

  **References**:
  - Pattern: `src/file.rs` - File, replay, options, dynamic IO.
  - Pattern: `src/io_module.rs` - sector and batch IO behavior.
  - Pattern: `src/validation.rs` - validator implementation scope.

  **Acceptance Criteria**:
  - [ ] All rows classified and severity-tagged.
  - [ ] `IO::write_sectors` appears as non-pass blocker row.

  **QA Scenarios**:
  ```
  Scenario: Classification consistency
    Tool: Bash
    Steps: Run validator script ensuring every non-pass row has class+severity
    Expected: Pass with zero missing tags
    Evidence: .sisyphus/evidence/task-2-classification-pass.txt

  Scenario: Missing severity detection
    Tool: Bash
    Steps: Run validator against sample row without severity
    Expected: Fails with explicit row id
    Evidence: .sisyphus/evidence/task-2-classification-fail.txt
  ```

  **Commit**: NO | Message: `docs(conformance): classify baseline gaps` | Files: matrix artifact

- [ ] 3. Implement `IO::write_sectors` to Planned Contract

  **What to do**: Implement missing `IO::write_sectors` behavior fully consistent with planned API semantics and existing sector write contracts.
  **Must NOT do**: Must not bypass bounds/alignment checks.

  **Recommended Agent Profile**:
  - Category: `ultrabrain` - Reason: correctness-critical data-path implementation.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: T10-T12 | Blocked By: T2

  **References**:
  - Pattern: `src/io_module.rs` - `sector`, `read_sectors`, `Sector::write` semantics.
  - Pattern: `src/file.rs` - `write_raw` and dynamic allocation path.
  - Test: `tests/integration_test.rs` - dynamic/fixed write expectations.

  **Acceptance Criteria**:
  - [ ] `IO::write_sectors` no longer returns unimplemented error.
  - [ ] Supports multi-sector writes with proper bounds behavior.

  **QA Scenarios**:
  ```
  Scenario: Multi-sector happy write
    Tool: Bash
    Steps: Run targeted test for multi-sector write/readback integrity
    Expected: Exact data round-trip match
    Evidence: .sisyphus/evidence/task-3-write-sectors-happy.txt

  Scenario: Out-of-range write rejection
    Tool: Bash
    Steps: Run targeted test writing past virtual disk size
    Expected: Deterministic error variant and no data corruption
    Evidence: .sisyphus/evidence/task-3-write-sectors-error.txt
  ```

  **Commit**: YES | Message: `feat(io): implement write_sectors with bounds-safe semantics` | Files: `src/io_module.rs`, tests

- [ ] 4. Align Log Replay Active Sequence Semantics

  **What to do**: Verify and adjust log active-sequence detection and replay ordering to be spec-equivalent for scoped clauses (§2.3.2/§2.3.3), including truncated/invalid entry handling.
  **Must NOT do**: Must not replay non-active or GUID-mismatched entries.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: algorithmic correctness with spec constraints.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: T10-T12 | Blocked By: T2

  **References**:
  - Pattern: `src/sections/log.rs` - sequence scan and replay pipeline.
  - Pattern: `src/file.rs` - replay policy integration.
  - External: `misc/MS-VHDX.md` §2.3.2 §2.3.3.

  **Acceptance Criteria**:
  - [ ] Active sequence selection behavior matches matrix clauses.
  - [ ] Invalid/mismatched sequences are rejected per clause requirements.

  **QA Scenarios**:
  ```
  Scenario: Valid active sequence replay
    Tool: Bash
    Steps: Run replay tests with crafted valid sequence chain
    Expected: Only active sequence entries applied in order
    Evidence: .sisyphus/evidence/task-4-log-replay-happy.txt

  Scenario: GUID mismatch rejection
    Tool: Bash
    Steps: Run test where entry LogGuid != header LogGuid
    Expected: Entry excluded from replay; expected failure/safe behavior
    Evidence: .sisyphus/evidence/task-4-log-replay-error.txt
  ```

  **Commit**: YES | Message: `fix(log): enforce active-sequence replay semantics` | Files: `src/sections/log.rs`, `src/file.rs`, tests

- [ ] 5. Reconcile ReadOnly Replay Policy Behavior

  **What to do**: Make read-only replay modes align with explicit plan/spec decisions, with deterministic behavior for `Require/Auto/InMemoryOnReadOnly/ReadOnlyNoReplay` and corresponding safety guarantees.
  **Must NOT do**: Must not silently mutate disk in read-only mode.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: policy semantics and safety invariants.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: T10-T12 | Blocked By: T2

  **References**:
  - Pattern: `src/file.rs` - `handle_log_replay`, overlay application.
  - Test: `tests/integration_test.rs` - readonly policy tests.

  **Acceptance Criteria**:
  - [ ] Policy behaviors are unambiguous and test-locked.
  - [ ] Read-only modes preserve no-write guarantee where required.

  **QA Scenarios**:
  ```
  Scenario: ReadOnlyNoReplay structural read
    Tool: Bash
    Steps: Open pending-log file with ReadOnlyNoReplay and read metadata
    Expected: Open succeeds for structure reads; pending state preserved
    Evidence: .sisyphus/evidence/task-5-readonly-happy.txt

  Scenario: Require policy blocks pending log
    Tool: Bash
    Steps: Open same file with Require policy
    Expected: Deterministic LogReplayRequired-style error
    Evidence: .sisyphus/evidence/task-5-readonly-error.txt
  ```

  **Commit**: YES | Message: `fix(file): harden readonly replay policy semantics` | Files: `src/file.rs`, tests

- [ ] 6. Close Validator Clause Coverage Gaps

  **What to do**: Ensure `SpecValidator` methods fully map to scoped matrix clauses and enforce missing MUST/MUST NOT checks.
  **Must NOT do**: Must not leave validator methods with undocumented partial scope.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: systematic rule completeness work.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: T10-T12 | Blocked By: T2

  **References**:
  - Pattern: `src/validation.rs` - validation entry points.
  - External: `misc/MS-VHDX.md` §2.2-§2.6 relevant MUST clauses.

  **Acceptance Criteria**:
  - [ ] Every validator row in matrix has executable positive+negative proof.
  - [ ] No scoped MUST clause remains untested for validator behavior.

  **QA Scenarios**:
  ```
  Scenario: Validator full pass on compliant fixture
    Tool: Bash
    Steps: Run targeted validator tests on known-good fixture
    Expected: All checks succeed
    Evidence: .sisyphus/evidence/task-6-validator-happy.txt

  Scenario: Validator rejects required-unknown item
    Tool: Bash
    Steps: Run targeted malformed fixture test
    Expected: Deterministic rejection with expected error class
    Evidence: .sisyphus/evidence/task-6-validator-error.txt
  ```

  **Commit**: YES | Message: `test(validation): close MUST-clause coverage gaps` | Files: `src/validation.rs`, tests

- [ ] 7. Strengthen BAT State Compliance Tests

  **What to do**: Add/adjust tests to enforce scoped BAT state legality and behavior matrix across fixed/dynamic/differencing expectations.
  **Must NOT do**: Must not assert behavior not allowed by scoped policy decisions.

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: targeted test expansion.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: T10-T12 | Blocked By: T2

  **References**:
  - Pattern: `src/sections/bat.rs` - state model.
  - Test: `tests/integration_test.rs` BAT-related tests.
  - External: `misc/MS-VHDX.md` §2.5.1.1 §2.5.1.2.

  **Acceptance Criteria**:
  - [ ] BAT state legality rows have explicit tests.
  - [ ] Differencing partial-present + bitmap dependency enforced in tests.

  **QA Scenarios**:
  ```
  Scenario: Legal BAT states pass
    Tool: Bash
    Steps: Run BAT compliance test set
    Expected: Allowed state/type combinations pass
    Evidence: .sisyphus/evidence/task-7-bat-happy.txt

  Scenario: Illegal state/type combination fails
    Tool: Bash
    Steps: Run malformed BAT case test
    Expected: Validator/open path rejects file deterministically
    Evidence: .sisyphus/evidence/task-7-bat-error.txt
  ```

  **Commit**: YES | Message: `test(bat): enforce state legality matrix` | Files: tests

- [ ] 8. Strengthen Metadata Required-Item/Flag Compliance Tests

  **What to do**: Expand tests for metadata table constraints: required known items, offsets/length constraints, flag semantics and rejection behavior.
  **Must NOT do**: Must not leave zero-length/offset coupling untested if scoped.

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: focused negative-case test additions.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: T10-T12 | Blocked By: T2

  **References**:
  - Pattern: `src/sections/metadata.rs`
  - Pattern: `src/validation.rs` metadata validators.
  - External: `misc/MS-VHDX.md` §2.6.1.2 §2.6.2.

  **Acceptance Criteria**:
  - [ ] Required-item rows fully evidence-backed.
  - [ ] Invalid metadata layout cases produce deterministic failure.

  **QA Scenarios**:
  ```
  Scenario: Required metadata set passes
    Tool: Bash
    Steps: Run metadata compliance positive tests
    Expected: Pass
    Evidence: .sisyphus/evidence/task-8-metadata-happy.txt

  Scenario: Missing/unknown required metadata fails
    Tool: Bash
    Steps: Run malformed required-item tests
    Expected: Deterministic rejection
    Evidence: .sisyphus/evidence/task-8-metadata-error.txt
  ```

  **Commit**: YES | Message: `test(metadata): close required-item compliance gaps` | Files: tests

- [ ] 9. Close CLI Conformance Evidence Gaps

  **What to do**: Add CLI tests proving conformance-critical flows: non-dry-run repair effects, differencing create positive path, strict JSON/exit semantics for key commands.
  **Must NOT do**: Must not rely on loose substring-only assertions for conformance rows.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: cross-command behavior closure.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: T10-T12 | Blocked By: T2

  **References**:
  - Pattern: `vhdx-cli/src/cli.rs` and `vhdx-cli/src/commands/*.rs`.
  - Test: `vhdx-cli/tests/cli_integration.rs`.

  **Acceptance Criteria**:
  - [ ] Repair writeback path has deterministic integration test.
  - [ ] Differencing create positive path has explicit pass assertion.
  - [ ] Critical JSON outputs checked for schema/value semantics.

  **QA Scenarios**:
  ```
  Scenario: CLI repair writeback happy path
    Tool: Bash
    Steps: Run integration test simulating pending-log and repair (non-dry-run)
    Expected: Pending state cleared and subsequent check passes
    Evidence: .sisyphus/evidence/task-9-cli-happy.txt

  Scenario: CLI invalid path/arg failure behavior
    Tool: Bash
    Steps: Run invalid parent/arg integration tests
    Expected: Exact exit code and deterministic error text
    Evidence: .sisyphus/evidence/task-9-cli-error.txt
  ```

  **Commit**: YES | Message: `test(cli): close conformance-critical behavior gaps` | Files: `vhdx-cli/tests/cli_integration.rs`

- [ ] 10. Execute Full Verification Gate

  **What to do**: Run full test/lint/format gates and collect artifacts linked in matrix.
  **Must NOT do**: Must not skip failures.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: full gate execution and triage.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: NO | Wave 4 | Blocks: T11,T12 | Blocked By: T3-T9

  **References**:
  - Command: `cargo test --workspace`
  - Command: `cargo clippy --workspace`
  - Command: `cargo fmt --check`

  **Acceptance Criteria**:
  - [ ] All gates pass with artifacts saved.

  **QA Scenarios**:
  ```
  Scenario: Full gate pass
    Tool: Bash
    Steps: Execute test/clippy/fmt commands
    Expected: All exit 0
    Evidence: .sisyphus/evidence/task-10-full-gate-pass.txt

  Scenario: Gate failure capture
    Tool: Bash
    Steps: Run with known failing branch state snapshot (if present)
    Expected: Failure output captured and linked to rows
    Evidence: .sisyphus/evidence/task-10-full-gate-fail.txt
  ```

  **Commit**: NO | Message: `chore(qa): run full conformance verification gates` | Files: evidence only

- [ ] 11. Burn Down Remaining Non-Pass Matrix Rows

  **What to do**: Resolve any rows still marked non-pass after T10. For each remaining row, implement/test/fix and re-run linked verification.
  **Must NOT do**: Must not downgrade severity to avoid blockers.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: targeted high-risk remediation.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: NO | Wave 4 | Blocks: T12 | Blocked By: T10

  **References**:
  - Artifact: `.sisyphus/evidence/task-10-full-gate-pass.txt`
  - Matrix: `.sisyphus/evidence/conformance-matrix.md`

  **Acceptance Criteria**:
  - [ ] No MUST/MUST NOT rows remain non-pass.

  **QA Scenarios**:
  ```
  Scenario: Remaining-row remediation pass
    Tool: Bash
    Steps: Re-run only linked commands for previously failing rows
    Expected: All rows transition to PASS with new artifacts
    Evidence: .sisyphus/evidence/task-11-remediation-happy.txt

  Scenario: Regression detection
    Tool: Bash
    Steps: Re-run high-risk regression subset after fixes
    Expected: No previously passing row regresses
    Evidence: .sisyphus/evidence/task-11-remediation-error.txt
  ```

  **Commit**: YES | Message: `fix(conformance): close remaining non-pass requirement rows` | Files: as needed

- [ ] 12. Finalize Conformance Report (Binary Verdict)

  **What to do**: Produce final report from matrix with binary verdict only: `ALIGNED` or `NOT ALIGNED`, with explicit row references.
  **Must NOT do**: Must not use hedged language.

  **Recommended Agent Profile**:
  - Category: `writing` - Reason: precise evidence-linked reporting.
  - Skills: `[]`
  - Omitted: `[]`

  **Parallelization**: Can Parallel: NO | Wave 4 | Blocks: none | Blocked By: T11

  **References**:
  - Matrix: `.sisyphus/evidence/conformance-matrix.md`
  - Evidence directory: `.sisyphus/evidence/`

  **Acceptance Criteria**:
  - [ ] Report includes verdict and zero-ambiguity rationale.
  - [ ] Every claim references at least one artifact.

  **QA Scenarios**:
  ```
  Scenario: Binary verdict generation
    Tool: Bash
    Steps: Run report generator/checklist script
    Expected: Output is ALIGNED or NOT ALIGNED only
    Evidence: .sisyphus/evidence/task-12-report-happy.txt

  Scenario: Hedging language lint
    Tool: Bash
    Steps: Run text lint for forbidden hedge terms (mostly/basically/appears)
    Expected: Zero forbidden terms in final report
    Evidence: .sisyphus/evidence/task-12-report-error.txt
  ```

  **Commit**: NO | Message: `docs(conformance): publish binary verdict report` | Files: evidence/report only

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [ ] F1. Plan Compliance Audit — oracle
- [ ] F2. Code Quality Review — unspecified-high
- [ ] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [ ] F4. Scope Fidelity Check — deep

## Commit Strategy
- Atomic commits per conformance cluster (IO/log/validator/BAT/metadata/CLI/tests).
- Never mix unrelated changes in the same commit.
- Commit messages use `type(scope): desc` and reference matrix row IDs.

## Success Criteria
- Binary verdict is `ALIGNED`.
- Zero open/partial/unknown MUST/MUST NOT rows.
- Full gate (`test + clippy + fmt`) clean.
- No hedge language in final claim.
