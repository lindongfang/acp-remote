# RV1/RV2 检视报告 · WP3（agent-host 进程监督与平台层）

- Review ID：`WP3-R1`（检视 `64a7582`）、`RV2-agent-host`（复核 `64a7582` → `e6b4dfa`，同为隔离上下文）
- Reviewer：独立 reviewer 子 Agent（新隔离上下文，只读）

## 第一轮结论：FAIL（1 BLOCKER + 1 MAJOR）

| ID | 级别 | 发现 | 状态 |
| --- | --- | --- | --- |
| WP3-F1 | BLOCKER | stdout 超限只 `break` 读循环：不结束进程、不收敛未完成请求、`is_running()` 仍为真 → 坏 runtime 被永久复用 | 已修：新增 `abort_agent`（Oversize 收敛 + `exit.mark` + 结束整棵树）；新增 `oversize_frame_ends_the_agent_and_fails_pending_requests` |
| WP3-F2 | MAJOR | stderr 只有内存环形缓冲，无 `§4.5/§12.2` 要求的结构化日志与丢弃计数 | 已修：结构化计数日志（丢弃字节数限频 warn + EOF 汇总，内容永不进日志）；口径同步进权威文档与规范；新增 `stderr_flood_is_bounded_and_counted` |
| WP3-F3 | MINOR | `wait_exit` 的 `done` 判定与 `notified()` 之间丢唤醒 → 白等 grace | 已修：先注册 waiter（`notified().enable()`）再判 `done` |
| WP3-F4 | MINOR | Unix `killpg` 在 pid 复用后可能误伤无关进程组 | **未处理**：记为已接受风险（需 pidfd 或 `exit.done` 跳过，属平台实现细节） |
| WP3-F5 | MINOR | spawn 后 `attach`/取管道失败会留下孤儿 | 已修：失败路径先 `terminate + start_kill` 再返回错误 |
| WP3-F6 | MINOR | `host.rs` 的 `cfg!(windows)` 落在 platform 之外，且 `grep 'cfg('` 判据命中不了 | 已修：下沉为 `platform::command_candidates`；日志改用两种模式扫描 |

## 复核轮结论：PASS

- 第 1–7 项逐条判为已解决，并给出可证伪性依据（超限用例断言 `Oversize` + `!is_running()`；未宣告能力用例靠 fake child「收到配置写入即退出」反证「没发消息」；未登记 content block 的旧实现必失败）。
- 新发现均为 P2：出站请求形状无快照断言（RV2-F1）、stderr 洪水用例缺失（RV2-F2，随后已补）、stderr 结束日志计数只反映上次播报值（RV2-F3）、`SECURITY_DESIGN` 引用条目写错（RV2-F4）、无会话 runtime 回收无用例（RV2-F5）、文档未同步 stderr 口径（RV2-F6）、verification 一条记法与代码顺序不符（RV2-F7）。
