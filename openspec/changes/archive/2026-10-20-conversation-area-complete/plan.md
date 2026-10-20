# Execution Plan

## Execution Model

- 主 Agent 使用 omp `task` batch，共享 context + tasks[]；无依赖且写范围不冲突的包同批派发，每包一次 spawn。
- 所有写入包 `isolated: true`，沿用 `task.isolation` patch 模式及 `apply: false`，不共享目录，不自行 commit/建分支。
- 交付 `<id>.patch` + `agent://<id>` + `history://<id>`，主 Agent 串行 `git apply --check` 后 `git apply`，只应用最终检视通过版。
- 仓库级检查仅主检出运行，隔离工作区不要求 target/node_modules。reviewer 为内置角色且独立于作者 spawn，提供 patch 绝对路径及两个报告引用。
- 当前阶段仅规划；实施前确认现有 session-file-browser 等在途变更基线，不能覆盖用户修改。合同先落地，但未实现的能力继续标计划，不能提前宣告支持。

## Work Packages

所有包交付均为 patch + 报告 + 过程；报告包含规格场景映射、未执行检查和残余缺口。

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 冻结新 feature/附件帧/媒体/有效交互/远程历史、端口与保留合同 | none | wp-coder | `schemas/`, `fixtures/`, `compatibility/`, `docs/`, `openspec/config.yaml`, `scripts/check-contract-drift.mjs`, `scripts/check-contract-assets.mjs`, `scripts/check-acp-compatibility.mjs` | 合同、共享正负 fixture、权威文档同步与状态画像修正 | PV1 |
| WP2 | 附件分块、消息/思考/部分输出、有效交互与原子绑定/清理 | contract:WP1 | wp-coder | `crates/core/`, `crates/storage-sqlite/`, `docs/CORE_PORTS_AND_STORAGE.md`, `docs/MODULE_ARCHITECTURE.md`, `scripts/check-contract-drift.mjs` | 端口与实现、版本化 migration、旧库/故障/竞争测试 | PV1、PV2 |
| WP3 | ACP 媒体映射、真实终端与表单能力，不越过 Agent 预算 | code:WP2 | wp-coder | `crates/acp-protocol/`, `crates/agent-host/`, `docs/ACP_COMPATIBILITY_MATRIX.md`, `docs/MODULE_ARCHITECTURE.md`, `docs/SECURITY_DESIGN.md` | 真实子进程服务、原文保真与 fake ACP 测试、Windows 清理探针 | PV1、PV3 |
| WP4 | Sync/Node Link 编解码、独立认证附件连接、媒体/待处理/历史查询 | code:WP2, contract:WP1 | wp-coder | `crates/sync-protocol/`, `crates/node-link-protocol/`, `crates/server/`, `docs/SYNC_PROTOCOL.md`, `docs/NODE_LINK_PROTOCOL.md`, `docs/SECURITY_DESIGN.md` | 协商兼容、路由、撤销、限流与传输故障测试 | PV1、PV4 |
| WP5 | Access 出站与 app 装配，本地/远程真实能力启用 | code:WP3, code:WP4 | wp-coder | `crates/node-link-client/`, `crates/app/`, `crates/server/`, `Cargo.toml`, `Cargo.lock`, `docs/MODULE_ARCHITECTURE.md`, `docs/NODE_LINK_PROTOCOL.md`, `docs/LOCAL_ADMIN_PROTOCOL.md`, `docs/CONFIG_REFERENCE.md`, `docs/INITIAL_DESIGN.md`, `docs/SECURITY_DESIGN.md`, `README.md`, `schemas/local-admin/`, `fixtures/local-admin/`, `compatibility/commands/` | backend、进程/连接所有权、import 接线、无正文磁盘测试 | PV1、PV5 |
| WP6 | 全部对话内容、真实流式、附件/表单/草稿/阅读体验 | contract:WP1 | wp-coder | `clients/app/src/`, `clients/app/app/`, `clients/app/e2e/`, `clients/app/package.json`, `clients/app/package-lock.json`, `docs/FRONTEND_DESIGN.md`, `prototypes/IMPLEMENTATION-GAPS.md` | 平台端口与分层状态、组件、针对性单测及浏览器断言 | PV1、PV6 |
| WP7 | 单/双节点与真实 Agent 联合验收和实际证据 | code:WP2, code:WP3, code:WP4, code:WP5, code:WP6 | wp-tester | `clients/app/e2e/`, `crates/app/tests/`, `crates/agent-host/tests/`, `crates/server/tests/`, `crates/node-link-client/tests/`, `docs/evidence/conversation-area-complete.md`, `openspec/changes/conversation-area-complete/tasks.md`, `README.md` | 跨层回归、实际 E2E 证据；不代替其他包自身测试 | PV7、Main E2E |

## Merge Order

派发可在 WP1 后并行 WP2 与 WP6；WP2 后 WP3 与 WP4 实现代码互不重叠，但同写安全/模块文档，须依据已合入前序包的基线或显式重放后重新检视。WP5 等待 WP3/4，WP7 等待所有实现。主检出合入顺序固定如下，覆盖所有 scope 重叠。

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 合同基线 | PV1 |
| 2 | WP2 | 与 WP1 文档及漂移脚本重叠，后包重跑 | PV1、PV2 |
| 3 | WP3 | 与 WP1/2 权威文档重叠，后包重跑 | PV1、PV2、PV3 |
| 4 | WP4 | 与 WP1/3 协议及安全文档重叠，后包重跑 | PV1、PV3、PV4 |
| 5 | WP5 | 与 WP1/3/4 文档及 WP4 server 重叠，后包重跑 | PV1、PV3、PV4、PV5 |
| 6 | WP6 | 与 WP1 前端文档重叠，后包重跑；之前可按合同独立实现 | PV1、PV6 |
| 7 | WP7 | 与 WP3/4/5/6 测试及 README 重叠，最终重跑全部 | PV7、Main E2E |

## Project Verify

以下命令都在主检出执行。源码包名在实施新增成员时以 Cargo.toml 为准，当前已有包名沿用仓库；新包登记为 node-link-client。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根目录 `npm run check` | schema/fixture/词表、ACP、链接、边界、合同漂移、OpenSpec 全绿 |
| PV2 | 根目录 `cargo test --locked -p core -p storage-sqlite` | 旧库升级、分块/摘要、部分正文、交互竞争、事务故障与清理竞态通过 |
| PV3 | 根目录 `cargo test --locked -p acp-protocol -p agent-host` | 类型/原文保真，真实子进程输出、退出/kill/release、Job Object 证据 |
| PV4 | 根目录 `cargo test --locked -p sync-protocol -p node-link-protocol -p server` | 旧/新协商矩阵、跨会话拒绝、传输取消/磁盘失败、真实接入通过 |
| PV5 | 根目录 `cargo test --locked -p node-link-client -p app` | 双节点重放/撤销/uncertain，Access 磁盘无正文与启动关闭通过 |
| PV6 | `clients/app` 内 `npm run check` | 类型、单测、E2E 类型和 Web 构建通过；页面行为另由 Main E2E 验证 |
| PV7 | 根目录 `npm run verify`，随后 `clients/app` 内 `npm run check` | 所有实现合入后合同、fmt/clippy/工作区测试及前端全绿 |

## Code Review

每包独立 reviewer spawn；输入为 patch 绝对路径 + agent:// + history://，结构化 verdict=pass/changes-requested/blocked。修复和复核各用新 spawn，最多 3 轮后重拆。表中均须无未闭环 CRITICAL/MAJOR 才 pass。

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer | 用户修订追溯、旧 peer 兼容、schema/词表/端口一致性 | 无未闭环 CRITICAL/MAJOR |
| WP2 | reviewer | 原子性、无整文件内存读取、旧库迁移、交互有效性、清理竞态 | 无未闭环 CRITICAL/MAJOR |
| WP3 | reviewer | ACP 原生语义、预算、真实输出与任务树清理 | 无未闭环 CRITICAL/MAJOR |
| WP4 | reviewer | 认证/撤销先行、隔离传输、原文与序号保真、旧 feature 隔离 | 无未闭环 CRITICAL/MAJOR |
| WP5 | reviewer | 出站所有权、无正文落盘、稳定 requestId、组合根依赖 | 无未闭环 CRITICAL/MAJOR |
| WP6 | reviewer | 分层、流式去重/替换、表单精确值、草稿及滚动、媒体安全 | 无未闭环 CRITICAL/MAJOR |
| WP7 | reviewer | 真实与模拟证据区分，需求全覆盖，未执行项不伪装通过 | 无未闭环 CRITICAL/MAJOR |

## Main E2E

```yaml
mode: required
```

使用 omp browser prelude，Windows 进程树观察需要时使用 computer prelude 与 `.omp/e2e.yml`。所有服务使用隔离数据目录与动态端口；正文/密钥不进入日志或截图证据。启动成功或截图本身不算通过。

| Case | Environment | Observable Pass Criteria |
| --- | --- | --- |
| E1 | 浏览器 + 真实 Owner Daemon + 可控 ACP 子进程 | 文本、思考、工具、计划、终端、文件改动、媒体按真实事件显示，刷新后恢复 |
| E2 | 同上，逐块发送未闭合 Markdown，主动中断 WSS | 流式原位更新，补回无重复，历史与最终一致；停止/失败保留部分正文 |
| E3 | 同上，两个浏览器设备 | 审批/真实表单首个有效回答胜出，刷新重新取得请求可答；无效输入不消费请求 |
| E4 | 同上，文件/图片/已有音频，失败注入 | 点击发送才上传、部分失败不执行、取消不执行、Agent 不支持不静默删附件、闲置清理不误删引用 |
| E5 | 两个真实 Daemon + 浏览器 | import/远程创建与读取、命令、交互、附件、重连重放闭环；Owner 离线显式不可用；Access 数据目录无远程正文 |
| E6 | 浏览器长输出/上读/文件查看/主题与窄屏 | 不强制滚底，展开与媒体加载保持位置；执行中能编辑不能发送，文件查看往返保留草稿 |
| E7 | 真实 Codex 与 Oh My Pi，各自实际 profile | 文本/工具/停止/历史真实通过；逐项记录初始化能力及媒体/终端/表单支持或明确不支持，不能凭假 Agent 代替 |
| E8 | Windows 真实终端任务与子进程树 | kill/release、Agent 崩溃及 Daemon 关闭不留孤儿进程，release 后工具历史仍可读 |

## Risks

- 现有在途变更有未完成 E2E：实施前记录基线，不擅自勾选或覆盖它们。
- omp/browser/真实 Agent 任一环境缺失：保留对应任务未完成并报告具体证据缺口，不能把无关检查当替代验收。
- 大范围工作包：按本文依赖分批交付；若单包需重拆，只细化任务与写范围，不缩减用户确认范围。
- 依赖许可证/advisory/密钥扫描仍以 CI 为准，本地不宣称等价通过；新增依赖先检查必要性、平台与许可证。
