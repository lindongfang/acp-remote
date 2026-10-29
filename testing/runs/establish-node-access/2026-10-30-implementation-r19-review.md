# R19 writer先赢子场景独立检视

日期：2026-10-30。结论：**PASS（NA008发送先赢、撤销随后提交子断言）**。未发现新增需修问题。补齐reviewer此前revocation-gap-review G01中列出的该有限分支；NA008整例最终由tester汇总其余已执行断言，不以本报告直接代签整例或最新33例总表。

## 固定版本与变更范围

R19清单 `298EA007D4E1270E15D37DE93FC680A76A1121D452ECF2B96AD8F2E60B1B32C1` 独立计算，`.tmp/frozen-implementation-r19-20261030` **83/83逐条匹配**。相对R18清单，实际改动为management_ws.rs/node.rs测试辅助与回归，以及plan/tasks/review三份元数据；不是只把两源文件数说成全部清单差异。后续新勾选不包含在该旧83项中，须最终metadata清单另记。

普通运行时/Web保持R18语义；R18 late replacement caller的独立main适配器结论继续有效，不重跑未改浏览器链路。R19普通binary交接SHA256 `377C780CB3FFEBB9936BA131DC46955A2EF3E455134DA56DE1E49805786C38A7`，本轮已对frozen-r19/bin计算核对；不冒称本测试执行的是ordinary binary中的故障开关。

## seam与断言真实性

`management_ws.rs:1476–1516`：writer原先取得目标actor gate并检查当前资格，继续持有guard，新增WriteLocked栅栏在此后、sink.send前；成功计数仅在实际sink.send成功时增加，且仍在guard生命周期内。新增hook/计数/访问方法均cfg(test)，无环境开关，不改普通帧/字节预算、认证或撤销顺序。node新增测试位于测试模块。

`node.rs:2214–2276`：真实NodeRuntime、SQLite、配对和TLS/WSS连接；发送node.info，等待writer锁内栅栏命中。真实owner IPC revoke future被持续poll50ms仍未完成，同时实际SQL目标状态trusted、发送计数未增。释放writer后revoke在2秒内成功，发送计数恰+1；真实WSS先收到对应requestId合法成功result，再Close4410；最后runtime.shutdown成功。

50ms是实际待决观察，不是虚拟时钟或源码常量推算。它**没有额外记录撤销任务精确进入lock().await的时刻**，单凭“50ms未完成”不能证明每种调度下已经到达锁；本结论结合源代码对同client gate的检查、writer已持锁和成功计数仍在锁内、释放后的实际响应/撤销顺序判断，不冒称所有调度被枚举。没有据此新增无限排列或强制新的测试入口门禁。

测试SecretProtector为fixture，不扩大为本轮DPAPI检查。真实socket result先于close支持在途字节不能回收的合同：发送先赢的已交网络栈结果不因随后撤销而要求回滚。已有撤销先赢/已入队丢弃回归按R9相同生产逻辑复用，本轮不重复冒称已重跑它们。

## 独立执行

真实当前用户，reviewer专属target，冻结源编译：

```text
cargo test --manifest-path .tmp/frozen-implementation-r19-20261030/app/Cargo.toml --target-dir .tmp/reviewer-implementation-r4-target/r10-build --locked --offline --lib writer_holding_the_actor_gate_finishes_before_a_competing_revoke_commits -- --nocapture
1 passed; 0 failed; 143 filtered out; exit0
```

本轮未跑全库/Web，未把主方144库+4/Web50完整check归为reviewer执行。用户已交原tester独立补验，原tester的整例通过状态与本报告分开。

只新增本报告并维护active review索引，不改产品/冻结源/规格/旧报告/清单/plan/tasks，不操作用户30447、日常浏览器或用户身份，不提交推送归档。
