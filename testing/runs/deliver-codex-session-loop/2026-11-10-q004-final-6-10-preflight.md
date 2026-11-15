# Q-004 6.10 固定快照预检

## 结论

当前固定快照未满足 6.10。CL-117–122 只有 L3/L4 开发证据，尚无同一版本的 L5
认证节点验收；CL-123 因真实 Web 缺少普通输入、上传、审批、取消、释放和恢复入口而
失败。独立 tester 和 reviewer 均未产出可读报告，因此当前结果只能作为修复前预检，
不能代替同版独立验收，也不能据此勾选 6.10。

## 固定快照

- 分支：`codex/session-foundations-replay-planning`
- HEAD：`65ec7815bb0280f4519989d93a7b4ed3b4b37994`
- 源码清单：`testing/runs/deliver-codex-session-loop/2026-11-10-q004-final-6-10-source.json`
- 源码清单 SHA-256：
  `0FE09DAC5799E3A6F9BC5222E51938862A4A5F4787C0646BC8D6BA5460CE4C40`
- 清单文件数：2160
- 文件列表组合 SHA-256：
  `E1BFE7CFC46B88CF2226CC09FF9033B372C624B51FBB7A5321146EF920DBDF29D`
- 本报告在快照冻结后新增，只记录预检，不属于受测产品实现。

## 已执行事实

在当前用户环境、Rust 1.98.1、隔离 target、`--locked --offline` 下执行：

```powershell
$env:CARGO_TARGET_DIR=(Join-Path (Get-Location) '.tmp\q004-final-6-10-target')
$env:RUSTFLAGS='-Awarnings'
cargo test --manifest-path app/Cargo.toml --locked --offline --lib `
  queue_read_valid_unimplemented_and_invalid_payload_are_distinct `
  -- --nocapture --test-threads=1
```

结果为 `1 passed; 0 failed`。原始输出：

`testing/runs/deliver-codex-session-loop/2026-11-10-q004-final-6-10-tester/current-unimplemented-wss.stdout.txt`

SHA-256：
`C62D20FDE25A6ACBC3EDE2D6EB16004B90564DD94AA8AFF4E0564BA7EE5B5BDC`

该测试明确断言认证 WSS 的 `input.upload.begin` 返回
`platform_unimplemented`。测试通过证明当前缺口被如实报告，不证明上传、普通输入或
真实 Web 闭环可用。

现有开发证据：

- CL-117–122 的 L3/L4 范围：
  `testing/runs/deliver-codex-session-loop/2026-11-10-q004-final-6-8-evidence.md`
  SHA-256
  `D1AF27F07FD36E437BCAC5C4F68BC13BCEC392AC116125980BC0F493AE81267C`
- 停止原因显示、继续操作及受控 WSS/Web 组件范围：
  `testing/runs/deliver-codex-session-loop/2026-11-10-q004-final-6-9-evidence.md`
  SHA-256
  `D9F31DDA34B9C54E913FFCCC31DA11D3122ECACBC906D6F2CE5CAF47CF53C483`

这两份报告都明确不能代替 6.10 的真实 Harness、真实 Web 或 L5 验收。

## 逐用例预检

| 用例 | L4/开发证据 | L5 | L6 | 当前判定与理由 |
| --- | --- | --- | --- | --- |
| CL-117 达到上限与拒绝的原因及停止结果 | 有 6.8 受控 v1/v2 证据 | 未执行 | 依赖 L5/真实 Harness | 保持待验收；不能由 L4 完成 |
| CL-118 普通取消确认与取消通知分开 | 有 6.8 受控 v1/v2 证据 | 未执行 | 依赖 L5/真实 Harness | 保持待验收 |
| CL-119 非成功早终态与迟插入独立协调 | 有 6.8 受控 v1/v2 证据 | 未执行 | 依赖 L5/真实 Harness | 保持待验收 |
| CL-120 已停止后的新输入优先与显式继续 | 有 6.8/6.9 受控 WSS、队列及网页组件证据 | 未执行 | 依赖 L5/真实 Harness | 保持待验收；网页组件夹具不算真实闭环 |
| CL-121 成功失败未知与已停止结果不混同 | 有 6.8 受控分支证据 | 未执行 | 依赖 L5/真实 Harness | 保持待验收 |
| CL-122 立即发送所选项的取消结束例外 | 有 6.8/6.9 受控分支证据 | 未执行 | 依赖 L5/真实 Harness | 保持待验收 |
| CL-123 真实非成功结束与继续操作闭环 | 仅组件/受控进程证据 | 未执行 | 失败 | 认证 WSS 未实现输入上传；Web 未提供普通输入、审批、取消、释放及恢复入口 |

## 代码审计依据

- `app/src/management_ws/session_commands.rs` 当前只处理
  `input.register`、`input.consume`、`input.queue` 和 `input.request`；其余已封闭
  DTO 动作返回 `platform_unimplemented`。
- 已存在 `app/src/node/input_upload.rs`、`app/src/management_ws/input_upload.rs`、
  `app/src/node/queue_management.rs` 等实现部件，但尚未接入认证 WSS 命令路由。
- `clients/web/src/management/client.ts` 与 `clients/web/src/main.ts` 当前只提供
  队列查看和显式继续；没有普通输入提交、上传正文、审批、取消、释放或恢复入口。
- ACPI 已定义 32 字节帧头、16 KiB 数据块和 ACK 编解码，尚需接入二进制控制通道与
  原始上传 owner。

## 后续门槛

修复上述入口后必须重新冻结完整产品文件摘要，执行 Rust/Web 定向与完整检查，并取得
真实 Codex/DeepSeek、真实 Web、SQLite 和 Windows Job 树证据。随后由不继承本预检
上下文的 independent tester 与同一快照的 independent reviewer 逐 ID 给出结论；
问题由提出者复查关闭后，才可考虑勾选 6.10。
