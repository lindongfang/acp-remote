# P1验收资产入仓风格修正与实际root复验

结论：只修正root验收主文件的两处局部命名和一处等价转换；原11项断言/流程不变。实际root **11/11通过**，0failed/ignored/filtered，2.71秒；root全目标严格clippy、全工程fmt及git diff --check均exit0。冻结产品10源前后全部符合revision2 manifest，外部原验收资产与原失败日志均未改变。

## 精确改动及两个root摘要

- `app/tests/acp_view_observations_acceptance.rs`：R2局部`stats`及其读取改为`baseline_metrics`，R3局部`stats`及三处读取改为`loss_metrics`；避开state/status相似名。header中的`version.number() as u8`改为`u8::try_from(version.number()).unwrap()`；当前闭合Version只有1/2，结果相同，没有改变字段期望/头偏移或增加新的产品路径。
- `app/tests/acceptance/p1_observation_ordering.rs`无需修改；两个文件均显式rustfmt，ordering文件最终字节仍与外部原三反证一致。没有新增allow/ignore、删除测试、改变Drop次序/尺寸/调用顺序或放宽预期。

| root文件 | 最终SHA256 |
| --- | --- |
| app/tests/acp_view_observations_acceptance.rs | 00146db42db0c6a78fe9f888e0d9cf68af0e43606ae5d95ed546656ffd6f9d0f |
| app/tests/acceptance/p1_observation_ordering.rs | 7c68b790f6e6df7debb0e0229c9b537de5a8d029c47b2f20198d15ffed452238 |

入仓主文件修正前SHA为a87f1f7e63ac1c7f6462ddab9a16def556007bdf8cd979596ac394b1c4cd21fb。完整hunk见`p1-root-style.diff`；相对外部原lib的完整差异见`p1-root-external-adaptation.diff`，仅额外包含父执行者已机械加入的显式模块路径。修正前两文件及原clippy日志副本保存在`p1-root-integration-before/`。

`p1-root-assets.json`同时核对：外部原lib仍为2db0c260...、ordering仍为7c68b790...；root原失败日志`.tmp/session-loop-p1-integrated-clippy.log`与备份均为93810c3735adb321e946e3f2cfba6e0fb81a23efbb310ea431b9454b547308f8，未覆写。该旧日志的3项测试风格错误不当成产品功能失败；最终检查另存新名。

## 实际执行证据

Windows，root工作目录D:/Project/acp-remote；独占本tester的target-management，进程CARGO_INCREMENTAL=0，offline/locked，不复用主target。先仅对授权两个文件运行rustfmt，不格式化改写其他源码。

```powershell
rustfmt --edition 2024 --config skip_children=true app/tests/acp_view_observations_acceptance.rs app/tests/acceptance/p1_observation_ordering.rs
cargo test --offline --locked --manifest-path app/Cargo.toml --test acp_view_observations_acceptance -- --nocapture
cargo clippy --offline --locked --manifest-path app/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path app/Cargo.toml --all -- --check
git diff --check
```

实际同shell退出码在`p1-root-exit.json`：twoAssetFormatExit/testExit/clippyAllTargetsExit/rootFmtExit/diffCheckExit全部0，UTC 2026-10-31T01:48:51.5857954Z。运行日志为`p1-root-tests.log`、`p1-root-clippy.log`；fmt静默，其`p1-root-fmt.log`确为0字节，退出依据是该JSON。diff日志`p1-root-diff-check.log`含Git行尾提示，不是空白错误，真实exit0。PowerShell对cargo正常stderr进度的包装不按产品错误计。

`p1-root-production-before.json`与`p1-root-production-after.json`各10项，均逐份匹配冻结revision2源（main0164d462...、开发tests0017c374...、合同/Domain/transport/lock等），没有产品逻辑或依赖变化。本轮11项含001原三反证；原95项回归/9负编译/布局证据保留其上一正式同版外部报告归属，本轮未冒称再次执行。主执行者随后完整check/build另取结果，不由这里代签。

## 范围保持

只完成入仓资产风格与root组合复验；仍沿`p1-revision2-acceptance.md`的限定：内部u64极值和P1私有分配故障未独立动态注入，满4096项snapshot交付限制、实际来源/单一owner/授权装配、P2实体工具原位/typed current、完整3.2及L5/L6保持pending。未运行Webaudit、网络或真实Harness；未提交、推送、发布或改Git索引。
