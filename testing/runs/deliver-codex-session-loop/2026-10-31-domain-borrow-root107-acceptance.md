# Domain借用端口：Root P2A组合独立验收

结论：本次冻结的107项Root来源上，**Root公开9例、独立allocator 3例、107源隔离副本私有8例，共20项通过；原适用106项回归通过；实际Root全目标严格clippy和fmt检查均exit0**。107来源执行前后全部匹配。未发现本范围新增阻断。此结论独立于旧101版本的通过记录，不代签P2A功能、真实输出授权或完整L5/L6。

## 固定输入与写范围

- Root：`D:/Project/acp-remote`。
- 冻结清单：`testing/runs/deliver-codex-session-loop/2026-10-31-p2a-dbr-integration-source.json`，SHA256 `1896b236e703b8274dc8ab246257dadbacb85d76254bd15891f047fd650cddf1`，107项逐一重算；前后结果见 `dbr-root107-source-before.json` / `dbr-root107-source-after.json`。
- Domain views SHA `01e8ad736724f71a26fab128f819f41f1f65b359b06face964aa5d117c4eb933`，staging SHA `ad3bf8644505a1f9656e58bc94580e146ca52702351570eda9b56c4e88cc8443`，与原正式Domain版本逐字相同。
- r8合同及参数优先级消歧 SHA `5ebc06dca886ad7e76834b8a564414cd034ced08b2dc7c4e483d0e2917aef172`；独立DBR01–08定义 SHA `28319d55ca866d8b4e83b030ea5f649d847268224577625d3e5a6cf17e1cea87`，预期不改。
- 新组合P2A源码为4cf2b6d3…、开发测试f9cbf51a…、ACP模块934d3b0b…；本报告仅验其与Domain及旧资产的编译/回归组合，不签P2A专门19例。

Root本轮只读：没有修改生产、验收断言、hook、Git配置或索引。独占target为本tester的 `target-management`，未复用主target。Windows x64、Rust/Cargo1.98.1，`CARGO_INCREMENTAL=0`、`CARGO_BUILD_JOBS=1`。Root锁及隔离副本锁均为 `87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324`，固定tokio1.54.1。全部cargo命令offline/locked，无网络或audit。详见 `dbr-root107-environment.json`。

## 实际执行

命令工作目录为Root；`<tester>`指本报告所在独占目录。

```text
cargo +1.98.1 test --offline --locked --manifest-path app/Cargo.toml --test domain_borrow_acceptance --test domain_borrow_allocator_acceptance -- --nocapture
cargo +1.98.1 test --offline --locked --manifest-path app/Cargo.toml --test acp_view_observations_acceptance --test session_loop_acceptance --test view_budget_acceptance --test view_budget_allocator_acceptance -- --nocapture
cargo +1.98.1 clippy --offline --locked --manifest-path app/Cargo.toml --all-targets -- -D warnings
cargo +1.98.1 fmt --manifest-path app/Cargo.toml -- --check
cargo +1.98.1 test --offline --locked --manifest-path <tester>/dbr-root107-copy/app/Cargo.toml --lib --no-run --message-format=json
```

| 证据 | 实际结果 |
| --- | --- |
| `dbr-root107-public.log`、`dbr-root107-public-exit.json` | Root公开9/9，独立allocator 3/3，exit0；DBR04还由其自身启动受限子进程运行同一例，child 1/1，已含在9例中，不重复计为第21例 |
| `dbr-root107-private-build.jsonl`、`dbr-root107-private-build-stderr.log`、`dbr-root107-private-build-exit.json` | 新107源copy私有测试可执行文件实际编译exit0；从compiler-artifact提取确切exe，复制进更窄的owner根，摘要留存 |
| `dbr-root107-private/dbr-private.log`、`dbr-root107-private-watchdog.log`、`dbr-root107-private-exit.json` | 原私有8/8，413项其他lib测试filtered，不算执行；owner实际回收pid23080，exit0 |
| `dbr-root107-regression.log`、`dbr-root107-regression-exit.json` | P1 11 + session_loop 69（含原view19）+ budget20 + 独立budget allocator6 =106/106，exit0 |
| `dbr-root107-clippy.log`、`dbr-root107-fmt.log`、`dbr-root107-style-exit.json` | 实际Root all-targets原all/pedantic/nursery配置并-Dwarnings通过；fmt静默通过，两者exit0，无新增allow |
| `dbr-root107-final-exit.json` | 上述退出码和逐目标确切数量汇总；不是只记录命令启动 |

stdout/stderr中的PowerShell NativeCommandError包装只因cargo向stderr输出进度；最终原生退出码均0。没有把包装文字误报为功能失败。

## DBR01–08本版覆盖

| 组 | 本次实际覆盖 | 限定 |
| --- | --- | --- |
| DBR01 借用和满额遍历 | 空/Finished/Incomplete；精确迭代顺序；首次/重复借用零分配；4096目标及32768节点满额逐项遍历、usage不变 | 零alloc由独立测试二进制的本线程计数器实测；零owned item Arc clone沿相同生产源码审查，不冒充运行时strong-count仪表 |
| DBR02 元数据返回 | stamp可返回/复制，handle clone可校验；旧stamp遇新可见变更ReadChanged；不含body的future可返回 | 原15项借用escape/伪造/私有hook负编译本轮未重跑，见下文 |
| DBR03 来源和代际 | 相同generation/revision的不同容器、不同source/native、旧read释放/重建和旧scope存活均按原预期 | 私有不可伪造的每个runtime/connection/load坐标仍是相同same_read源码保证；未新增任意身份setter来伪造动态覆盖 |
| DBR04 锁序/析构 | 同线程callback Drop、barrier跨线程Drop、R析构重入同cell均有界通过；冻结status、callback一次、ReadChanged及退账 | watchdog10秒、失败kill/wait、真实owned child；不宣称穷尽所有线程调度或锁poison |
| DBR05 可见变更 | insert/replace相同bytes仍增版；publish/CAS/容量/overflow、FIFO后失败、overrun/shortwrite/Drop、首次Incomplete和重复no-op精确计数 | 独立allocator实际一次73123-byte null命中、FIFO水位及版本正确；不等同全部全局OOM可恢复 |
| DBR06 post边界 | 返回stamp的post点；其后Drop会使stamp失效；旧代际Drop不影响新read；R析构不持Meta锁 | 不承诺post之后至网络发送期间持续最新，也不回滚callback任意外部副作用 |
| DBR07 MAX/封闭 | 原8私有例实际覆盖最后合法版本、封闭后的写拒绝与legacy诊断读取、已准备stage/writer、FIFO保未移除后缀、no-op MAX、单调hook和stale hook、参数优先级三对照 | 仅现有封闭Penultimate/Last hook注入，不是自然执行2^64次；没有新公开setter或Root cfg include |
| DBR08 布局/集成 | 私有实测meta16、cell48、window160、stamp16、borrow guard120、mutex guard16；window+cell+2usize=224≤1024计费；Root严格检查与106回归 | 不代签完整产品/L5/L6或P2A实体语义 |

私有测试中含 `pending_final_contract` 的旧函数名仅为永久资产命名，实际始终按最终5ebc…合同的坏key优先级执行，没有降低预期。

## 入仓语义完整性

`dbr-root107-public-asset-check.json`确认真实Root三文件逐字等于候选LF源：

| Root资产 | SHA256 |
| --- | --- |
| `app/tests/domain_borrow_acceptance.rs` | `2538c111fe9eda61d5627cca515c82de6714d598d3c97793f9ee5a62527f90de` |
| `app/tests/domain_borrow_allocator_acceptance.rs` | `36292daac3e441b9551bd435c57148dce447dfc5d9cd79282d8df95738151f49` |
| `app/tests/acceptance/domain_borrow_support.rs` | `3ca0debb449739f3b64aca78f2e3288835924eec437299a214558f28754b23db` |

唯一路径/风格适配仍为：库glob改相对support；stage移到公开文件并私有、StagedItem导入随移；epoch/key加const；fixture中0或4096的usize转换改try_from.unwrap；一个unit宏补分号；rustfmt导入/空白。原9/3断言、夹具、调用顺序和child策略不改。`dbr-root-assets/semantic-reversal-proof.json`将适配撤回后，原三文件SHA分别精确恢复为9adcce7a…、dbc58a39…、d4531b20…。Root入仓初次CRLF差异已由父执行者机械恢复为LF，本轮直接核真实Root原字节并实际执行，没有只比较规范化文本。

`dbr-root107-previous-assets-preserved.json`核原28资产和旧六份主要报告/日志均未变；旧原私有日志 `dbr-private.log`没有被本轮覆盖。

## 私有copy及所有权

新copy仅按107项公开清单建立，没有复制ignored目录、用户db/private、target或本机身份。107份中106份与产品逐字同源；views.rs唯一差异是原三行cfg(test)相对path include，完整diff见 `dbr-root107-private-overlay.diff`。原私有8例文件SHA `0c8a9c31bbf8a469e7cd2a41be50904cecfeb0e224017d5ca59cf68a3c27204b`未改；另列合成 `clients/web/dist/index.html`仅满足私有Domain编译，见 `dbr-root107-extra-fixtures.json`，不代表Web/节点验证。copy前后107份摘要一致（允许的views overlay也未变）。

为保护旧日志，runner原字节复制到新 `dbr-root107-private/runner/src/main.rs`；源码SHA与原 `owned_private_watchdog.rs`相同。std-only独立runner没有registry依赖，本地单包lock单列。它的canonicalize允许根缩为 `dbr-root107-private`；只将刚编译、同SHA的自有测试exe复制到该根内，未扩大允许进程范围。固定private filter、预期8 passed、30秒超时及kill/wait完全保留。没有改宿主执行策略。

## 影响判断与未验

`dbr-root107-impact.json`列旧101→新107范围差异：Domain产品源未变，增加P2A代码及独立测试、三个DBR长期资产、P2A合同，ACP模块和Domain README变化。旧两份planning/definition文件仅未列入新的源清单，仍存在于Root、摘要未变，并非删除。因为真实Root组合改变，本轮重新执行20+106和Root严格检查，未沿用旧101绿作为本轮结果。

原固定版15项负编译结论保留在旧报告和原日志；本轮明确 **0项重跑**。Domain借用/stamp/私有hook定义及其稳定依赖同源未变，相关历史静态/编译证据可以定位，但不在本版计为新15项通过。任意线程调度、锁poison、全局Arc/list OOM/OS故障、真实SQL授权/发送时效、WSS/订阅交付、background/approval owner、真实Harness和完整L5/L6仍未由本报告验证。

P2A专门19项及其旧112组合由另一独立tester负责；本报告不代签其功能。Root全工程check/build由主执行者另报。本轮没有网络/npm audit外发，没有新增产品问题ID，没有将文档或合成编译夹具写成已实现链路。
