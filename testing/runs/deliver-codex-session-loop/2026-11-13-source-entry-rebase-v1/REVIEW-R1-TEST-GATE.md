# R1 测试差异定义门

2026-11-13 reviewer 增量结论：允许开始 R1 schema 11 实现。此结论仅授权 `REBASE-PLAN.md` 中的 R1 产品写集及 `TEST-DELTA.md` 所覆盖的开发测试，不表示代码已经实现、编译、通过测试、可安装或完整 WP4 验收通过。

固定输入：

- `REBASE-PLAN.md` SHA-256 `d3545ffdbd7683f91495c288666f398866f0c91943f96f2ce969944fe76818e6`
- `REVIEW-R1-PLAN.md` SHA-256 `955c44c41002be5b38eb9953b653077df941f0346f923909006f8564698611da`
- `TEST-DELTA.md` SHA-256 `c204ed5567cb2c2cb888854c1ce9405709547697b22ee79693dd43f316121e32`

定义覆盖 schema 0 fresh、精确 schema 9→10→11、schema 10→11、11 reopen；四条 Source DDL 的前后故障、metadata/pragma/validation 回滚；真实 deferred-FK COMMIT 失败与 COMMIT 报告丢失；双版本头、完整结构、外键、身份和 schema 10 native release 数据/索引保留。`node_catalog_schema/tests.rs::synthetic_eight()` 被正确识别为受影响 fixture：必须先按依赖顺序移除 Source 对象，再制造并验证无 Source 遗留的精确 schema 8/9 布局。

实施范围仍限于 `storage.rs`、`session_storage/mod.rs`、`session_storage/schema.rs`、新增 `source_sessions_schema.rs`，以及定义中列出的开发测试和受影响 fixture。实现交付后需绑定实际 `--list` 限定名、逐例 Red/Green、数据库快照、进程自然退出和固定源身份；本门不预先授权安装或合入。
