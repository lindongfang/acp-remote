# Plan

## Execution Model

- 派发：主 Agent 用 `task` 的 batch 形状一次性扇出独立工作包；每个工作包是一次 spawn。
- 隔离：`.omp/config.yml` 的 `task.isolation`（`merge: patch`、`apply: false`）；工作包不共享目录。
- 交付：`<id>.patch` + `agent://<id>` + `history://<id>`；不产生 commit、不产生分支。
- 合入：主 Agent 按 `## Merge Order` 串行 `git apply --check` → `git apply` → 主检出 Project Verify → commit。
- 验证：隔离工作区没有 `target/` 与 `node_modules/`，仓库级检查只在主检出执行。
- 检视：每个工作包由独立 spawn 的 `reviewer` 读 patch 与 `agent://<id>` 后给出结构化结论。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | lookup 的 wire DTO / schema / fixtures；新增 `pairing.code_invalid`；`SYNC_PROTOCOL.md` 写清两种入口、单次消耗与独立限流 | none | wp-coder | `schemas/sync/v1/`, `fixtures/sync/v1/`, `crates/sync-protocol/`, `compatibility/errors/v1/errors.json`, `docs/SYNC_PROTOCOL.md` | patch + 协议 crate 往返/负例测试 + 错误词表 | `npm run check:schemas`；`npm run check:errors`；`cargo test -p sync-protocol` |
| WP2 | 配对码派生（域分离、40 bit、Crockford）、内存反查、单次消耗、碰撞去重、secret 清除同步；补过期清理 API | none | wp-coder | `crates/identity-auth/` | patch + 派生固定向量 + 单次/碰撞/生命周期测试 | `cargo test -p identity-auth` |
| WP3 | 接线 lookup 端点；`device.pair.begin` 返回 `pairingCode`；CLI 打印 URL+码；daemon 过期维护接到 `due_pairings`/`reset_memory` | `code:WP1`, `code:WP2` | wp-coder | `crates/server/src/sync/pairing.rs`, `crates/server/src/local_admin/`, `crates/server/src/transport/`, `crates/app/src/`, `crates/server/tests/`, `crates/app/tests/` | patch + 端点/管理面/CLI/清理接线测试 | `cargo test -p server -p app` |
| WP4 | `/pair` 无 fragment 时按原型 S0–S12 显示配对码输入（S0 不显示数字倒计时）；lookup 后复用 claim/status；固定失败文案；lookup 后按服务端 `expiresAt` 启动倒计时 | `contract:WP1` | wp-coder | `clients/app/` | patch + 模型单测 + 页面 e2e | `cd clients/app && npm run typecheck && npm run test` |
| WP5 | 安全/前端/管理/身份合同文档写回；新增 ADR | `contract:WP1` | wp-coder | `docs/SECURITY_DESIGN.md`, `docs/FRONTEND_DESIGN.md`, `docs/LOCAL_ADMIN_PROTOCOL.md`, `docs/IDENTITY_AND_AUTH_CONTRACT.md`, `docs/adr/0010-web-pairing-code-entry.md` | patch + ADR + 文档一致性检查 | `npm run check:docs`；`npm run check:openspec` |

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 无重叠；先落 wire、错误词表与协议文档 | `npm run check:schemas`；`npm run check:errors`；`cargo test -p sync-protocol` |
| 2 | WP2 | 与 WP1 无文件重叠；提供 WP3 的查找/清理 API | `cargo test -p identity-auth` |
| 3 | WP3 | 依赖 WP1 的 DTO/错误码与 WP2 的 API | `cargo test -p server -p app` |
| 4 | WP4 | 依赖 WP1 的 lookup 形状；只改前端工程 | `cd clients/app && npm run typecheck && npm run test` |
| 5 | WP5 | 文档写回需以已合入行为为准 | `npm run check:docs`；`npm run check:openspec` |

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm run verify`（仓库根） | `npm run check` + `cargo fmt`/`clippy`/`cargo test` 全绿 |
| PV2 | `cd clients/app && npm run check` | 前端 typecheck（含 e2e）、vitest、`export:web` 全绿 |
| PV3 | `npm run check:openspec`（仓库根） | 变更产物、spec 增量与 rules 校验通过 |
| PV4 | `cd clients/app && node e2e/run-e2e.mjs` | 真实 Chromium 下二维码路径回归 + 配对码入口 S0–S12 通过 |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | schema 自洽、fixture 有效/无效边界、Rust DTO 往返、错误码词表、文档与 wire 一致 | 无 blocker/major 未闭环 |
| WP2 | reviewer | 域分离、40 bit 无取模偏差、Crockford 归一化、常数时间比较、单次消耗原子性、碰撞去重、secret 生命周期同步、无持久化 | 无 blocker/major 未闭环 |
| WP3 | reviewer | lookup 原子性、统一 401、独立限流、Origin/Host 校验、日志脱敏、CLI 输出、过期清理接线不回归 | 无 blocker/major 未闭环 |
| WP4 | reviewer | S0–S12 状态穷尽、码不进公开状态/URL/存储、S0 无数字倒计时、lookup 后按服务端 `expiresAt` 倒计时、固定失败文案、二维码路径回归 | 无 blocker/major 未闭环 |
| WP5 | reviewer | 文档与实现一致、TOFU 边界写清、单次消耗与“S0 无倒计时、lookup 后按服务端时间”写清、链接有效 | 无 blocker/major 未闭环 |

## Main E2E

```yaml
mode: required
reason: ""
basis: ""
alternative_checks: []
```

- Web/PWA：`cd clients/app && node e2e/run-e2e.mjs`，断言 `/pair` 无 fragment 显示 S0（8 格、origin、HTTPS、无数字倒计时、静态有效期提示）、错误码呈现「配对码不对或已过期，请重新生成配对码」、正确码经 S1→S2→S3→S4 进入目录、二维码 fragment 路径不回归。
- 真实 Daemon 冒烟：按 `docs/LOCAL_DEVICE_PAIRING.md` 启动 Daemon 与 `device pair`，在浏览器 `/pair` 输入终端配对码，终端仍提示 `y/N`，确认后页面进入 `online`。该手册当前是未跟踪文件，不作为本次 patch 的写范围；`prototypes/acp-remote-pwa.html` 同样已是用户修改过的在途文件，本次只参考、不修改。

## Risks

- 主检出存在未跟踪的 `docs/LOCAL_DEVICE_PAIRING.md` 与已修改的 `prototypes/*`；隔离工作区会把它们当作基线。本变更不修改这两处，避免把在途内容混入 patch。
- WP3 的过期清理接线触及 daemon 维护路径；合入后必须重跑配对生命周期测试，确认不会把仍有效的配对提前清掉。
- WP4 的 e2e 依赖前端 `node_modules`；主检出已有依赖，隔离区不跑。
- 真实 Daemon 冒烟依赖本机 TLS 证书与端口；若环境缺失，以浏览器 e2e + 单元/集成测试为替代证据，并在合入记录中说明。
