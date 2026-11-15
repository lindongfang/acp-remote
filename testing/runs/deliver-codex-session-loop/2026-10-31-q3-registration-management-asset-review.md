# 旧管理资产schema终点适配：限定静态复核

2026-10-31。PASS仅限两处机械终点适配，未运行或签管理功能通过。

亲核原资产 `management_independent.original.rs` SHA256 `f93596287f840e5352499ce4b47a0d26c9bed49cec6802d076cdca15166d0261` 与根历史 `app/tests/acceptance/management_independent.rs` 相同；候选为 `927de2a44a29027ca48f1040932fd1c84e251245de9f82fda7fbd1ced29434a8`。已逐hunk读patch `3abcda504ecff0e2c3674eec39d8d516bf119a35ec8d44b90a64437abebf543a`，并以完整文本精确替换验证：仅 SCHEMA_VERSION 断言4→5、PRAGMA user_version 断言4→5，各唯一命中，除此完全相同。

product_delivery_attempts计数、旧TTL/JWS/正文/审计/旧管理行为断言不变。依赖 `management_deadline_semantics.rs` SHA256 `124890c8423b9537d6927ce3622c26d5b99bdf892bd98170b9f66d263f0ed4aa` 与根旧资产及既有记录一致。原资产与旧报告保留；候选仅用于schema5同源回归，不回填历史版本或改变旧产品oracle。
