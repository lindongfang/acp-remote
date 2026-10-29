# implementation-r8 独立定向检视

日期：2026-10-30。结论：**PASS（NA012五个模拟失败及真实线程清理子断言）**。未发现本次新增需修问题；不代签NA012整例、跨SID/远程pipe或整体归档。r7的真实SQLITE_FULL定向独立通过已记录在同日implementation-r7-review，继续有效。

## 固定版本

独立读取 `2026-10-30-implementation-r8.sha256`：SHA256 `1F0F7A599C6839D53560EBF40C676015852BFE364DA4C7B51B2BF0BB849E9A9E`，83项逐文件全部匹配。windows.rs为 `CD028D68BB4AF7B9927BE17C1A1A76D6DD260063010A88FB40018F617EE69971`，与交付一致。

审查当前AGENTS/角色规则下的只读范围，使用reviewer专用 `.tmp/reviewer-implementation-r4-target`；不使用主方独占target。仅新增本报告，不修改源码、manifest、规格、旧报告、plan/tasks；不提交、推送或归档，不碰30447节点/身份、正常浏览器。

## 实现与真实清理路径

- `app/src/platform/windows.rs:140–158` 的故障Cell及辅助函数仅cfg(test)，线程局部，匹配时先set(None)再返回错误。安装至验证/后续断言之间同步无await，不会因Tokio任务迁移把故障留给别的执行线程；各测试线程隔离。
- 五点分别在ImpersonateNamedPipeClient、OpenThreadToken、GetTokenInformation size/data、显式RevertToSelf调用之前返回模拟错误。它们**不是实际Win32 API故障**；其余路径确实调用Windows API，返回真实线程状态。
- Impersonate前故障未取得模拟身份，无guard需要回退。token查询故障发生在成功impersonate且guard存在之后，验证函数仍执行显式真实revert再返回验证错误。
- `windows.rs:162–187,290–293`：显式Revert模拟错误时active仍true，`reverted?`提前返回触发guard Drop；Drop不经过测试故障注入，实际调用真实RevertToSelf。错误仍作为Err返回，不能以清理成功改成认证成功。
- 每次检查真实OpenThreadToken失败且GetLastError为ERROR_NO_TOKEN，不只检查模拟标记；随后真实同owner verify成功并再次无线程token。测试使用新建唯一命名的真实owner DACL管道，结束drop client/server，无文件/用户数据删除。
- 去除仅测试块/调用后，与reviewer已保存r5a非test平台源码比较一致（规范化空行/CRLF）；无生产环境开关、新API或普通构建失败策略改变。故障函数会出现在测试构建的current_user_sid等共享token路径，但安装发生在setup完成后且同步消费；不声称该测试覆盖每个Win32调用方。

## 独立执行

在真实当前用户上下文独立编译并运行：

```text
cargo test --manifest-path app/Cargo.toml --target-dir .tmp/reviewer-implementation-r4-target --locked --offline --lib named_pipe_check_failures_reject_and_restore_the_current_thread -- --nocapture
1 passed; 0 failed; 128 filtered out; exit 0
```

该1个测试内顺序执行全部5点，要求各自Err包含对应操作、Cell已消费、真实线程无线程token、后续同owner验证成功。它是实际owner pipe和Windows清理结果证据，不是结构校验或纯假token。

## 严格限制及闭环范围

本轮证据足以收缩NA012的上述模拟失败子断言。**不证明真实RevertToSelf返回失败、尤其持续/永久失败时线程安全恢复**：模拟点在真实API调用前，Drop的真实Revert本次成功；生产Drop对失败只best effort且不检查结果是原有行为，本包没有修复或认证该边界。不得在汇总中写“永久revert失败也已安全处理”。

本轮没有重做受限shell全量DPAPI或pipe等待；交接所述中止运行不能标成r8产品动态FAIL，也不能当通过。本报告只认上述实际定向执行，不等待/冒签主方全量check。跨真实另一SID、远程pipe及当前用例要求的五命令覆盖仍需按独立证据逐项保留缺口，不以同owner故障测试关闭；已有未变字节的历史有效子断言可明确范围复用。

原CON/LIFE/SC/I01关闭保持；NA-RV-001/002为不同生产协调问题，r8测试补充不关闭它们。整包其他用例与是否可进入待归档阶段仍由最终独立汇总判断。
