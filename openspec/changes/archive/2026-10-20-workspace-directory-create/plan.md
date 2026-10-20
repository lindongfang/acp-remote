# Execution Plan

## Execution Model

- 用户已确认新增协议与 ADR 变更，开始实现。
- 当前宿主未提供 Oh My Pi `task`，使用独立快照目录和子 Agent 产出补丁，不自动应用。
- 每包交付隔离补丁与当前会话的作者/检视消息，宿主没有 `agent://`/`history://` 报告资源；不产生分支或提交。由主 Agent 串行 `git apply --check` 再 `git apply`。
- 隔离区没有 `target/`、`node_modules/`，只读检视与补丁产出在那里完成；表内 Verify 全在主检出执行。
- 每包由不同于作者的 Agent 交叉检视，输入包含补丁绝对路径与作者报告；具体宿主适配与检视结果见末尾执行记录。
- 保存当前用户未提交修改的基线，不重置索引或覆盖本轮已授权代码；实际 diff 按写范围交付。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 命令、feature、schema/fixture 和 ADR；旧客户端兼容 | none | wp-coder | `compatibility/commands/`, `compatibility/features/`, `schemas/sync/`, `fixtures/sync/`, `crates/sync-protocol/`, `crates/node-link-protocol/` 的命令拒绝兼容匹配, `clients/app/src/protocol/`, `docs/`, `openspec/changes/workspace-directory-create/` | patch + 合同资产清单 + ADR 取代范围 | PV1；crate 全量集成在 WP3 后执行 |
| WP2 | 立即建目录、持久受理、终态、幂等和崩溃窗口 | contract:WP1 | wp-coder | `crates/core/`, `crates/storage-sqlite/` | patch + 文件系统/事务顺序与无会话副作用证据 | PV2 + PV1 |
| WP3 | Sync 命令路由、feature 门控、任务所有权、启动恢复、Node Link 拒绝 | contract:WP1, code:WP2 | wp-coder | `crates/server/`, `crates/app/` | patch + wire 到磁盘的测试与恢复记录 | PV3 + PV1 |
| WP4 | 新建与选择分离、连续三级、失败/不确定状态、旧服务端降级 | contract:WP1, code:WP3 | wp-coder | `clients/app/`、`prototypes/`，不修改 WP1 拥有的 `src/protocol/` | patch + 前端/页面回归证据 | PV4 + PV5 |

WP1 独占权威文档与协议资产；其它包仅在报告里提交文档事实，由主 Agent在 WP1 的同一文档写范围内收口。不存在静默重叠写入；如实施必须改变端口签名或 DDL，先修订 design 与 WP1 合同，不在后包私改合同。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 新命令/feature 是其它包的合同依赖；前端协议目录由本包独占 | PV1 |
| 2 | WP2 | 依赖 WP1 的用例与持久化约束 | PV1、PV2 |
| 3 | WP3 | 消费 WP1 wire 与 WP2 用例，完成新增枚举的匹配接线 | PV1、PV3 |
| 4 | WP4 | 消费真实服务端；前端协议目录不进入本包写范围 | PV4、PV5 |

每包只合入最终检视通过的补丁。跨包合同修正按原写范围串行修订并重跑消费者检查。最终主检出重新检视实际组合 diff。

## Project Verify

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | `npm.cmd run check` / 仓库根 | 命令/feature、schema/fixture、边界、合同漂移、文档和 OpenSpec 全绿 |
| PV2 | `cargo test -p core`、`cargo test -p storage-sqlite` / 仓库根 | 参数/权限/文件已存在、受理失败零 IO、目录成功不建会话、重试/重启窗口、SQLite 全局终态与日志脱敏 |
| PV3 | `cargo test -p sync-protocol`、`cargo test -p node-link-protocol`、`cargo test -p server`、`cargo test -p app` / 仓库根 | feature 与身份前置、Node Link 拒绝、状态查询/终态与恢复、旧 fixture 不回归 |
| PV4 | `npm.cmd run check --prefix clients/app` / 仓库根 | 前端类型、E2E 类型、单元/组件测试与 Web 构建全部通过 |
| PV5 | `npm.cmd run verify`、`node clients/app/e2e/run-e2e.mjs` / 仓库根 | 仓库总门禁与假主机真实页面回归；不能代替下面的真实 Daemon E2E |

## Code Review

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer（独立 spawn） | 词表唯一性、feature 兼容、schema/fixture 对齐、新 ADR 与路径隐私 | 无 CRITICAL/MAJOR 未闭环项 |
| WP2 | reviewer（独立 spawn） | 落盘先于 IO/广播、无会话事件、幂等/重启窗口、路径校验与文件不覆盖 | 无 CRITICAL/MAJOR 未闭环项 |
| WP3 | reviewer（独立 spawn） | 认证/feature 的拒绝顺序、关闭顺序、恢复 uncertain、Node Link 隔离 | 无 CRITICAL/MAJOR 未闭环项 |
| WP4 | reviewer（独立 spawn） | 单份状态、不假造目录、成功后不关闭、不自动选中、多级和降级/错误呈现 | 无 CRITICAL/MAJOR 未闭环项 |

## Main E2E

```yaml
mode: required
```

由主检出使用 browser prelude 操作真实 Web/PWA，连接真实 Daemon 和可控测试 Agent。使用已核对的隔离临时父目录与独立 Daemon 数据目录；测试只在这些隔离路径下创建与清理目录，不在用户 D 盘根做副作用实验。

- 输入一级名字并点「新建」，断言主机磁盘已有该目录、选择器仍开、无会话/Agent 新增。
- 进入新目录再创建二、三级，分别检查磁盘，再点「选择此文件夹」确认完整层级；创建会话时 Agent 实际 cwd 与选择一致。
- 再新建一个测试目录后取消选择与会话创建，检查目录仍存在。
- 受理未终态时不宣称成功，重复点击不会生成第二次请求；断线/重启故障注入后只接受确认终态或显式 uncertain。
- 同名、父层删除/变文件、无写权限、非法名字、旧 feature、无 Agent/会话副作用与路径不进入错误/事件/日志逐项验收。权限用例用可控拒绝端口覆盖，不为了测试改变用户目录 ACL。
- 任一浏览器环境无法启动或真实 Daemon E2E 未运行，明确记录未验证，不以截图、进程启动或构建通过充当成功。

## Risks

- 工作区已有大量用户修改：派发时显式携带基线和写范围，主 Agent禁止 reset/覆盖。
- 新命令枚举需要消费者匹配同步：WP1 后只做合同检查，WP3 后再要求整组 Rust 编译通过。
- 本环境曾出现浏览器缺失/回归长时间无输出：实施先验证可用浏览器；无法执行时如实报告剩余验证边界，不自动安装或改系统设置。

## 本次执行记录

宿主适配：基线与四个工作包隔离副本保存在临时目录 acpr-mkdir-7hN8og，保留现有未提交修改。子 Agent 在隔离副本产出 wp1.patch 至 wp4.patch，主检出串行应用、运行验证。使用不同作者的 Agent 交叉检视；受宿主线程数限制复用已有 reviewer 实例，不复用作者检视自己的包。

四个工作包已串行合入并通过最终独立静态检视。WP1 由 mkdir_frontend 检视；WP2、WP4 和修正后的 WP3 由 mkdir_contracts 检视。WP3 的受理消息阻塞和 WP4 的字段投影、跨设备终态误归并问题均已修正，最终无未关闭的 major/critical。

验证结果：

- PV1：`npm.cmd run check` 全部通过，含新 fixture、协议词表、文档、边界和端口漂移。
- PV2/PV3/PV5：格式检查与全工作区 Clippy 通过；`cargo test --locked --workspace --all-features` 全部通过。Windows DPAPI 在沙箱中返回 PlatformUnavailable，移至沙箱外运行同一完整测试集后通过；未降低实现或测试要求。
- PV4：前端与 E2E 类型检查、74 个测试文件的 1074 项测试、Web 构建全部通过。
- 假主机真实浏览器：`node clients/app/e2e/run-e2e.mjs` 为 83/83 通过（沙箱外 Edge 临时配置）。
- 真实浏览器/TLS Daemon/磁盘/受控 Agent：`cargo test -p app --test workspace_mkdir_browser -- --ignored --nocapture` 通过。独立临时身份与证书，页面完成配对、进入隔离目录、连续创建 project/src/api、单独确认选择；创建与取消期间 Agent 未启动，取消后目录仍存在；最终创建会话，受控 Agent 按 ACP session/new 收到的 cwd 执行，实际目录等于所选 api。测试为 opt-in，常规环境不强制安装浏览器。
- 故障/兼容场景由 core、SQLite、Sync 集成与前端测试覆盖：受理落盘失败、终态落盘失败、撤销、同名/父层变化、非法输入、旧 feature、重复/冲突请求、跨设备相同编号及启动恢复 uncertain。真实浏览器用例验证正常三级创建、选择和取消，不声称逐项执行浏览器故障注入。

主检出另修正合同提取器，使其能比较带默认实现的 trait 方法签名；默认方法体不属于端口签名，参数名前置下划线不改变合同。SQLite 没有新增表或 migration。

后续交互调整（用户明确要求）：新建成功后自动进入真实新目录，保持最终选择独立。同一次创建最多自动进入一次；父层刷新失败或截断未找到条目时不拼接路径跳转。前端 1074 项测试、类型检查、Web 构建、合同检查及真实浏览器假主机 83/83 回归通过。真实 Daemon 验收脚本也已改为等待自动进入；本次重跑因用户正在使用 target/debug/acp-remote.exe，Cargo 无法覆盖而未运行，未停止该用户服务。上一轮真实 Daemon 验收覆盖建目录能力，本轮页面自动导航由单元和真实浏览器回归验证。
