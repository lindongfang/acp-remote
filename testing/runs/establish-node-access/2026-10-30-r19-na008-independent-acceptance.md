# R19：NA008唯一writer先赢边界独立验收

**NA008 PASS。** 当前有限Steps/Expected中的最后一条writer先赢受控次序已经实际通过；结合既有受理/缓存/入队/撤销提交/队列失效证据，原PARTIAL可以关闭。**tasks2.4完成判据全部满足，可由主方勾完成**；本tester不修改tasks或plan，不代签reviewer。

统一总表更新为 **27 PASS / 6 PARTIAL / 0 NOT-RUN，共33例**，剩余007/012/013/026/031/032仍不得当通过。本例通过不意味着全change可归档，不要求管理027–029重跑。

## 固定范围

- 冻结源 `.tmp/frozen-implementation-r19-20261030`，83项manifest自身SHA256 **298EA007D4E1270E15D37DE93FC680A76A1121D452ECF2B96AD8F2E60B1B32C1**。执行前后本tester独立核对 **83/83，0 missing/mismatch**。
- source关联R18 generated Web dist单列，不冒称dist是83原始条目。本轮没有浏览器执行，不将R18/R19页面声明为重跑通过。
- 用户交付ordinary binary **377C780CB3FFEBB9936BA131DC46955A2EF3E455134DA56DE1E49805786C38A7** 已独立Get-FileHash匹配；**未运行该ordinary binary**。实际执行对象为本tester从冻结app编译的cfg(test)库测试exe，二者明确分开。
- tester CARGO_TARGET_DIR `.tmp/approval-acceptance-20261030/probe-target`，TEMP/TMP `.tmp/independent-na008-r19-20261030/temp`；Windows真实jack上下文显式require_escalated。未占主target，未启动或操作用户30447/浏览器。

## 实际命令与结果

```
cargo test --manifest-path .tmp/frozen-implementation-r19-20261030/app/Cargo.toml --lib --locked --offline node::tests::writer_holding_the_actor_gate_finishes_before_a_competing_revoke_commits -- --exact --nocapture
```

**exit0，1 passed、0 failed、143 filtered，测试耗时0.16秒**，编译11.68秒。不是零匹配，也不是主方完整144库+4/Web50结果代签；本轮未重跑完整check。

测试实际使用普通NodeRuntime、真实SQLite、owner IPC撤销以及真实TLS/WSS客户端。测试SecretProtector为明确FakeProtector，证书为测试trust root，writer暂停与计数为cfg(test)适配器；不声称此测试覆盖真实CurrentUser DPAPI或宿主受信证书。端口为TestArea自选空闲接口，不使用用户或其他tester固定节点。

## 精确测量边界审查及通过事实

1. 已核对WriteLocked seam位于writer取得actor gate并通过still_valid之后、`sink.send`之前；`_gate`所有者仍在作用域内。不是旧pause_next_write在取得gate之前的暂停。
2. 真实管理连接已认证，发出带唯一requestId的node.info，等待barrier entered，明确命中持gate状态。
3. 构造并通过 `timeout(50ms, &mut revoke).await` **实际轮询**owner IPC revoke future。它未完成，而不是只创建未被poll的惰性future。
4. 持gate期间真实SQLite查询仍trusted，protected_write_count与before相同，避免把已经提交或已发送的状态当“等待”。
5. 释放barrier，writer实际 `sink.send`成功后，cfg(test)计数在仍持gate作用域内增加；随后owner revoke在2秒界内成功，确认COMMIT。计数精确before+1。
6. 客户端随后实际读到原node.info成功、原requestId，再读到Close4410。此处不依赖客户端观察时间来反推内核发送或COMMIT顺序，顺序依据服务端gate所有权和成功send计数。
7. `runtime.shutdown(None)`成功，测试进程exit0，TestArea自有数据目录全部清理。

原先的有限缺口是“writer先赢与竞争撤销的受控反向次序”，本次恰好补足。没有新增无限调度/每个并发命令必须success/追回已交网络栈字节等门槛。

## NA008整例有限对应

| 边界 | 已采纳实际证据 |
|---|---|
| COMMIT先于受理 | 原reviewer独立Admission栅栏，撤销后admitted不增、enqueued不增、4410 |
| 普通动作完成后，COMMIT先于入队 | 原reviewerEnqueue分支，admitted+1、撤销后enqueued不增 |
| 真实幂等缓存命中后，COMMIT先于入队 | 原reviewercached分支实际命中原revoke结果，再撤销actor、释放栅栏，无入队 |
| 撤销失败 | 原storage语句/实际COMMIT失败回滚；本testerR17真实WSS失败，B资格/revision/audit/result不变，两条B仍有效 |
| 已入队/出队后，COMMIT先于writer gate/socket提交 | 原reviewer独立queued-result writer栅栏，只见4410；R17两个订阅B并发收敛、原key新证明4401 |
| writer持gate先完成send，竞争revoke随后COMMIT | **本轮R19精确测试PASS** |
| 多连接订阅/操作、队列与身份关闭 | 本testerR17普通binary真实两B订阅/并发命令、两连接4410、A自撤销及owner恢复；不重复执行该未变生产路径 |

R19仅新增cfg(test) seam/计数/测试，未改普通受理/队列/发送路径。原RV001/002关闭继续有效，本报告不重新打开或代原reviewer关闭别的问题。当前tasks2.4原判据NA006/008/023/027–030全部为PASS，且提交后不新发送由上述门/队列及实际连接证据共同成立，故component已满足；不等待其他component的NA007/012等才允许勾选。

## 清理与证据

实际TestArea正常shutdown，测试进程查询为0，自有TEMP条目为0后仅删除该空目录。没有自建浏览器/永久daemon；没有额外声称逐个随机端口枚举或exact pipe存在前后采样已执行，清理依据测试正常shutdown、进程退出、TestArea目录为空。未动其他target、data、用户配置或进程；未修改冻结源/产品/规格/清单/tasks/plan。

- 原始日志 `.tmp/independent-na008-r19-20261030/writer-first.log` SHA256 **682FEAE4C9DEE8E876A3485338F15C057D6B776A6AE2CD1C77B8C33747B3F41B**。
- 实际测试exe `.tmp/approval-acceptance-20261030/probe-target/debug/deps/acp_remote-266e97892ace37dd.exe` SHA256 **FDA853465E831ACEA58B653C66AE88666355C5F2876F2543724F4D5457B3E8A3**。
- 无确认产品失败；未提交、推送、归档。
