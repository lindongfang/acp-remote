# P1 raw观察初版独立执行：保留失败证据

结论：初版main `1e38ef176b4c2d505d05cc84aa506817952a91148e79b7e50d6b2574926f0dec` 的8组普通公共API基础探针实际8/8通过，但P1-CODE-001三项独立负例全部失败；不能签P1整体通过。测试预期不变，修订版须在新冻结源上重新执行。本文不是最终交付验收。

本版输入是root `2026-10-31-p1-raw-observations-source.json` 十份来源；本tester初始逐项校验匹配，记录 `p1-source-before.json`。root随后保留十源完整副本于 `D:/Project/acp-remote/.tmp/session-loop-p1-revision1/`，原source.json与原路径结构不变。来源/权限仍由未来实际Connection owner负责，字段检验不证明授权；P2实体原位002仍pending。

## 实际探针与原日志

外部普通依赖crate `p1-probe` 通过产品公开API、真实Connection双工输入和合成数据执行；没有include产品源码、依赖cfg(test)端口、读取用户数据或网络/audit。独占target为本目录target-management。root Cargo.lock种子，offline。

- `p1-matrix.log` / `p1-matrix-exit.json`：8 passed、0 failed、3 filtered；2.65秒，exit0，UTC 2026-10-31T01:15:18.0943301Z。主断言包括双版完整raw/外层meta/数值与转义、P0真实v1 null入口、Replay→Live/DuringResume、Ignored、预算失败后继续pump与安全权限处理、精确预算/旧代际Drop、头负面和真实协议错误区别。
- `p1-ordering-failures.log`：同普通依赖/root-lock的定向P1-CODE-001，0 passed、3 failed、8 filtered、exit101。OtherSession event/raw不一致和v2 State event/raw不一致均实际Err(InvalidObservation)，原预期Ok(Ignored)；旧read被新read取代后匹配event/raw不一致也实际Err(InvalidObservation)，原预期View(Stale)。三项通过真实Incoming后只改公开event字段复现，不伪造原始raw或授权。
- `p1-first-test.log` / `p1-first-exit.json`：更早第一轮6 pass/5 fail、exit101，原样保留。其中3项是同001产品失败，另外2项为探针fixture错误：v2合成权限漏必需title；对已有Incomplete视图调用finish误用unwrap。后续仅补合法合成title、明确断言finish返回Incomplete且sticky，未降低产品预期。该首轮误将外部tokio精确钉在1.53.3而使其从root锁1.54.1降级，旧锁另存 `p1-first-cargo.lock`；正式matrix/ordering已重新种子root锁并采用其同版本范围，首轮不冒充最终锁证据。

普通8组不能抹掉同版001失败；3个定向测试与8组基础测试分开计数，不用过滤失败签全绿。

## 已实际观察的存储与交付区别

默认生产上限下实际连续13000条小记录，多次FIFO后status末position=13000、retained=4096；此时完整snapshot必须再计4096个引用，真实返回Capacity。此分支只证明持续保存/尾位置推进，**没有证明首次订阅者能读到满条数窗口中的内容**，也没有按None key取单条的入口。完整delivery/首开无后续输出仍是后续义务，不因P1阶段范围缩掉。

另一路实际205帧各至少256KiB（仅payload已51.25MiB，超过3倍16MiB），预算FIFO后成功借snapshot核尾seq=205；这是字节上限路径可读出的证据，不能替条数满路径代签。合法ACP单条超过6000字节目标预算后Dropped seq1，下一小条成功seq2/position1；首原因和丢弃边界保持，finish拒绝Incomplete，原wire无P1自动控制消息。

独立预算数值样本：raw137，item=512+32+137=681；read=1536，per-frame workspace=768；该fixture真实峰值5042，相差1字节失败、恰好成功。跨旧read/snapshot/new代际的明确费用与最后Drop均实际核对，不把它签成所有heap故障可恢复。

## 变更时段归属

基础matrix完成后，root在UTC 01:16:57/58改写main/dev测试进行001窄修；新main变为0164d462...。本tester发现后停止启动后续检查，待父执行正式新冻结。后续第一批 `p1-regression.log`、`p1-negative.log`、`p1-clippy.log` 和 `p1-check-exit.json` 跨该变更时段：虽回归/格式/clippy exit0、负向9项拒绝，但这些结果**不归属任何完整固定快照**，不能代签初版或新修订。原日志不改；新冻结后全部必要检查重新执行。

内部u64极值/私有分配故障尚未从普通依赖触达，单列静态/未执行；全局allocator OOM保留abort边界。实际owner授权/公平调度、首次交付限制、实体current/工具anchor、JSON scanner、可信轮次分页、WSS/Web/真实Harness与Windows全树及完整L5/L6均未验。本轮没有产品修改、提交、发布或外发审计。
