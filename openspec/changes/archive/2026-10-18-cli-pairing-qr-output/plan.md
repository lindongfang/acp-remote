# Plan

## Execution Model

- 本变更只有一个工作包；由主 Agent 在独立分支上直接实现并按 `docs/WORKFLOW.md` 合入。
- 验证只在主检出执行；隔离工作区没有 `target/` 与 `node_modules/`。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | `device pair` 输出裸 `/pair`、配对码、终端二维码与指定提示；`node pair` 不变 | none | wp-coder | `crates/app/src/cli/pairing.rs`, `crates/app/Cargo.toml`, `Cargo.lock`, `crates/app/tests/cli_commands.rs`, `docs/LOCAL_ADMIN_PROTOCOL.md`, `openspec/changes/cli-pairing-qr-output/` | patch + 测试 + 文档 | `cargo test -p app --test cli_commands`；`cargo clippy -p app --all-targets`；`cargo fmt --check`；`npm run check:docs`；`npm run check:openspec` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 唯一工作包 | `cargo test -p app --test cli_commands`；`npm run check:docs`；`npm run check:openspec` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `cargo test -p app --test cli_commands` | 设备输出包含裸地址/配对码/提示且不含 `#data=`；node 输出不变 |
| PV2 | `cargo clippy -p app --all-targets -- -D warnings` | 无警告 |
| PV3 | `npm run check:docs`；`npm run check:openspec` | 文档与变更产物校验通过 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | QR 内容、裸地址截断、node 输出不变、错误不泄密、依赖边界 | 无 blocker/major 未闭环 |

## Main E2E

```yaml
mode: not-applicable
reason: 变更只影响 CLI 终端文本与二维码渲染；没有浏览器/DOM 行为。
basis: 集成测试直接断言 `device pair` 的输出文本与 node 分支不变。
alternative_checks:
  - cargo test -p app --test cli_commands
  - manual: 在支持 Unicode 的终端运行 device pair，用手机扫码确认打开完整配对地址
```

## Risks

- `qrcode` 依赖需要 CI 的 deps 检查；许可证为 MIT/Apache-2.0，纯 Rust。
- 窄终端下二维码会折行；裸地址与配对码仍可手输。
