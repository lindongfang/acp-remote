# RV1/RV2 检视报告 · WP1（文档与依赖矩阵口径）

- Review ID：`RV-WP1-round1`（检视 `4b0145e` → `64a7582`）、`RV-WP1-recheck`（复核 `64a7582` → `e6b4dfa`）
- Reviewer：独立 reviewer 子 Agent（新隔离上下文，只读；未参与实现）
- 提交方式：由调度者写入本文件；正文为实现者收到的 reviewer 报告摘要，逐条保留其结论与依据
- 限制（reviewer 自述）：该上下文无 git 工具，无法读取提交区间 diff，结论以目标版本文件内容为据

## 第一轮结论：FAIL

| ID | 级别 | 发现 | 状态 |
| --- | --- | --- | --- |
| WP1-F1 | MAJOR | PV5 判据写「两条断言」而 `tree_` 只跑 1 个用例；断言未覆盖父进程退出与强制终止路径 | 已修：新增 `tree_terminate_while_parent_alive_stops_parent_and_grandchild`，PV5 现为 2 passed |
| WP1-F2 | MAJOR | Check Plan Change 1（`libc`→`nix`）只登记在 verification.md，design/tasks/plan 仍写 `libc` | 已修：三处全部改为 `nix` 并标注原因 |
| WP1-F3 | MINOR | `MODULE_ARCHITECTURE` 文档头仍只列 6 个 crate；§3.1 写「十二个」；`openspec/config.yaml` 仍称两个新 crate 未实现 | 已修：三处同步为 8 个成员 / 十三个 / 已落地 |
| WP1-F4 | MINOR | §5 矩阵缺 `storage-sqlite`/`node-link-client` 列（既有缺口，`app` 落地时会硬失败） | 未处理：属 App/Node Link 切片范围，已登记 |
| WP1-F5 | MINOR | `win32job` 的 MSRV/许可证无核验留证；doc 与 verification 的许可证口径不一致 | 已修：§4.5 记录本地 registry 元数据结论，并补 `nix 0.30.1` |
| WP1-F6 | MINOR | plan 声称「不改 Sync 契约」，而适配器 view 缺 `turnId`/`version` | 已修：plan 加限定并指向 Contract Changes 的缺口登记 |
| WP1-F7 | MINOR | `lib.rs` 的 `cfg` 判据不成立（`launch.rs` 有 `#[cfg(test)]`）；verification 有悬空引用 | 已修：措辞改为「平台分支只在 platform」，悬空引用删除 |

## 复核轮结论：FAIL（报告后已按报告修完）

- MAJOR：`nix` 口径未同步到 design/tasks/plan（同一文件内自相矛盾）；stderr 的权威文档与变更规范仍写「内容进结构化日志」；规范里的 stderr 超限场景（R51–R53）无用例。
- MINOR：测试计数不一致；`AGENTS.md` 仍称两个 crate 未落地；`nix` 的 MSRV/license 无留证；PV3 无独立 Checks 行；plan 的模块名仍写不存在的 `supervisor.rs`/`interaction.rs`/`catalog.rs`/`credentials.rs`；`cfg` 措辞未说明 `#[cfg(test)]` 例外。
- 处理：全部按报告修复，证据见 `reports/wp3-agent-host-supervision.log`、`reports/du1-main-verify.log` 与 `verification.md` 的「RV2 后的修复」表。
- **注意**：这批文档修复尚未再派发独立 reviewer，因此 3.2 的「无未解决阻断项」目前只有实现者证据。
