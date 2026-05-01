# Task 4 Guardrail Evidence

## Scope Lock

本文件用于固化 T4 执行 guardrail，作为后续 T5+ 的可追溯约束基线。

### Hard Constraints

1. 只改访问模式，不改业务逻辑。
2. 不改枚举语义。
3. 不改磁盘格式计算流程。
4. 不改 `misc/`、`Cargo.toml` 依赖、`rustfmt.toml`。

### Execution Rule

- 当前任务仅允许产出 guardrail 与白名单证据文件，不做源码行为修改。
- 每次后续任务执行前，应先用 `git diff --name-only` 对照白名单做越界检查。
- 若出现白名单外文件，直接判定本轮为越界，需先收敛范围再继续。

## Traceability

- Plan reference: `.sisyphus/plans/sync-code-for-accessor-only-api.md` Task 4
- Companion evidence: `.sisyphus/evidence/task-4-file-whitelist.txt`

## Result

- Guardrail 条目完整性: **PASS**
- 可追溯性: **PASS**
