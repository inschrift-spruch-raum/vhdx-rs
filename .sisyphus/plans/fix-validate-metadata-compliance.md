# Fix `validate_metadata` Compliance with API Plan

## TL;DR
> **Summary**: Bring `SpecValidator::validate_metadata()` from a stub to full plan/spec compliance for metadata table, metadata entries, and known metadata item constraints, while preserving responsibility boundaries with `validate_required_metadata_items()`.
> **Deliverables**:
> - Fully implemented `validate_metadata()` constraint checks
> - Focused validator tests for metadata success/failure scenarios
> - Workspace verification evidence (tests + clippy)
> **Effort**: Medium
> **Parallel**: YES - 2 waves
> **Critical Path**: Task 1 → Task 2 → Task 3 → Task 5

## Context
### Original Request
Verify whether current implementation fully satisfies `docs/plan/API.md` (strictly, no more/no less), identify gaps, and produce concrete fix tasks.

### Interview Summary
- Current status is **not fully compliant**.
- Decisive gap: `validate_metadata()` is still a stub and does not enforce table/entry/known-item constraints required by API plan.
- Boundary confirmed: `validate_required_metadata_items()` remains responsible for required-item completeness/unknown-required behavior and must not be duplicated.

### Metis Review (gaps addressed)
- Add strict, explicit guardrails to prevent scope creep into required-item completeness and parent locator semantics.
- Enforce acceptance criteria with concrete command-level verification.
- Cover edge cases: duplicate IDs, overlap, entry bounds, zero-length rule, sector-size and block-size constraints.

## Work Objectives
### Core Objective
Implement metadata validation logic so `validate_metadata()` satisfies API plan semantics: metadata table constraints + entry constraints + known metadata item constraints (excluding required-item completeness checks).

### Deliverables
- Updated `src/validation.rs` with complete `validate_metadata()` checks.
- Validator tests in existing validation test module.
- Evidence files under `.sisyphus/evidence/` from command outputs.

### Definition of Done (verifiable conditions with commands)
- `validate_metadata()` no longer returns success unconditionally after simple metadata access.
- Metadata table/header constraints are validated with explicit error paths.
- Metadata entry constraints are validated with explicit error paths.
- Known-item constraints are validated with explicit error paths.
- Existing and new tests pass:
  - `cargo test -p vhdx-rs test_validate_metadata`
  - `cargo test --workspace`
- Lint passes:
  - `cargo clippy --workspace`

### Must Have
- Use `Error::InvalidMetadata(String)` for metadata constraint violations.
- Preserve `validate_required_metadata_items()` current responsibility and behavior.
- Maintain non-mutating behavior outside planned code/test files.

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- Do **NOT** move required-item completeness checks into `validate_metadata()`.
- Do **NOT** alter parent locator validation responsibility (`validate_parent_locator()`).
- Do **NOT** introduce unrelated refactors, new dependencies, or API surface changes.
- Do **NOT** modify `misc/` or Cargo manifests.

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + existing Rust test framework (`cargo test`)
- QA policy: every task includes executable happy-path + failure/edge scenario
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.txt`

## Execution Strategy
### Parallel Execution Waves
Wave 1: foundational metadata validator implementation tasks
- T1 (unspecified-high), T2 (unspecified-high), T3 (unspecified-high)

Wave 2: tests + full verification
- T4 (unspecified-high), T5 (unspecified-high)

### Dependency Matrix (full, all tasks)
- T1 blocks T2, T3, T4, T5
- T2 blocks T4, T5
- T3 blocks T4, T5
- T4 blocks T5
- T5 is terminal verification

### Agent Dispatch Summary (wave → task count → categories)
- Wave 1 → 3 tasks → unspecified-high
- Wave 2 → 2 tasks → unspecified-high

## TODOs

- [x] 1. Implement metadata table-header validation in `validate_metadata`

  **What to do**:
  - In `src/validation.rs`, expand `validate_metadata()` to validate metadata table/header-level constraints (signature/reserved/entry-count semantics per project API plan + spec-aligned expectations).
  - Keep error mapping consistent with existing validator style using `Error::InvalidMetadata(...)`.

  **Must NOT do**:
  - Do not add required-item completeness checks here.
  - Do not modify public API signatures.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: localized but logic-sensitive validator work
  - Skills: `[]` - no special skill required
  - Omitted: `review-work` - final verification wave handles review orchestration

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: T2,T3,T4,T5 | Blocked By: none

  **References**:
  - Pattern: `src/validation.rs` (`validate_header`, `validate_region_table`) - existing validator error/reporting pattern
  - API/Type: `src/validation.rs` (`validate_metadata`, `validate_required_metadata_items`)
  - External: `docs/plan/API.md` (validation contract), `misc/MS-VHDX.md` (§2.6 metadata)

  **Acceptance Criteria**:
  - [ ] `validate_metadata()` includes explicit table/header failure branches (not stub behavior)
  - [ ] Metadata table/header violations return `Error::InvalidMetadata`

  **QA Scenarios**:
  ```
  Scenario: Happy path metadata table
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_metadata
    Expected: metadata-related tests pass for valid fixture/files
    Evidence: .sisyphus/evidence/task-1-metadata-table-happy.txt

  Scenario: Corrupted table/header input
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_metadata_rejects_invalid_signature
    Expected: test fails validation with Error::InvalidMetadata
    Evidence: .sisyphus/evidence/task-1-metadata-table-error.txt
  ```

  **Commit**: NO | Message: `fix(validation): enforce metadata table header constraints` | Files: `src/validation.rs`, test files

- [x] 2. Implement metadata entry structural validation in `validate_metadata`

  **What to do**:
  - Validate entry-level constraints: unique identifiers, offset/length bounds, non-overlap, zero-length rule, and entry-count-related invariants as required by plan/spec.
  - Use metadata raw region length as bounds baseline from parsed metadata object.

  **Must NOT do**:
  - Do not duplicate required-item existence checks.
  - Do not alter metadata parsing structs unless strictly required for read-only access.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: multiple interdependent constraints and edge cases
  - Skills: `[]`
  - Omitted: `refactor` - avoid broad structural rewrites

  **Parallelization**: Can Parallel: NO | Wave 1 | Blocks: T4,T5 | Blocked By: T1

  **References**:
  - Pattern: `src/validation.rs` (`validate_region_table` entry loop style)
  - API/Type: `src/sections/metadata.rs` (metadata table/entry raw structures)
  - External: `docs/plan/API.md` (metadata validation scope), `misc/MS-VHDX.md` (§2.6.1.x)

  **Acceptance Criteria**:
  - [ ] Duplicate/overlap/out-of-range entry cases are rejected with `Error::InvalidMetadata`
  - [ ] Zero-length entry rule is enforced consistently

  **QA Scenarios**:
  ```
  Scenario: Valid non-overlapping entries
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_metadata_entry_constraints_happy
    Expected: validator accepts metadata with compliant entries
    Evidence: .sisyphus/evidence/task-2-entry-happy.txt

  Scenario: Duplicate or overlapping entries
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_metadata_rejects_duplicate_or_overlap
    Expected: validator rejects with Error::InvalidMetadata
    Evidence: .sisyphus/evidence/task-2-entry-error.txt
  ```

  **Commit**: NO | Message: `fix(validation): enforce metadata entry structural constraints` | Files: `src/validation.rs`, test files

- [x] 3. Implement known metadata item semantic constraints in `validate_metadata`

  **What to do**:
  - Validate known-item semantic constraints (e.g., block size rules, logical/physical sector size rules, virtual disk size alignment/range) required by plan/spec.
  - Keep ownership boundaries: this task validates value constraints, not required-item completeness.

  **Must NOT do**:
  - Do not move unknown-required-item rejection logic from `validate_required_metadata_items()`.
  - Do not enforce unrelated parent locator key semantics here.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: semantic correctness with spec-sensitive boundaries
  - Skills: `[]`
  - Omitted: `oracle` - architecture decision already made in planning

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: T4,T5 | Blocked By: T1

  **References**:
  - Pattern: `src/validation.rs` (`validate_bat` style known-item access)
  - API/Type: `src/sections/metadata.rs` (known metadata item accessors)
  - External: `docs/plan/API.md` (`validate_metadata` contract), `misc/MS-VHDX.md` (§2.6.2)

  **Acceptance Criteria**:
  - [ ] Invalid known-item value scenarios return `Error::InvalidMetadata`
  - [ ] Valid known-item values continue to pass existing validation flow

  **QA Scenarios**:
  ```
  Scenario: Known-item values valid
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_metadata_known_items_happy
    Expected: validator passes with compliant block/sector/size values
    Evidence: .sisyphus/evidence/task-3-known-items-happy.txt

  Scenario: Known-item values invalid
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_metadata_rejects_invalid_known_item_values
    Expected: validator rejects with Error::InvalidMetadata
    Evidence: .sisyphus/evidence/task-3-known-items-error.txt
  ```

  **Commit**: NO | Message: `fix(validation): enforce known metadata item constraints` | Files: `src/validation.rs`, test files

- [x] 4. Add/adjust metadata validation tests for plan-scope completeness

  **What to do**:
  - Add targeted tests in existing validator test module to cover:
    - valid metadata path,
    - table/header invalid cases,
    - entry invalid cases,
    - known-item invalid cases,
    - explicit non-overlap with required-item completeness path.
  - Ensure test naming clearly maps to violated constraint.

  **Must NOT do**:
  - Do not delete/disable existing tests.
  - Do not assert behavior outside metadata validator scope.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: high-value regression safety and contract proof
  - Skills: `[]`
  - Omitted: `playwright` - non-UI Rust library task

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: T5 | Blocked By: T2,T3

  **References**:
  - Pattern: existing test style in `src/validation.rs` test module
  - API/Type: `src/error.rs` (`Error::InvalidMetadata`)
  - External: `docs/plan/API.md` (scope boundary statement for `validate_metadata` vs `validate_required_metadata_items`)

  **Acceptance Criteria**:
  - [ ] New tests directly prove compliance for metadata table/entry/known-item constraints
  - [ ] Tests also prove scope boundary remains intact with required-item checks in separate function

  **QA Scenarios**:
  ```
  Scenario: Full metadata validator suite pass
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_metadata
    Expected: all metadata validator tests pass
    Evidence: .sisyphus/evidence/task-4-metadata-suite-happy.txt

  Scenario: Intentional malformed metadata fixtures
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_metadata_rejects
    Expected: malformed metadata cases fail with expected errors
    Evidence: .sisyphus/evidence/task-4-metadata-suite-error.txt
  ```

  **Commit**: NO | Message: `test(validation): add metadata compliance coverage` | Files: test files under `src/validation.rs` module

- [x] 5. Execute workspace-level verification and collect evidence

  **What to do**:
  - Run full project checks after implementation:
    - targeted metadata tests
    - full workspace tests
    - workspace clippy
  - Save command outputs to evidence files.

  **Must NOT do**:
  - Do not skip failing checks.
  - Do not suppress lints to pass verification.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: final integration safety gate
  - Skills: `[]`
  - Omitted: `quick` - verification breadth is non-trivial

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: none | Blocked By: T4

  **References**:
  - Build/Test commands in `AGENTS.md`
  - Existing CI-like local validation flow

  **Acceptance Criteria**:
  - [ ] `cargo test -p vhdx-rs test_validate_metadata` passes
  - [ ] `cargo test --workspace` passes
  - [ ] `cargo clippy --workspace` passes

  **QA Scenarios**:
  ```
  Scenario: All validations pass
    Tool: Bash
    Steps: cargo test -p vhdx-rs test_validate_metadata && cargo test --workspace && cargo clippy --workspace
    Expected: all commands return exit code 0
    Evidence: .sisyphus/evidence/task-5-workspace-happy.txt

  Scenario: Regression detection
    Tool: Bash
    Steps: run same command sequence after introducing a known-bad metadata fixture in test-only branch
    Expected: at least one metadata validation test fails, proving guard effectiveness
    Evidence: .sisyphus/evidence/task-5-workspace-error.txt
  ```

  **Commit**: NO | Message: `chore(validation): verify metadata compliance across workspace` | Files: none (verification only)

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.

- [x] F1. Plan Compliance Audit — oracle
- [x] F2. Code Quality Review — unspecified-high
- [x] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [x] F4. Scope Fidelity Check — deep

## Commit Strategy
- Prefer small atomic commits by constraint group:
  1) metadata table/header checks
  2) metadata entry checks
  3) known-item semantic checks
  4) tests
- Do not commit generated evidence files unless repository policy explicitly requires them.

## Success Criteria
- `validate_metadata()` behavior matches API plan scope exactly: table/entry/known-item constraints, excluding required-item completeness.
- `validate_required_metadata_items()` remains distinct and unchanged in responsibility.
- Full automated verification passes and evidence is captured.
