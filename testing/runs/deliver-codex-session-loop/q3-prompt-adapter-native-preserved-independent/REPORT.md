# A 在最终 Native preserved fixes 组合上的独立重验

2026-11-02。**有限A通过：新789源公开7、私有5首次均绿；必要default/allfeatures all-targets与fmt均0。完整产品未签。** 原783报告、Root及其他人目录没有修改，本轮没有产品改动或验收预期改动。

## 版本与资产

输入freeze dcb2f657f8250dd231cea71993ba4a424dba8801bac2e6973a1725e58c1dffd0的1621成员已逐项核SHA并记录长度（含roundtrip重复源）；实际受测仅workspace787源，manifest f5d3794755177393620dbb6871e56cb2cb4a61abef33fe59fdccff8b400ba680。全部复制到新独占workspace后逐SHA核同。与783相比仅node/trust变更及4新增叶，与固定清单一致。独立读取node源码确认生产前缀在cfg(test) mod tests之前原字节不变。

已准原asset-install.patch原字节SHA2f310d341e30bb2392cf3f226c8ebb5b9dd1399ddf59d78c6079024ba697450d，在完整独占副本实际git apply --check=0、apply=0。只新增原公开叶、原唯一改名私有叶及lib原3行挂载；固定lib原SHAa881e2383ac331c25d88d7290943f8855a4a1ee240e3233f2c5339329f3385a0，两个测试叶须原不存在。安装后三路径SHA逐字匹配上一已准资产：公开a40379915fae73fba1befdb7cb552bae9608968a35cbc2121d6f5bd9226da97f；私有7587db52c6bd5dde0da6665caea83eaa98c4b3788e86ad84299b77a94976e5ab；lib edf312134c8961ddd3b3238faa3f435a8e91a3e179369b1f71ccac0e9391f1f0。

最终789源清单SHA **c3436b5cf732fec40f819c038b53d644cabda9170a504e20d8191defdcca9a72**。精确前后合同见ASSET-CONTRACT.json；在未来不同before上不得仅凭补丁同名直接安装。

A四产品叶、ValidatedBodyPort/RetainPrompt/NativeProtocolPort实现、真实NativeQueueDriver capture来源和Storage入口均与已准783原字节一致；当前789扫描仍唯一实现，无新增authority/proof构造口。公开PA01关联类型和私有PA07真实消费来源在本轮重新运行，未仅继承旧绿。

## 实际执行和关闭尾

| 新789实际命令 | 退出 | 实际数量 |
| --- | ---: | --- |
| cargo test --manifest-path app/Cargo.toml --offline --locked --test prompt_document_acceptance -- --test-threads=1 --nocapture | 0 | 7绿，0红 |
| cargo test --manifest-path app/Cargo.toml --offline --locked --lib prompt_document_acceptance:: -- --test-threads=1 --nocapture | 0 | 5绿，0红；过滤671不计执行 |
| cargo check --manifest-path app/Cargo.toml --offline --locked --all-targets | 0 | 编译，不计测试 |
| cargo check --manifest-path app/Cargo.toml --offline --locked --all-targets --all-features | 0 | 编译，不计测试 |
| cargo fmt --manifest-path app/Cargo.toml --all -- --check | 0 | 格式，不计测试 |

本轮A实际12绿、失败0、未执行0。公开与私有原断言、Audit五实参、唯一fixture方法改名保持；没有改原预期。逐测试名见RESULT.json和原stdout。PA05实际大于32MiB wire/v1回退，PA06实际完整字节去重/冲突、parse前置和同Arc，PA07原prepared/direct消费、drop/edit/stale/revision均执行。

私有5均实际走到原Area.close：QueueService.shutdown→Storage.close→report.joined，PA05另走原IO shutdown→closed.joined；catch_unwind/close/resume_unwind原样。证据仅限这些原断言，不能扩大为物理DB/sidecar释放或完整Windows进程树证明。原测试要求DB保留，5个自有合成SQLite文件仍在data/private；tmp空，无失败资源清理重试。所有5条命令已收到close退出事件，不宣称全机进程审计。

五条命令均有before/after789逐SHA清单，源码及锁不变；target、tmp、data全为本新目录独占。Rust依赖离线locked，fmt不解析依赖。没有网络、npm audit、提交、推送、Root/索引/旧包修改。

## 精确复用与限制

固定787中的Trust11、Node46真实stdout、退出0和前后source manifest已读并复制到inputs/candidate/logs；Web check0、27文件558绿、build0原日志及exit也已核读。此次资产安装只影响Rust的cfg(test)局部挂载与2测试叶，未改Trust/Node实现和既有测试、Web/package/锁；其编译影响另由新789实际default/allfeatures检查覆盖。因此本轮精确复用这些Root同版证据，**不冒称独立又执行57+558项**。Root所报50区首次remove、2主DB首次独占及6侧文件不存在仅作为其同版范围记录，不替代A本轮关闭尾，也不关闭Shared View间歇问题。

strict复用固定787实际101/603及其code/message/source文本对比记录；独立已数原JSON603条。本轮没有在789重复strict，不宣称789 strict绿或所有target lint完整可达。Q3-COMP-STARTUP001、Q3-COMP-CLEANUP001继续开放；未重跑全ACP143、不用旧783或诊断副本绿代签最终789回归，也不重试碰绿关闭Root问题。

未签Root实际合入后结果、C/D普通生产工厂/schema9、Node/WSS/上传/SocketFlush/UI完整链路、真实Harness、物理OOM、完整进程树和数据库物理释放。相关能力不因这12项绿变成完成。

SAFE_FINAL（本冻结证据和有限A数据组件）=true；SAFE_FINAL（完整产品）=false。
