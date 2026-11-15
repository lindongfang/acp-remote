# P2A隔离候选交接

P2A有限工具锚点已实现并冻结，等待原tester A–H同版独立验收及reviewer代码检视。没有合入/提交/推送；主D、旧外部冻结副本与独立资产未修改。

## 来源与差异

- baseline-source.json SHA256：5ddc610e9c4d3c6b898616e2e4c324e5ab11a7ca3a51e82d5d6618e6de1b28c4。实际root git ls-files --cached --others --exclude-standard限定公开app源码/tests/examples、Cargo/build.rs/toolchain及固定JWS fixture，共同获授权生成dist合计101文件；不复制忽略配置/数据库/target。
- implementation-source.frozen.json SHA256：9181f209916771ecc59e2b3b7f0f8703beaab59266234b8f95b2ee769f7637b6。固定103文件，原100文件逐hash未变；仅原acp/mod.rs增加声明，新tool_projection.rs及其tests.rs。
- p2a-candidate.patch SHA256：ea8bac49e74c436482ca4457095732d9ca1d9e8e6436bb0b6257b5263d73ac49。仅上述3路径；root执行只读git apply --check --no-index --whitespace=error exit0，未实际apply。
- 主产品新模块SHA256：4cf2b6d36a8a3095bed73de496d926cec871e97bc0addf431256be5a8d54b503。
- 依据root81d0合同、tester faa72/inputs4a797及reviewer cbcff，完整摘要写在manifest。基线P1=0164d462...、Domain=a22bb6.../d2a2a6...，未修改；不覆盖root新README或node fixture。

## 实现边界

ObservationRead::observe恰一次，其Ignored/Err/Recorded/Dropped事实保留。整帧形状与本版枚举先验证，才消费现有Tool/id lease、构建固定512并单次发布；只有真实MissingItem可CreatedPartial，Capacity/损坏/错scope不另位置fallback。所有§4A保持/赋值、q/origin/loss与flags按最终矩阵实现；chunk只触content，不patch其_meta到实体。

decoder检查恰512、namespace/wire、保留区、16槽、序号/来源/loss及小码联合约束。v1四status/十kind，拒cancelled/未来枚举；v2未知status/kind各按in_progress/other保守显示，保留原raw且无Retained承诺。getter仅借用/Copy，任意长度值仍须P2B实际定位；工具小状态不形成执行/审批事实。

未clone RawMessage/Value/String或原生工具ID，唯一生产.clone为既有ViewHandle控制身份。无unwrap/expect/panic、无新依赖/namespace、无Domain/P1/strategy/transport修改。没有新scanner/current或真实连接owner证明。

## 实际检查

全部现有锁、offline/locked、固定工具链，使用本次visualizations根的独占短target-p2a；同进程加载项目Windows owner脚本，禁用incremental避免文件系统hardlink警告。无需外部审计/网络。

| 命令/范围 | 实际结果 | 证据 |
| --- | --- | --- |
| cargo fmt -- --check | exit0 | fmt.final.log |
| cargo clippy --offline --locked --all-targets -- -D warnings | exit0 | clippy.final.log |
| cargo test --offline --locked --lib tool_projection --no-fail-fast -- --nocapture | 新开发15/15 | test.final.log |
| cargo test --offline --locked --lib adapters::acp --no-fail-fast | 合计60/60，含本15，勿重复累计 | regression.final.log |
| cargo test --offline --locked --lib domain::views --no-fail-fast | Domain开发20/20 | domain.final.log |
| cargo test --offline --locked --test session_loop_acceptance | 原公开69/69 | public.final.log |
| 四个原公开二进制：P0/P1/budget/allocator | 6/11/20/6，全0忽略 | supplemental-regression.actual.md（该次完整stream仅工具会话，文件为明确汇总） |

15项新开发覆盖严格格式/码/offset、版本与一帧原子失败、精确null/Deferred/no-op、D→R历史loss、257双版真实Connection原位、长raw>256KiB、Dropped小值、真正FIFO与同帧raw自淘汰、引用/字节峰值失败禁止换位、Unicode4096边界、begin退款、corrupt/stale/跨容器及实际replay response→Live来源。非全部独立A–H，也非完整node/Harness；同步内部CAS/极值无新公共暂停/注入入口。

实际编译布局打印：ToolProjectionRead=552，ObservationRead=304，额外248<=512；ToolPatch=40，AnchorView=48，StageWorkspace=144，StagedItem=360。payload编码在已付费512 scratch，固定字段没有另建DOM/动态索引；这些size_of不是操作系统RSS/全进程OOM恢复证明。原预算20+allocator6公开回归同版通过，独立角色仍须按其原预期检视P2A峰值/源码。

## 失败与未验

初次fresh依赖构建在ring/lib.exe发生LNK1114/error5，尚未产品编译；较短target及同进程owner后实际构建成功。初始两处clippy开发提示及后补空String提示均已修复，原中间日志/initial-attempts.md保留；最终fmt/clippy全过。初始输出有工具截断/未留完整stream的地方已明示，未重建成伪原始日志。

P2A独立A–H尚未由原tester执行，coder不自行关闭VPROJ-PLAN-002或完整任务3.2。满4096引用时lookup/替换仍可能真实Capacity；首开交付、P2B完整current/scanner/借用revision、全部SDK嵌套解释、真实来源/授权/撤销、node/WSS/Web/可信轮次/实际Harness均未实现/未验，不用此候选充通过。Web/依赖audit未运行。请从冻结workspace复制同版或按patch合入后另固定集成SHA，不复制其他共享文件覆盖root。

