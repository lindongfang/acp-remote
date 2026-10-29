# implementation-r7 独立定向检视

日期：2026-10-30。结论：**PASS（新增SQLITE_FULL回滚恢复测试范围）**，不代签NA027整例或整体归档。

清单SHA256独立核对 `D0D20F74FE174616A7876F05664A2D993769D997928091605B603A1B6E1A9471`，83项全部匹配。trust.rs新增 `#[tokio::test] sqlite_full_rolls_back_revocation_and_allows_retry_after_capacity_recovers` 位于cfg(test)测试模块，普通产品无新故障开关，无须替换产品binary。原r6及r5a已关闭项保持。

审查 `app/src/trust.rs:1048–1126`：在专用测试SQLite库创建full_probe和审计INSERT触发器，限制该DB max_page_count到当前page_count，真实产品TrustStore.revoke在审计触发器分配131072字节时返回SQLITE_FULL；明确解构实际SQLite Database错误并断言code="13"。不是用字符串模拟错误，不是先返回伪StorageUnavailable，也不是宿主盘被填满。

故障后检查client仍trusted、原generation=1、revoked_at=None，revision/audit/management_commands/full_probe均为0；还原容量上限、删除本轮trigger/table后，用同requestId真实撤销成功，原结果重放相等。最后释放trust、storage.close并删除自有DB文件。未修改用户DB/全局磁盘配额。

独立执行：

```text
cargo test --manifest-path app/Cargo.toml --target-dir .tmp/reviewer-implementation-r4-target --locked --offline --lib sqlite_full_rolls_back_revocation_and_allows_retry_after_capacity_recovers -- --nocapture
1 passed; 0 failed; 127 filtered out; exit 0
```

使用真实用户和reviewer自有target，未跑整套检查或冒称开发Clippy为独立执行。没有发现需修问题。

限定采纳：满足NA027“模拟磁盘满”的真实SQLite pager容量失败/状态审计原子回滚/恢复重试子断言，不应继续要求填满宿主盘作为额外门禁。故障发生在审计INSERT阶段，不是COMMIT阶段；真实延迟FK COMMIT失败应复用独立存储probe证据，两者不能混名。没有证明真实WSS错误映射、文件系统ENOSPC、WAL/fsync失败或全盘恢复；后几种不自动新增为当前用例必要交付门禁。相关剩余范围见同日revocation-gap-review。

仅新增本报告及本轮授权的revocation-gap-review，未修改源码、manifest、规格、旧报告或plan/tasks，不提交推送归档。
