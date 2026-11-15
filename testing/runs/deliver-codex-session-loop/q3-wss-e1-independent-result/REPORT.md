# E1 独立正式验收（formal-v1）

结论：真实纯数据组件的可达用例通过；本包不能签成 WSS 或 deliver-codex-session-loop 交付。EI01–EI06 有动态通过证据；EI07 是可达动态加结构证据，极限/失败分支未执行；EI08 的 default/all-features all-target check 与 fmt 通过，严格 clippy 两种配置均失败，E1-OPEN001 保持 Open。

## 固定版本与独立性

- 候选：../../session-loop-q003-root-wss-input-implementation-v1/candidate-v1/freeze.json（实际位置为本 tester 目录的同级目录），SHA-256 f18cb38376ed07320ac617fd161f6638e2060122067a57e6403a73c4a9122d50；684 成员逐项重核无差异。
- 候选 source-manifest.json：3ed3bae5408ff4347cb859e094150de929065e0a1eb033bd9cb2a4731513a945，638 输入。Root636 的 635 原输入不变；只改 app/src/management_ws.rs 私有声明，并新增 input_transfer.rs 与 input_transfer/dev_tests.rs。无依赖/锁文件/SQL/前端/权限改动。
- 独立编前定义 ../freeze.json：44123afa6b1498aef69d8373fb7de0597a0e7bd2225d3f580ea8c91630951928，37 成员逐项重核无差异；原定义及首轮记录保留，未回填为通过。
- 受测 workspace：639 文件；相对638仅在 input_transfer.rs 末尾加入非空私有验收叶声明，并新增 app/tests/acceptance/input_transfer.rs（255行、7个 #[test]）。assets/input_transfer.rs 与挂载叶逐字节相同。使用真实 claims::Id128、真实 codec/store，不用旁路 Std 演示冒组件覆盖。
- evidence/provenance-recovery.json 记录逐摘要复核；所有七次命令的 before/after 清单相同，且均等于首次独立测试的639文件清单。开发自评和开发测试不代替本结论。

## 实际环境和执行

Windows 10.0.26300.0 x86_64；rustc/cargo 1.98.1，host x86_64-pc-windows-msvc，Git 2.56.0.windows.2。完整工具输出见 evidence/environment.txt。全部使用本 formal-v1 的 target、tmp；CARGO_NET_OFFLINE=true、CARGO_INCREMENTAL=0、CARGO_BUILD_JOBS=2，设置 testing/ci/windows-owner.ps1 的实际当前owner。没有安装依赖、联网、npm audit、Socket、数据库、用户数据或产品/Harness外部进程。

run.ps1 逐次保留 command/cwd/target/tmp/start/finish/exitCode/sourceSame。表中 cargo 参数以实际 result.json 为准；所有检查均已结束，无在飞 cargo 会话，tmp 为空。恢复时只读系统进程枚举被系统拒绝，未据此推断进程状态；完成状态来自已经结束的工具会话和各 result.json。

| 证据目录 | 实际命令要点 | 结果 |
|---|---|---|
| evidence/independent01 | cargo test --offline --locked --lib management_ws::input_transfer::independent_acceptance -- --nocapture --test-threads=1 | exit0；7 passed，0 failed，530 filtered out |
| evidence/developer-test-inventory | cargo test --offline --locked --lib management_ws::input_transfer::dev_tests -- --test-threads=1 | exit0；4 passed，0 failed，533 filtered out；只作开发叶库存 |
| evidence/check-default | cargo check --offline --locked --all-targets --message-format=json | exit0；13 dead_code warning |
| evidence/check-all | cargo check --offline --locked --all-targets --all-features --message-format=json | exit0；13 dead_code warning |
| evidence/fmt | cargo fmt --all --check | exit0 |
| evidence/strict-default | cargo clippy --offline --locked --all-targets --message-format=json -- -D warnings | exit101；13 dead_code error |
| evidence/strict-all | cargo clippy --offline --locked --all-targets --all-features --message-format=json -- -D warnings | exit101；13 dead_code error |

所有命令 manifest 为 app/Cargo.toml。evidence/check-summary.json 是从原始JSON诊断与测试输出重算的数量。严格检查失败后不能声称其余所有target均完成分析；不将 cargo check 当测试执行。

## 逐例结论

| ID | 结论 | 实际覆盖与证据边界 |
|---|---|---|
| EI01 | PASS（L3/L4数据组件） | 独立手工32字节header、ACK固定常量，00..0f/全0/全ff ID；offset0、2^53两侧、u64::MAX及固定0x0102030405060708 ACK端序；1/16384 payload；字段、原payload地址一致。大offset只证codec数值保真。 |
| EI02 | PASS（L3组件） | 长度0..31，魔数含ACPV、版本、kind、两保留位、Data空/16385、ACK多1字节、反向合法帧均真实Err；全部闭合错误类型Display/Debug不带canary。构造Allocation枚举仅测试格式，未触发分配失败；InputChunk/Uploads无正文Debug。 |
| EI03 | PASS（L3/L4组件） | 真实decode后append；旧/跳跃/u64::MAX offset、未知ID、重复begin拒绝；每次核旧内容不变，正确续写和take字节恰一次；ACK解析不推进容器。没有发送真实ACK。 |
| EI04 | PASS（L3/L4组件） | 实际33554433字节、2049块逐块decode/append、全文等于原字节；含空白、大整数字面、300层嵌套及跨16384边界UTF8。未解析JSON，也未受理Prompt或连Harness。 |
| EI05 | PASS（L3/L4组件） | 3项交错写、独立4097个u128 ID真实begin/append/take，重复不覆盖；两容器相同ID保持不同正文，taken/discarded明确失效。不是跨连接认证或4097端到端管理请求。 |
| EI06 | PASS（动态所有权 + 结构证据） | 非空Vec take前后指针/capacity相同，第二取回失败；drop原容器后返回Vec完整，discard与自然drop执行。字段HashMap<Id128,Vec<u8>>、remove无隐藏副本为结构证据；未计量allocator物理释放次数。 |
| EI07 | PARTIAL（动态可达 + 结构） | u64::MAX在小buffer上真实返回Position，后续正确续写无污染；普通真实增长见EI04。代码在变更前做usize/u64转换与checked_add，并先try_reserve再insert/extend。OffsetExhausted、合法近地址极限及真实组件Allocation失败分支未动态触达；无假len、fake OOM、孤立Std签字。 |
| EI08 | FAIL / E1-OPEN001 Open | 默认和全特性all-target check及fmt通过；严格默认/全特性各13 dead_code错误。父模块仅私有声明，生产WSS尚无消费者；真实用例总数7，开发叶另4。不能完成CL-001或Root最终完整检查。 |

## 问题与未执行义务

E1-OPEN001（阻断正式产品交付；Open）：所有生产输入类型与函数未被生产消费者使用，strict clippy 默认与全特性失败。evidence/strict-default、strict-all 的首轮错误原样保留，未加allow、假消费、公开导出或修改门槛。真实WSS接线并固定新候选后，需由独立tester重验关闭；本数据组件的通过证据可按同源规则决定复用，不能自动继承。

未执行：真实组件分配失败/OffsetExhausted/近usize-isize-u64极限；allocator物理释放计数；WSS当前认证与逐块/交接前复核、随机upload签发、撤销/断线/Node停止释放、ACK-after-save实际发送及SocketFlush+Credit；parse/A/R3、Node登记去重/consume/accepted接管、队列、ACP/Harness、Web和CL/EP整体L5/L6。现有控制帧32KiB、120/min、15秒等规则没有被本包改写。未跑 npm run check、npm run build:rust、整库all-target test、Web、供应链/审计、真实Harness；该范围保持未完成，不做替代签字。

## 可移植交付与安全收尾

acceptance.patch（SHA-256 ee0d38473d4e2de82d61778a55566e6b558e3a65eb203b1531f3e470a7e28f7c）只含独立叶和其cfg(test)挂载，先在638同源隔离副本git apply --check --whitespace=error-all，再实际git apply，两者exit0。Root商品组件补丁的先前check/apply与636→638摘要核验记录仍在 evidence/product-patch-verification.json，本次没有应用到Root。

本机core.autocrlf=true使两个补丁的新叶后像只出现CRLF/LF差异；原始后像保存evidence/product-patch-raw与acceptance-patch-raw，确认只换行后恢复批准的精确源字节，638及639后像逐项一致。没有改Git配置、索引、执行位，没有提交/推送/发布。

MOUNT.md 给出后续安装与重验步骤；source-manifest.json 固定639受测文件，freeze.json 固定本报告、脚本、用例、原始日志、二进制及受测源码（排除target/tmp/patch-assembly）。SAFE_FINAL=true只表示本独立工作包资源和证据已收束可交接；productDeliveryPass=false，EI07未达分支及E1-OPEN001不因收尾改变。
