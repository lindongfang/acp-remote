# SQLite 容量耗尽回滚开发验证

日期：2026-10-30。仅新增 `app/src/trust.rs` 的测试，不改变产品代码、协议或验收预期。

固定版本：`2026-10-30-implementation-r7.sha256`，83项，SHA256 `D0D20F74FE174616A7876F05664A2D993769D997928091605B603A1B6E1A9471`。相对r6只有trust.rs测试变化；其SHA256为 `2084AAB9DBE3919149918F120BBC86863DA9284D9D45030ADF7CD39EEA621849`。

实际执行以下定向测试退出0（1 passed，127 filtered out），并执行all-targets Clippy退出0：

```text
cargo test --manifest-path app/Cargo.toml --locked --offline --lib trust::tests::sqlite_full_rolls_back_revocation -- --nocapture
cargo clippy --manifest-path app/Cargo.toml --all-targets --locked --offline -- -D warnings
```

测试使用真实SQLite及产品TrustStore.revoke。在同一产品连接池限制独立数据库的max_page_count，以仅测试存在的审计触发器分配128KiB blob，实际得到SQLite错误码13（SQLITE_FULL）。没有用普通外键/RAISE错误替代容量错误，也没有占满宿主磁盘。

失败后客户端仍trusted，auth_generation和revoked_at不变，revision、审计、幂等结果及触发器写入均无半提交。恢复此隔离数据库容量并删除适配器后，同一requestId成功撤销，重放返回原结果。连接池显式关闭，仅删除本测试新建数据库。

这属于NA-027模拟磁盘满分支的开发证据，尚需独立tester运行与reviewer检视；不据此将整例或change判定通过。旧manifest及独立报告保持原版本含义。构建缓存硬链接不可用时Cargo自动复制的警告未影响断言，不是产品故障。

另对当时522个可版本化文件的独立副本执行官方Gitleaks 8.30.1，约6.99MB，退出0、零命中。原始结果`.tmp/current-source-scan-r7-20261030-results.json`；扫描早于本报告落盘，不声称覆盖后续新增报告或探针。此项是开发扫描，历史扫描及允许项独立闭环见独立供应链报告。
