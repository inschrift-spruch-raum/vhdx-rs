# API.md Strict Parity Remediation Plan

## TL;DR
> **Summary**: Repair all confirmed blocking mismatches between implementation and `docs/plan/API.md` with strict plan-first semantics, while keeping `docs/plan/API.md` read-only.
> **Deliverables**:
> - strict-mode behavior implemented (no longer no-op)
> - parent locator/chain validation semantics aligned to plan
> - `Log::entry(index)` implemented
> - `InMemoryOnReadOnly` semantics aligned to plan
> - BAT sector-bitmap parsing path implemented
> - `StandardItems` public namespace exposed per plan
> - regression tests + raw evidence for each task
> **Effort**: Medium
> **Parallel**: YES - 3 waves
> **Critical Path**: T1/T2 → T3/T4/T5 → T6/T7 → Final Verification

## Context
### Original Request
- 做出修复所有问题的规划。

### Interview Summary
- User explicitly requested a plan to fix all identified mismatches.
- Source of truth is `docs/plan/API.md`; `misc/MS-VHDX.md` is supporting context.
- `docs/plan/API.md` is explicitly read-only and must not be modified.

### Metis Review (gaps addressed)
- Added explicit guardrails to prevent scope creep (no unrelated refactor/debt cleanup).
- Added deterministic acceptance criteria (exact commands + expected outcomes).
- Added explicit failure-path QA for every task.
- Added boundary that extra exports are out of scope unless required by parity.

## Work Objectives
### Core Objective
Resolve all seven blocking plan parity mismatches in code and tests so implementation behavior and public API surface match `docs/plan/API.md` requirements.

### Deliverables
- Code updates in:
  - `src/file.rs`
  - `src/validation.rs`
  - `src/sections/log.rs`
  - `src/sections/bat.rs`
  - `src/lib.rs`
- Test updates/additions in existing test modules and `tests/api_surface_smoke.rs`.
- Raw evidence files for every task and final verification in `.sisyphus/evidence/`.

### Definition of Done (verifiable conditions with commands)
- `cargo test -p vhdx-rs --test api_surface_smoke` passes with parity assertions.
- Targeted unit/integration tests for strict, parent locator/chain, log entry, BAT sector-bitmap, and StandardItems paths pass.
- `cargo test --workspace` passes.
- `cargo build -p vhdx-tool` passes.
- `cargo clippy --workspace` runs without introducing blocking errors.

### Must Have
- Strict mode changes from no-op to enforced behavior.
- Parent locator validation includes `parent_linkage2` handling policy per plan.
- Parent chain validation checks linkage consistency against parent metadata GUID semantics.
- `Log::entry(index)` returns parsed entry when index exists.
- `InMemoryOnReadOnly` is not equivalent to no replay.
- BAT state parsing path can produce `BatState::SectorBitmap(...)` where applicable.
- Public `StandardItems` namespace exposed as plan-compatible path.

### Must NOT Have (guardrails, AI slop patterns, scope boundaries)
- MUST NOT modify `docs/plan/API.md`.
- MUST NOT modify `misc/`.
- MUST NOT introduce unrelated API redesign or broad refactoring.
- MUST NOT add dependencies.
- MUST NOT alter non-parity business behavior outside scoped mismatches.

## Verification Strategy
> ZERO HUMAN INTERVENTION - all verification is agent-executed.
- Test decision: tests-after + existing Rust test framework (`cargo test`)
- QA policy: Every task includes one happy-path + one failure/edge scenario.
- Evidence: `.sisyphus/evidence/task-{N}-{slug}.{ext}`

## Execution Strategy
### Parallel Execution Waves
> Target: 5-8 tasks per wave. <3 per wave (except final) = under-splitting.
> Shared dependencies extracted to Wave 1.

Wave 1: strict/validation foundation
- T1 strict enforcement wiring
- T2 parent locator + parent chain validation semantics

Wave 2: data/log semantics
- T3 implement `Log::entry(index)` behavior
- T4 align `InMemoryOnReadOnly` replay semantics
- T5 BAT sector-bitmap parse path

Wave 3: API surface + regression closure
- T6 expose `StandardItems` namespace path
- T7 parity regression tests/evidence closure

### Dependency Matrix (full, all tasks)
- T1: blocked by none; blocks T7
- T2: blocked by none; blocks T7
- T3: blocked by none; blocks T7
- T4: blocked by T3 (replay interaction); blocks T7
- T5: blocked by none; blocks T7
- T6: blocked by none; blocks T7
- T7: blocked by T1,T2,T3,T4,T5,T6
- F1-F4: blocked by T1-T7 complete

### Agent Dispatch Summary (wave → task count → categories)
- Wave 1 → 2 tasks → deep/unspecified-high
- Wave 2 → 3 tasks → deep/unspecified-high
- Wave 3 → 2 tasks → quick/unspecified-high
- Final verification → 4 tasks in parallel → oracle + unspecified-high + deep

## TODOs
> Implementation + Test = ONE task. Never separate.
> EVERY task MUST have: Agent Profile + Parallelization + QA Scenarios.

- [ ] 1. Enforce strict-mode semantics (remove no-op behavior)

  **What to do**:
  - Implement strict behavior path in open flow so `strict=true` enforces required-unknown failure contract.
  - Ensure strict flag propagates into the actual validation/parse decision path.
  - Preserve current default (`strict=true`) and builder API shape.

  **Must NOT do**:
  - Do not change `OpenOptions` public method names/signatures.
  - Do not relax existing stricter checks.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: semantic behavior change across open/validation path.
  - Skills: `[]` - no external skill required.
  - Omitted: `review-work` - not needed during implementation task.

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [7] | Blocked By: []

  **References**:
  - Pattern: `docs/plan/API.md:335-336,449-461` - strict contract.
  - Pattern: `src/file.rs:325-333,845-863` - strict option wiring.
  - Pattern: `src/validation.rs:41-127` - validation entry points.
  - Test: `tests/api_surface_smoke.rs` - API surface assertions.

  **Acceptance Criteria**:
  - [ ] `strict=true` path fails on required unknown condition in automated test.
  - [ ] `strict=false` path allows opening same crafted fixture in automated test.
  - [ ] `cargo test -p vhdx-rs <targeted strict test filter>` returns exit code 0.

  **QA Scenarios**:
  ```
  Scenario: strict=true rejects required-unknown
    Tool: Bash
    Steps: run targeted test creating/using crafted metadata/region fixture then open with strict=true
    Expected: test asserts Error variant for required unknown and passes
    Evidence: .sisyphus/evidence/task-1-strict-enforcement.txt

  Scenario: strict=false allows same fixture
    Tool: Bash
    Steps: run targeted test opening same fixture with strict=false
    Expected: open succeeds; structural reads work
    Evidence: .sisyphus/evidence/task-1-strict-enforcement-error.txt
  ```

  **Commit**: YES | Message: `fix(file): enforce strict unknown-required handling` | Files: `src/file.rs`, tests

- [ ] 2. Align parent locator and parent chain validation semantics

  **What to do**:
  - Extend `validate_parent_locator` to include `parent_linkage2` handling policy per plan.
  - Implement parent chain linkage validation against parent metadata `DataWriteGuid` semantics.
  - Populate `ParentChainInfo.child` with real child path context.

  **Must NOT do**:
  - Do not degrade existing parent path resolution order.
  - Do not change `ParentChainInfo` public fields.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: cross-file validation semantics + diff-disk rules.
  - Skills: `[]` - no extra skills.
  - Omitted: `artistry` - conventional validation problem.

  **Parallelization**: Can Parallel: YES | Wave 1 | Blocks: [7] | Blocked By: []

  **References**:
  - Pattern: `docs/plan/API.md:466-470,472-475` - parent locator/chain contract.
  - Pattern: `src/validation.rs:129-227` - current minimal implementation.
  - Pattern: `src/sections/metadata.rs:539-557` - parent path resolution sequence.
  - External: `misc/MS-VHDX.md §2.6.2.6` - parent locator normative context.

  **Acceptance Criteria**:
  - [ ] Parent locator validation covers `parent_linkage2` policy with tests.
  - [ ] Parent chain validation checks linkage consistency against parent metadata GUID with tests.
  - [ ] `ParentChainInfo.child` is non-empty real child path in success test.

  **QA Scenarios**:
  ```
  Scenario: valid differencing chain passes
    Tool: Bash
    Steps: run targeted validation test with parent+child fixtures and matching linkage
    Expected: validate_parent_chain returns linkage_matched=true and non-empty child/parent paths
    Evidence: .sisyphus/evidence/task-2-parent-validation.txt

  Scenario: mismatch linkage fails
    Tool: Bash
    Steps: run targeted validation test with mismatched parent linkage metadata
    Expected: deterministic Error variant/assertion path
    Evidence: .sisyphus/evidence/task-2-parent-validation-error.txt
  ```

  **Commit**: YES | Message: `fix(validation): implement parent linkage2 and chain checks` | Files: `src/validation.rs`, tests

- [ ] 3. Implement `Log::entry(index)` indexed access

  **What to do**:
  - Implement deterministic indexed retrieval based on parsed entries order.
  - Keep behavior consistent with `entries()` parsing semantics.

  **Must NOT do**:
  - Do not change log replay algorithm semantics while implementing accessor.
  - Do not alter public signatures.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: focused parsing/API behavior implementation.
  - Skills: `[]`.
  - Omitted: `deep` - not required beyond module-local logic.

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [4,7] | Blocked By: []

  **References**:
  - Pattern: `docs/plan/API.md:856-861` - indexed entry contract.
  - Pattern: `src/sections/log.rs:73-78,85-106` - current entry/entries behavior.

  **Acceptance Criteria**:
  - [ ] `entry(i)` returns same element as `entries().get(i)` for valid indices.
  - [ ] `entry(out_of_range)` returns `None`.
  - [ ] Targeted log tests pass.

  **QA Scenarios**:
  ```
  Scenario: valid index returns parsed entry
    Tool: Bash
    Steps: run targeted log module test constructing log bytes with >=2 entries
    Expected: entry(1).is_some and equals entries()[1] semantic checks
    Evidence: .sisyphus/evidence/task-3-log-entry.txt

  Scenario: out-of-range returns None
    Tool: Bash
    Steps: run targeted test querying large index
    Expected: None without panic
    Evidence: .sisyphus/evidence/task-3-log-entry-error.txt
  ```

  **Commit**: YES | Message: `fix(log): implement indexed entry accessor` | Files: `src/sections/log.rs`, tests

- [ ] 4. Align `InMemoryOnReadOnly` semantics with plan

  **What to do**:
  - Implement read-only in-memory replay behavior distinct from no replay.
  - Ensure it preserves structure-read guarantees and does not write back to disk in read-only mode.

  **Must NOT do**:
  - Do not merge behavior with `ReadOnlyNoReplay`.
  - Do not require writable handle for this policy.

  **Recommended Agent Profile**:
  - Category: `deep` - Reason: subtle policy semantics with replay path constraints.
  - Skills: `[]`.
  - Omitted: `quick` - high risk of subtle regression.

  **Parallelization**: Can Parallel: NO | Wave 2 | Blocks: [7] | Blocked By: [3]

  **References**:
  - Pattern: `docs/plan/API.md:353-365` - replay policy semantics.
  - Pattern: `src/file.rs:91-103,494-530` - current policy handling.

  **Acceptance Criteria**:
  - [ ] Read-only + `InMemoryOnReadOnly` executes replay path logic without write-back.
  - [ ] Read-only + `ReadOnlyNoReplay` remains no-replay behavior.
  - [ ] Policy behavior differences covered by tests.

  **QA Scenarios**:
  ```
  Scenario: in-memory replay on read-only
    Tool: Bash
    Steps: run targeted test opening fixture with pending logs under InMemoryOnReadOnly
    Expected: replay-dependent reads succeed; no disk mutation assertion in test
    Evidence: .sisyphus/evidence/task-4-inmemory-replay.txt

  Scenario: read-only no replay remains no replay
    Tool: Bash
    Steps: run paired targeted test using ReadOnlyNoReplay on same fixture
    Expected: behavior differs from in-memory replay path per assertions
    Evidence: .sisyphus/evidence/task-4-inmemory-replay-error.txt
  ```

  **Commit**: YES | Message: `fix(file): implement readonly in-memory replay policy` | Files: `src/file.rs`, tests

- [ ] 5. Implement BAT sector-bitmap parse path

  **What to do**:
  - Implement parsing logic/path that can emit `BatState::SectorBitmap(SectorBitmapState)` where applicable.
  - Keep payload state decoding correctness and invalid state rejection.

  **Must NOT do**:
  - Do not weaken invalid-state handling for reserved values.
  - Do not alter BAT raw encoding layout.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: module-focused but requires careful semantic mapping.
  - Skills: `[]`.
  - Omitted: `deep` - enough with focused implementation.

  **Parallelization**: Can Parallel: YES | Wave 2 | Blocks: [7] | Blocked By: []

  **References**:
  - Pattern: `docs/plan/API.md:625-648` - BatState/Payload/SectorBitmap intent.
  - Pattern: `src/sections/bat.rs:199-220,296-324` - current enums + parse path.
  - External: `misc/MS-VHDX.md §2.5.1` - state semantics context.

  **Acceptance Criteria**:
  - [ ] Tests prove parser can construct `BatState::SectorBitmap(...)` in intended context.
  - [ ] Existing payload parsing tests remain green.
  - [ ] Reserved invalid states still error.

  **QA Scenarios**:
  ```
  Scenario: sector-bitmap state is produced
    Tool: Bash
    Steps: run targeted BAT tests with crafted entry/context expected as sector-bitmap
    Expected: matches BatState::SectorBitmap(Present/NotPresent) assertions
    Evidence: .sisyphus/evidence/task-5-bat-sector-bitmap.txt

  Scenario: invalid state rejected
    Tool: Bash
    Steps: run targeted BAT test with reserved/invalid state bits
    Expected: Error::InvalidBlockState assertion
    Evidence: .sisyphus/evidence/task-5-bat-sector-bitmap-error.txt
  ```

  **Commit**: YES | Message: `fix(bat): add sector bitmap state parsing path` | Files: `src/sections/bat.rs`, tests

- [ ] 6. Expose `StandardItems` namespace path compatible with plan

  **What to do**:
  - Add plan-compatible public namespace path for standard metadata GUIDs as `StandardItems`.
  - Preserve existing constants access paths for backward compatibility.

  **Must NOT do**:
  - Do not remove or rename existing `constants` exports.
  - Do not modify GUID values.

  **Recommended Agent Profile**:
  - Category: `quick` - Reason: focused export surface adjustment.
  - Skills: `[]`.
  - Omitted: `deep` - unnecessary for namespace wiring.

  **Parallelization**: Can Parallel: YES | Wave 3 | Blocks: [7] | Blocked By: []

  **References**:
  - Pattern: `docs/plan/API.md:808-845` - required `StandardItems` namespace.
  - Pattern: `src/lib.rs:49-60,77-79` - current public module exports.
  - Pattern: `src/common/constants.rs` - canonical GUID constants.

  **Acceptance Criteria**:
  - [ ] `vhdx_rs::section::StandardItems::*` (or agreed plan-compatible path) compiles in smoke test.
  - [ ] Existing `constants::` path remains available.

  **QA Scenarios**:
  ```
  Scenario: StandardItems path import works
    Tool: Bash
    Steps: run api surface smoke test including StandardItems imports
    Expected: compilation and test pass
    Evidence: .sisyphus/evidence/task-6-standard-items.txt

  Scenario: legacy constants path still works
    Tool: Bash
    Steps: run paired smoke assertion using previous constants path
    Expected: no breakage/regression
    Evidence: .sisyphus/evidence/task-6-standard-items-error.txt
  ```

  **Commit**: YES | Message: `feat(api): expose StandardItems metadata GUID namespace` | Files: `src/lib.rs`, tests

- [ ] 7. Parity regression closure and evidence packaging

  **What to do**:
  - Add/adjust targeted tests to cover all seven repaired mismatches.
  - Run parity command set and capture raw outputs.
  - Ensure no accidental edits outside scope.

  **Must NOT do**:
  - Do not modify `docs/plan/API.md`.
  - Do not use summary-only evidence files.

  **Recommended Agent Profile**:
  - Category: `unspecified-high` - Reason: cross-cutting verification orchestration.
  - Skills: `[]`.
  - Omitted: `writing` - technical verification first.

  **Parallelization**: Can Parallel: NO | Wave 3 | Blocks: [F1,F2,F3,F4] | Blocked By: [1,2,3,4,5,6]

  **References**:
  - Pattern: `.sisyphus/plans/api-md-code-parity.md` evidence quality lessons.
  - Test: `tests/api_surface_smoke.rs` and module tests in touched files.

  **Acceptance Criteria**:
  - [ ] `cargo test -p vhdx-rs --test api_surface_smoke` passes.
  - [ ] `cargo test --workspace` passes.
  - [ ] `cargo build -p vhdx-tool` passes.
  - [ ] Evidence files for T1-T7 include raw terminal output.

  **QA Scenarios**:
  ```
  Scenario: full regression succeeds
    Tool: Bash
    Steps: run smoke, workspace tests, and CLI build in sequence
    Expected: all exit code 0
    Evidence: .sisyphus/evidence/task-7-parity-regression.txt

  Scenario: evidence quality gate catches malformed/empty logs
    Tool: Bash
    Steps: run evidence sanity script/check (size/content markers) over T1-T7 files
    Expected: binary pass with all files non-empty and command traces present
    Evidence: .sisyphus/evidence/task-7-parity-regression-error.txt
  ```

  **Commit**: YES | Message: `test(parity): close API plan parity regression coverage` | Files: tests + `.sisyphus/evidence/*`

## Final Verification Wave (MANDATORY — after ALL implementation tasks)
> 4 review agents run in PARALLEL. ALL must APPROVE. Present consolidated results to user and get explicit "okay" before completing.
> **Do NOT auto-proceed after verification. Wait for user's explicit approval before marking work complete.**
> **Never mark F1-F4 as checked before getting user's okay.** Rejection or user feedback -> fix -> re-run -> present again -> wait for okay.
- [ ] F1. Plan Compliance Audit — oracle
- [ ] F2. Code Quality Review — unspecified-high
- [ ] F3. Real Manual QA — unspecified-high (+ playwright if UI)
- [ ] F4. Scope Fidelity Check — deep

## Commit Strategy
- Small atomic commits per task (T1..T7), each with matching tests.
- No squash until final user-approved completion.
- Commit message convention: `fix(scope): ...`, `feat(api): ...`, `test(parity): ...`.

## Success Criteria
- All 7 blocking mismatches resolved and validated by automated tests.
- No forbidden file modifications (`docs/plan/API.md`, `misc/`).
- Final verification wave F1-F4 all APPROVE.
- User explicitly approves final verification outcome before closure.
