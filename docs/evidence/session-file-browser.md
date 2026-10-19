# `session-file-browser` 验收证据

本文件只登记**实际执行过**的命令与观察结果；未执行项明确列在最后一节，不写成通过。
分层边界：假主机 + 真实组件树只证明客户端行为；真实 Daemon 的字节、Git、根边界与断连
释放由 Rust 入站测试证明；两者不互相替代。

## 1. 真实 Daemon 入站（Rust）

命令（主检出，Windows）：

```text
cargo test --locked -p app -p server
```

结果：**413 passed / 0 failed / 1 ignored**（ignored 为既有的 `workspace_mkdir_browser`
手动用例）。`crates/app/src/daemon/sync_ingress_tests.rs` 的新用例经真实组合根、
真实 SQLite 与真实临时目录/Git 仓库执行：

| 用例 | 断言 |
| --- | --- |
| `session_file_queries_require_negotiation_before_any_io` | 未协商 `core.session-files.v1` 时四条查询回 `protocol.feature_required`；会话根指向不存在目录，错误仍不是 `internal.unavailable`；快照计数为 0 |
| `imported_sessions_are_rejected_before_touching_the_access_host` | imported 会话的 list/status/read 全部回 `capability.unsupported_by_broker`，不建立快照 |
| `real_files_are_chunked_consistently_and_budgets_are_enforced` | 70 000 字节文件按 ≤32 KiB 分块、偏移/版本/字节逐块一致；300 KiB 文本预览显式 `previewTruncated` 且只保存 256 KiB；64 MiB + 1 字节文件回 `resource.result_too_large`；断连后连接级快照归零 |
| `git_status_and_diff_use_the_real_repository_without_writing_the_index` | 真实仓库的 added/modified/untracked/deleted；会话根外的 `top.txt` 不出现；`tracked-sub.txt` 的 diff 含 `+two`/`-one`；已有 HEAD 的仓库里未跟踪文件按 `base=head` 展示（`empty` 只用于尚无提交的仓库）；查询前后 `.git/index` 字节不变；`../top.txt` 被 schema 拒绝 |
| `links_outside_the_root_are_visible_but_not_openable_and_never_hang` | 根外链接条目可见、`openable=false`、`restricted=outside_root`，响应里不含根外绝对路径；`escape/secret.txt` 读取与 `escape` 遍历均 `internal.unavailable`；循环链接在超时预算内返回 |
| `a_disconnected_git_query_leaves_the_repository_usable_on_windows`（`cfg(windows)`） | 断连中断 Git 查询后，仓库仍可被独立 `git status` 使用，新连接可再次取得状态；不标 `ignore` |

Windows 进程树/Job Object 的现场证据由 WP2 的 `crates/app/src/session_files/tests.rs`
提供并实际执行（junction、替换竞态、Git 超时/取消后的整棵树清理）；本文件不把 Linux
专属的 symlink/FIFO 用例记作 Windows 已执行。

## 2. 假主机 + 真实浏览器组件树

命令（`clients/app`，真实 Chromium）：

```text
node e2e/run-e2e.mjs
```

结果：**100 / 100 passed**。新增 `SF-1`…`SF-10` 覆盖：

- 默认「会话变更」，三来源文案与切换；
- 目录树逐层展开、忽略项默认可见/可隐藏、`.git` 不作为条目出现、根外链接不可打开；
- 分页追加与部分结果；
- Git 状态、冲突/二进制/删除文件标识、hunk 头与 `+`/`-` 行，删除文件可打开差异，
  非仓库显式说明；
- 同一文件多次修改只列一行、打开后看到历次记录与部分历史提示；
- 文本按纯文本、脚本/远端图片/SVG 不执行；长文本查看器有独立滚动并能滚到底；
  图片 `src` 以 `blob:` 开头；
- A→B 直接替换、无页签、关闭后草稿/滚动/展开状态保留、焦点回到触发行；
- 窄屏打开后关闭返回文件栏与滚动位置，跨断点保留选中文件；
- 下载交付物是 Blob URL，`data-download-saved` 的写入字节数与源文件一致；页面 CSP
  `connect-src 'self'` 不允许在页面内 `fetch(blob:)`，逐字节一致性由第 1 节的真实
  daemon 用例证明；
- 关闭按钮可聚焦、名称明确并可按 Enter 触发。

`e2e/run-e2e.mjs` 的假主机 CSP 与
`crates/server/src/transport/net/static_ui.rs::CONTENT_SECURITY_POLICY` 逐字一致，含
`img-src 'self' blob:`，脚本策略未放宽。

## 3. 客户端类型、单测与构建

命令（`clients/app`）：

```text
npm.cmd run check
```

结果：typecheck、e2e typecheck、**1204 tests passed**、`expo export --platform web` 均通过。

## 4. 合同与全 workspace 门禁

命令（仓库根）：

```text
npm.cmd run check
npm.cmd run verify
```

结果：合同门禁全绿（schemas/commands/errors/features/assets/acp/docs/boundaries/drift/openspec）；
`verify` 的 `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --all-features -D warnings`
与 `cargo test --workspace --all-features` 全绿。CI 专属的依赖许可证/公告/密钥扫描仍需 PR
检查单独记录，本地 `verify` 不覆盖它们。

## 5. Main E2E（真实 PWA + 真实 Daemon，PV7）

已在主检出启动本机 Daemon（`https://faraday.tail74842e.ts.net:8765`），用设备
`94fcfc9d-7cae-4316-a4a6-1c2e1cd2ff87` 完成真实配对，并在临时工作目录
`D:\tmp\12312`（真实 SQLite 会话 `85eb4d5c-…`）上操作真实页面。实际观察：

- 目录页进入会话；文件栏默认「会话变更」，可切「全部文件 / Git 变更 / 会话变更」。
- 「全部文件」列出 `README.md`、`src/`、`assets/`、`debug.log`、`new.txt`、
  `untracked.txt` 与根外 junction `escape`；`.git` 目录没有出现；`escape` 可见但
  标为「根外链接」且没有打开/下载入口。
- 打开 `README.md`：内容按纯文本呈现，`<script>window.__pwned = true</script>` 原样
  可见、未执行；关闭按钮存在。
- 打开 `assets/pixel.png`：`<img>` 的 `src` 是 `blob:https://faraday.tail74842e.ts.net:8765/…`，
  不是 `data:` 或远端地址。
- 「Git 变更」列出 modified / deleted / added / untracked；打开 `src/app.ts`、
  `old.txt`、`new.txt` 分别得到真实 HEAD 合并差异，删除行/新增行逐行呈现。
  **发现并修复**：根内 junction 曾让 Git status 列出根外 `escape/secret.txt`；适配器
  现在按实际对象过滤根外路径，回归用例
  `git_status_does_not_list_outside_root_junction_children` 已加入并通过。
- 在真实浏览器里以 Blob 回退下载 `new.txt`：状态变为已保存、`data-download-saved`
  为 9 字节、交付物是 `blob:` URL；有 `showSaveFilePicker` 时用户取消保存的路径显示
  「你取消了保存」，不留下未完成内容。
- 窄屏（600×800）：出现「对话 / 会话 / 文件」三档；从文件栏打开 `README.md` 后关闭，
  回到文件栏并保留文件列表，而不是跳回对话顶部。

**未完成的 PV7 部分（不得计为通过）**：真实 Agent 的 `session.prompt` 在本机 dev
Agent 上会进入 running 状态，但页面同时显示
`protocol.schema_invalid：the request does not match the v1 schema`；该现象属于既有的
Agent/命令接线问题，不是文件栏本身，也未被本次变更修复。因此「会话变更按真实 Agent
事件归并」「imported 会话降级」「旧主机未协商降级」仍由假主机 + Rust 用例覆盖，
未取得真实浏览器证据。

## 6. 未执行项汇总（不得计为通过）

1. 真实 Agent 事件驱动的「会话变更」归并、imported 会话与未协商旧主机的真实浏览器
   降级（见 §5 末段；由假主机与 Rust 用例覆盖，但不计作 PV7 通过）。
2. CI 专属检查：依赖许可证与来源、依赖安全公告、密钥扫描（需 PR 检查）。
3. Linux 专属 symlink/FIFO/进程组用例（本机为 Windows；代码按 `cfg(unix)` 编译覆盖，
   未在本机执行）。
