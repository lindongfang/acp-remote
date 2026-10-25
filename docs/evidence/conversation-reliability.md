# conversation-reliability 验收证据

日期：2026-10-23。范围：OpenSpec 变更 `conversation-reliability` 的合同冻结（WP1）、后端受理/读取（WP2）、单准备槽位（WP3）、消息与过程呈现（WP4）、跨层与浏览器回归（WP5.1）。真实 Daemon/Chromium/Codex 验收（WP5.2）**部分执行**，未跑项在文末明确列出，不冒充通过。

> **2026-10-25 补充**：仓库所有者按 5.2/5.3 的判据手动完成剩余真实验收并确认通过，任务据此标记完成。本次确认的原始日志与样本未入库；下方 2026-10-23 的记录保留原样。

## 1. 合同与仓库门禁（PV1/PV5）

在隔离构建目录 `target-verify` 下执行（主 `target/` 被本机开发 Daemon 占用）：

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| PV1 合同门禁 | `npm run check` | 通过：schema/fixture、命令、错误码、feature、资产、ACP 矩阵、文档链接、crate 边界、合同漂移、OpenSpec `--strict` |
| PV5 Rust 全门禁 | `CARGO_TARGET_DIR=target-verify npm run verify` | 通过：`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test --workspace --all-features` |
| PV2 关键 crate | `cargo test --locked -p core -p storage-sqlite -p agent-host -p server --all-features` | 通过：1045 项（54 套件，2 ignored） |
| PV5 跨层套件 | `CARGO_TARGET_DIR=target-verify cargo test -p app --test conversation_reliability_e2e --all-features` | 通过：8/8（E1–E7 的可控 ACP 部分） |

`conversation_reliability_e2e` 覆盖：E1 首条/两条无回显后续 + 重启 + 第二设备读取；E2 两次“继续” + 同 requestId 重试；E3 保存失败零副作用、保存后派发失败、重启 `uncertain` 不重发；E5 文字+附件、异内容回显；E6 同 ID restore；E7 并发 restore 不产生第二个准备。

### 1.1 最终代码上的运行产物

以下日志由最终工作树直接产生，可复核：

- `docs/evidence/conversation-reliability-pv1-pv2-pv5.log`：`npm run verify` 全量输出（PV1 + fmt/clippy/workspace test + PV5）。
- `docs/evidence/conversation-reliability-pv2.log`：`cargo test --locked -p core -p storage-sqlite -p agent-host -p server --all-features --offline`。
- `docs/evidence/conversation-reliability-pv3.log`：`clients/app` 的 typecheck、e2e typecheck、vitest 与 Web 导出。
- `docs/evidence/conversation-reliability-pv4.log`：`node e2e/run-e2e.mjs` 的 128/128 输出。

## 2. 前端门禁（PV3/PV4）

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| PV3 前端 | `cd clients/app && npm run check` | 通过：typecheck、e2e typecheck、1509 项 vitest、Web 导出 |
| PV4 浏览器回归 | `cd clients/app && node e2e/run-e2e.mjs` | 通过：128/128（真实 Chromium + 假主机；含 CR-1…CR-6） |

CR 系列断言：旧“继续”不抵消新“继续”、正式正文按 `turnId` 原位接替、附件上传中/失败、等待→工具→回复过程行、复制标签页不接管，以及新会话不显示临时状态行或放弃入口。

## 3. 真实 Daemon + Chromium + Codex（WP5.2）

环境：

- 独立 Daemon：`target-verify/debug/acp-remote.exe --config D:/Data/acp-remote/reliability-live/config.toml daemon start`
- 独立 `data_dir`：`D:/Data/acp-remote/reliability-live/data`；临时自签证书（`localhost`/`127.0.0.1`）；端口 8899；未触碰用户开发 Daemon 的 data_dir/信任库。
- 真实 Agent：配置中的 `codex`（`codex-acp` dist/index.js），由 Daemon 以真实子进程启动。
- 浏览器：Edge（`--ignore-certificate-errors`，仅本机自签证书），经配对码 `device.pair.*` 完成一次性配对。

已执行并观察：

- **E1（部分）**：在隔离 workspace 新建准备会话，发送“用一句话介绍你自己，然后结束。”；页面先显示本地用户输入与“等待响应”，随后真实 Codex 返回“我是 Codex，……”。刷新页面后，`session.read` 仍返回同一条用户消息与回复（正文与身份未变）。
- **过程反馈**：等待阶段显示“等待响应 / 消息已受理，Agent 尚未返回内容”，未伪造“思考中”；回复到达后过程行让位。
- **发现并修复的真实缺陷**：首条已转正后刷新，准备槽位把同一 `sessionId` 重新显示为“未转正新会话”（准备能力不可用），遮蔽正式会话。修复为「会话一旦进入权威快照即退出准备角色」（`preparationPage`/`#occupiesPreparationSlot`/`openPreparationSession`），并新增 feature 测试。修复后重新构建 Daemon/前端，刷新同一地址回到正式会话与完整正文。
- **终审发现并修复的客户端缺口**：快速失败只回一条终态时，客户端原先把已保存输入重新放回草稿区；现在以终态 `acceptedAt` 作为“已受理”证据，保留时间线条目且不回填草稿（新增 feature 测试）。

未执行（不勾选 5.2，也不声称通过）：

- **E2–E9 的完整真实路径**：刷新后该会话未自动重新激活（页面提示“会话尚未激活；页面可见时会自动恢复原会话”，发送保持禁用）。这与前置变更 `session-activation-lifecycle` 仍有 2 个未完成任务（7.2 真实 Codex 上下文验收、7.4 主检出整体检视）一致；本变更不替它签收，也不在真实验收里绕过激活闸门。
- **D3 分段耗时**：未采集可信的 Agent 发出 → Owner 持久化 → 广播 → 浏览器渲染各段样本，不能报告预算结论；本次只记录“首次真实回复在 15 秒观察窗口内到达”这一事实，不作为性能承诺。
- **E8 原型真机对照**：只完成假主机浏览器断言与 `IMPLEMENTATION-GAPS.md` 第 2.7 节差异清单；未做真实数据下的桌面/窄屏人工对照。
- **E9（30 次提交/事件）**：未在真实 Codex 上做前台负载采样。

## 4. 未决与限制

- 主 `target/` 有用户正在运行的开发 Daemon；所有 Rust 验证使用 `CARGO_TARGET_DIR=target-verify`，未停止或修改该进程。
- 真实验收的临时 Daemon/数据/证书位于 `D:/Data/acp-remote/reliability-live/`，与仓库和用户开发数据隔离；配对设备只存在于该临时库。
- 未记录任何私钥、配对 secret、完整 prompt 到证据或日志；真实验收使用固定无害测试句。
- 5.2 未完成前不能把 `docs/evidence` 当作真实 Agent 的最终签收；5.3 的“所有必需检查有证据”同样不成立。
