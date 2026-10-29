# R11/R12 独立定向交付检视

日期：2026-10-30。结论：**PASS（R11容量/时钟测试及R12配对修复限定范围）**。NA-022-001实现修复已由reviewer独立复核；原tester原HTTP失败报告保留，HTTP复测和整例状态由原tester另行签署。本报告不改其历史FAIL。

R12清单自身SHA256 `6D7E0EC6CC1FD9DD6AC0F07719841AF1B0736F81EB265B7E5C0ACDB3DE6E409B`，83项当前逐文件匹配。使用用户交付 `.tmp/frozen-implementation-r12-20261030/app`，关键pairing.rs `D9EC7C80E5D244460301B5ACD2B966076A4D4D1E4856EE2AFEC2E6DC8E418338`、trust.rs `CABDFB0166D1E4DFEE646A347039B9EB1F5DD69F06413441141D1C72DD6CFBB7`与清单一致；generated dist另作为编译资产，不称83项原有内容。reviewer target独立，不改产品/旧清单/失败报告/plan/tasks。

## R11限定复查

R11清单自身摘要 `6BAB1B83EC3508AE5F0957C9BE97655918FBBBF8A0E8D6960F334561F8EDEAFD`已核对。R12 frozen trust字节匹配R11条目，本轮不把R11其他文件强行称为当前全匹配。

实际独立执行 `trust::tests::full_revoke_budget_preserves_results_until_the_exact_clock_deadline`：**1 passed，0 failed，139 filtered，exit0**。使用真实SQLite/TrustStore，仅时钟控制：先真实revoke一条，再SQL夹具填9999条未来未过期结果；86399秒旧请求返回原响应、不同载荷冲突、新请求CapacityExhausted且10000条不被驱逐；精确86400秒仅旧真实结果过期，复用原ID撤另一目标成功，结果仍10000，重复响应相同且两客户端仍revoked。未修改expires_at模拟这次时间推进；SQL夹具不冒称10000次真实协议受理。无生产修改，无宿主磁盘满声明。这关闭revocation-gap-review的G03容量/24小时联动缺口，WSS只读/审计等其他断言不受此自动关闭。

## R12配对修复

`pairing.rs:386–415` 在bound缓存重试之前要求PendingConfirmation，且该检查位于registry同一锁内、prune之后。原proof仍先经过真实验签/字段解析；pending同字节重试仍得原hostProof，不重新claim。Rejected/Cancelled/Expired/Approved/Consumed不能通过旧claim缓存取成功；status的批准/consumed恢复保留规则未被误删。原失败是终态接受claim，不是已批准信任被撤销，修复保持二者分离。

独立从冻结副本编译并运行：

```text
cargo test --manifest-path .tmp/frozen-implementation-r12-20261030/app/Cargo.toml --target-dir .tmp/reviewer-implementation-r4-target/r10-build --locked --offline --lib pairing::tests -- --nocapture
15 passed; 0 failed; 125 filtered out; exit0
```

新增测试实际覆盖：五终态原claim拒绝；pending精确重试仍成功；299秒签发新的status proof，在300秒自然过期后旧lookup/claim和仍在挑战自身寿命内的status proof全部拒绝且无持久信任；批准在0/240/299秒的结果保留终点分别300/300/359，签名结果expiresAt一致，错key不能查询；consumed在deadline前1秒返回原clientId，deadline及之后新/旧status拒绝但同公钥持久clientId保持。真实产品服务+ES256+SQLite，clock是明确适配器，不声称实际等待359秒或真实浏览器执行。

同组既有批准/取消竞争、终态信任不变、撤销key不可新建身份也通过。没有修改产品等待时间、挑战期限或增加普通构建故障开关。

主方完整日志 `.tmp/implementation-r12-check.log` 摘要 `6E6835B5207ABDFF5878183510133F38203D9CE994D32A71554072B7FDA8F347` 已读，确有140库+4向量/Web46；binary `92D9D759D4D6EFE5806338FB1E3DF0FA996CCA10F4766D9E6CC4E14484A2DD44`独立核对。全check/build为开发执行证据，不冒称reviewer全量。实际定向执行为上述15+1。

未发现本包配对新缺陷。保留原 `2026-10-30-na022-independent-acceptance.md` 和NA-022-001失败依据，由tester在新binary另行确认HTTP终态不再200。R9/R10 management结论不因本包变更重开。另NA002总检视新发现的debug Web资源交付配置问题NA-SC-003与本包配对修复分开，不能由15项通过覆盖。
