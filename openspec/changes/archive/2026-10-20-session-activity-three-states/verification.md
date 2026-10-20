# 验收记录

日期：2026-10-20。以下命令均在主检出执行，记录实际结果，不把未运行项写成通过。

## 合同与仓库门禁

| 检查 | 命令 | 结果 |
| --- | --- | --- |
| PV1 | `npm run check` | 通过（schemas/commands/errors/features/assets/acp/docs/boundaries/drift/openspec 全绿） |
| PV2 | `cargo test --locked -p sync-protocol -p node-link-protocol` | 通过 |
| PV3 | `npm run verify` | 通过（`cargo fmt --check`、`cargo clippy --workspace --all-targets --all-features -D warnings`、`cargo test --locked --workspace --all-features`） |
| PV4 | `cd clients/app && npm run check` | 通过：82 个测试文件 / 1269 个测试；typecheck（含 e2e）与 `expo export` 通过 |
| PV5 | `node clients/app/e2e/run-e2e.mjs` | 通过：101/101 browser checks |

## 真实 Daemon 浏览器 E2E（5.1）

命令：

```text
cargo test -p app --test session_activity_browser -- --ignored --nocapture
```

结果：`1 passed`。真实 TLS Daemon + 可控 ACP Agent + Chromium 驱动覆盖：

- 新建会话 `idle`；提交后 `running`；两个权限请求 `pending`；
- 目录待处理计数按会话（两个交互仍只加一）；
- 逐条应答后仍 `pending`，最后一次确认后 `running`，turn 结束 `idle`；
- 失败 turn 结束为 `idle` 且失败原因可见，随后可再次提交；
- 页面切换不取消/不重派（Agent 证据中每个 prompt 恰好一次，无 `session/cancel`）；
- 断线保留最后确认 `running`，重连后收敛，断线期间输出补回。

旧历史 `session.updated` 不覆盖当前状态不在该空库浏览器场景中伪造；由 5.2 的升级库用例覆盖。

## 跨层恢复与兼容（5.2）

```text
cargo test -p app --test session_activity_cross_layer
```

结果：`4 passed`：

- 重启把 `accepted` prompt 结算为 `uncertain`，不重派，持久行可读回；
- 旧执行停止未确认时队列保持阻塞；确认后按 FIFO 继续；
- 缺失 `core.session-activity.v1` 的 Node Link 握手得到 `nodelink.protocol.feature_required`；声明后协商成功；
- v12 → v13 升级后旧 `session.updated` 原文、摘要与序号不变，当前摘要为三态。

既有 `cargo test -p app --test session_resume_e2e --test node_link_e2e` 同时通过（7 + 5）。

## 独立检视（5.3）

- `ReviewCoreStorage`：最终复核 core/storage 无未闭环 blocker/major；两次 follow-up 确认失效摘要重算与同提交标题覆盖已修复。
- `ReviewAdaptersPwa`：最终复核 adapter/server/app/node-link-client/PWA 无未闭环 blocker/major；确认 PWA 必需 feature、`session.updated` 的 daemon origin、占位释放后的摘要收敛与同提交标题覆盖均已修复。
- 检视发现的 1 个 P0（PWA 未声明必需 feature）与 3 个 integration defect 均已在合入前修复，并重新跑了相关门禁。

## 未覆盖/环境说明

- `session_activity_browser` 为 `#[ignore]` 的真实浏览器用例，需要 `node`、Chromium/Edge 与已构建的 `clients/app/dist`。
- 本轮未执行 Android/iOS 原生客户端与真实多机网络；它们不属于本变更范围。
