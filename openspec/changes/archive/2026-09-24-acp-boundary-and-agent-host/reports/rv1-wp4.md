# RV1/RV2 检视报告 · WP4/WP5（会话映射、目录与启动装配）

- Review ID：`RV-WP45-round1`（检视 `64a7582`）、`RV2-agent-host`（复核 `64a7582` → `e6b4dfa`）
- Reviewer：独立 reviewer 子 Agent（新隔离上下文，只读）

## 第一轮结论：FAIL（3 MAJOR）

| ID | 级别 | 发现 | 状态 |
| --- | --- | --- | --- |
| WP45-1 | MAJOR | `session/new` 缺 pinned schema 的必填 `cwd`/`mcpServers`；`set_config` 的 `value` 不匹配 schema 的 anyOf | 已修：`session_new_params`（cwd 必须来自已解析 workspace，补空 `mcpServers`）；`config_value_boolean`/`config_value_id`，文本取值显式拒绝 |
| WP45-2 | MAJOR | `turn.failed`（崩溃路径）无任何用例 | 已修：新增 `turn_failed_is_reported_once_when_the_agent_crashes_mid_turn` |
| WP45-3 | MAJOR | `set_mode`/未宣告能力路径不可达（fake child 恒返回 modes/configOptions） | 已修：fake child 增 `--no-modes`/`--no-config-options`/`--exit-on-config-write`；新增 `undeclared_capability_is_refused_without_sending_anything` |
| WP45-4 | MINOR | 白名单交集只在测试替身里实现 | 已修：`resolve_launch` 增加交集校验（`EnvNotAllowed`）+ 新增用例 |
| WP45-5 | MINOR | 配置文件用例不可证伪 | 记为已记录局限：真实证据是代码里没有配置文件读取路径（日志含相应扫描） |
| WP45-6 | MINOR | 无活动会话的 runtime 不被空闲回收 | 已修：`AgentRuntime` 进程级 `last_activity` |
| WP45-7 | MINOR | 先登记交互后 emit，emit 失败会悬挂 | 已修：映射失败不登记并回错误响应 |
| WP45-8 | MINOR | 文档/计划漂移（`libc`、不存在的模块名、`lib.rs` 的能力查询措辞） | 已修 |
| WP45-9 | MINOR | WP4/WP5 的 PV4 日志缺 fmt/clippy 记录；PV5 只有 1 个用例；`cfg` 扫描模式漏 `cfg!` | 已修：日志补齐 fmt/clippy；PV5 两条断言；扫描模式修正 |

## 复核轮结论：PASS

第 1–7 项均判为已解决（依据见上表）；新发现均为 P2，其中 stderr 覆盖缺口（RV2-F2）与文档口径（RV2-F6）已在 `df734e3` 中处理，其余（出站请求形状快照断言、无会话回收用例、`SECURITY_DESIGN` 引用条目、verification 记法）登记为后续项。
