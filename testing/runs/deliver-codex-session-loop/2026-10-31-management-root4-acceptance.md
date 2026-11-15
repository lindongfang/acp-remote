# 管理输入主区 schema4 组合独立验收

结论：已在真实主区schema4组合重新执行31项管理验收，全部通过；69项现有独立合同回归全部通过；外部接口负向编译11项拒绝符合预期；lib/tests clippy与最终全工程fmt检查通过。本报告是新的根集成版本证据，没有用外部schema3候选的绿灯代签。实际gateway、审批owner、真实Harness及完整L5/L6仍未装配或未验。

## 接入内容与版本核对

本轮只新增`app/tests/acceptance/management_independent.rs`、`management_deadline_semantics.rs`，在`app/src/session_storage/product_commands.rs`加入可移植的cfg(test)相对路径模块。主入口路径为`../../tests/acceptance/management_independent.rs`，后者的child路径为`management_deadline_semantics.rs`；已由根实际编译验证，不含外部绝对include路径。未修改产品逻辑、schema4布局、Storage版本常量、31项业务预期或Git索引。

独立与revision3冻结11文件比较：物理SHA差异包含CRLF转换，不能假称逐字节一致。全行diff显示产品内容差异只有保留`domain::views`声明、Storage的SCHEMA_VERSION=4，以及本次cfg(test)include；其他管理代码在统一行尾后完全相同。见`management-root4-before-compare.json`、`root4-app_src_domain_mod.rs.diff`、`root4-app_src_storage.rs.diff`和`management-root4-normalized-compare.json`。最终overlay的全行diff为`management-root4-portable-overlay.diff`。

实际新库每个fixture额外核对：代码常量4、PRAGMA user_version=4、product_delivery_attempts表真实存在。因此没有把open4当旧schema3。schema4的独立布局/迁移K11与原13用例仍由其原执行者报告，本轮只新增管理行为与4组合的证据。

测试资产适配仅包括：自有合成库位置由外部copy相对目录改为操作系统临时目录，避免在仓库根产生数据；增加上述schema4前置断言；修正测试局部命名/等价数值写法/模块格式。两个即时可信callback刻意保留async，让所有权读取与writer计数仍发生在future被poll时，避免因改成提前执行的ready future改变取消语义；只加窄范围带reason lint说明。完整差异`management-root4-test-adaptation.diff`，原31业务断言没有放宽。

## 冻结摘要

78份根Rust/测试及Cargo/build输入的初始、最终清单：`management-root4-source-initial.json`、`management-root4-source-final.json`。执行期间仅本验收主文件的语义不变风格修正，以及cfg(test)声明顺序修正；无其他源变化，逐项见`management-root4-source-changes.json`。最终重点摘要：

| 文件 | SHA256 |
| --- | --- |
| management_independent.rs | f93596287f840e5352499ce4b47a0d26c9bed49cec6802d076cdca15166d0261 |
| management_deadline_semantics.rs | 124890c8423b9537d6927ce3622c26d5b99bdf892bd98170b9f66d263f0ed4aa |
| session_storage/product_commands.rs（仅含测试overlay） | e4d1f446e7e3f1ac4f2496f0b2d7511298a57d29b5d2cb087e742900c8c27333 |
| storage.rs（SCHEMA_VERSION=4） | d4bf0e624ffa20b9b73cc167f2a346557f02a23dd0a54a240cfb4caf51ef7bb4 |
| session_storage/schema.rs | fcec6f7e929056bdf7809534c7f9566c7bbe77f5ebb0c2a47be1a00e06564d18 |
| session_storage/product_schema.rs | e3d2d32c0e3ec32f1c6ef4ed4a971f31c4f203bd8ca6d67063f81b4f14aa6c00 |

31项最终运行之后只调整了cfg(test)模块声明的排序，以满足rustfmt；全行diff证明没有改变产品代码、测试函数或断言，所以31/69/负向/clippy证据按无行为变化复用，最终全工程fmt另实际通过。后续实际完整入口由主执行者重新运行并独立保留结果，不据此声称其已通过。

## 实际执行

工作目录`D:/Project/acp-remote`，Windows，工具链rustc/cargo1.98.1，独占`CARGO_TARGET_DIR=<本报告目录>/target-management`。所有运行使用既有锁和offline；故意失败的外部crate从锁种子offline新增本地根包，不增加registry依赖。真实SQLite及合成key、actor、owner、结构、clock/writer；无真实用户库/凭据/Harness/WSS或Web审计外发。

```powershell
cargo test --offline --locked --manifest-path app/Cargo.toml --lib session_storage::product_commands::independent_acceptance -- --nocapture
cargo test --offline --locked --manifest-path app/Cargo.toml --test session_loop_acceptance -- --nocapture
cargo check --offline --manifest-path <本报告目录>/management-negative-root4/Cargo.toml
cargo clippy --offline --locked --manifest-path app/Cargo.toml --lib --tests -- -D warnings
cargo fmt --manifest-path app/Cargo.toml --all --check
git diff --check
```

| 检查 | 实际结果 | 原始证据 |
| --- | --- | --- |
| schema4管理31项 | 初次31/31通过；测试风格修正后再实际31/31通过，0失败/0忽略，最终15.72秒 | management-root4-tests.log、management-root4-tests-final.log |
| 现有独立合同回归 | 69/69通过，exit0；包括原21模型、ACP投影/发现重放及有界视图 | management-root4-model-regression.log |
| 外部负向接口 | 10项E0277+1项E0423符合预期，cargo exit101；无Clone/Serialize/Deserialize、无裸ActorContext转换及私有构造器 | management-root4-proof-negative.log、management-root4-negative-classification.json |
| lib/tests严格clippy | 初次仅验收资产10条风格lint失败；语义不变修正后exit0，未改生产或降警告 | management-root4-clippy.log、management-root4-clippy-final.log |
| fmt/diff | 最终cargo fmt --all --check exit0；git diff --check exit0 | management-root4-fmt-final.log、management-root4-fmt-exit.json |

核心退出汇总为`management-root4-verified-exit.json`，fmt单独记录。类型负向的预期编译拒绝不计作11个Rust运行用例，也不是产品构建失败。

主执行者本轮第一次完整npm check：Web50通过，随后仅因新增cfg(test)声明应按字母排序而在fmt终止，Rust clippy/test尚未运行。原日志`.tmp/session-loop-root4-check.log`已只读复制为`management-root4-initial-full-check.log`。overlay owner仅将independent_acceptance排到tests前，31业务断言与生产代码未动，全工程fmt现已实际通过。这次格式失败不被记录为产品功能失败，后续npm完整check/build结果由主执行另报。

## 结论边界与资源

MGT-CODE-001/002/003在根4组合的原断言仍全部通过：保留ticket跨签名期限后正文/预算清零；末端共享fault零首次发送；操作预算/对象TTL不冒充仍有效签名的请求过期。31逐例名称与结果在真实日志中；各M组已执行网格沿`management-acceptance.md`和revision3报告定义，随机/内存分配失败及COMMIT后owner关联失败等未逐一独立注入的细分仍未执行，不因本次集成补签。

实际认证/trust gate、原生资源和重放来源、实际writer、审批owner与SQL批准交付、正式恢复/最终reconciliation/ready、清理、WSS/Web、真实Windows进程树/真实Harness及完整L5/L6仍未执行或未装配。默认100000审计自然路径仍不可达，只有checked算术与合法缩限SQL有证据。根组合通过不等于全change完成。

pool、future/task与supervisor均在测试结束或显式关闭；合成临时库只在测试自有临时目录保留供证据复核，仓库根没有independent-management-data生成目录，无用户资源待清理。未提交、推送或发布。
