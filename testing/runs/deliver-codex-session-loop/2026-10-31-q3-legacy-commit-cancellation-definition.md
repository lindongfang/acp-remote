# ACP-INTEG-LEGACY-CANCEL-001 最小独立定义

仅定义，未编码或运行；亲核工程补充 SHA256 `91c67e4632c7dab02d0d536dd02aeaeaa34e788605213e950936d6b7b3fa262c`。原ProductStore/独立C资产及历史失败字节保持；ACP v3/190源不改，不将115/120公开ACP绿代签根组合。下列四行共同构成同版门槛。

| ID/级别 | 操作步骤 → 预期结果 |
| --- | --- |
| C-SYNC01/L4 | 独立Fixture完成原target/proof后仅选CommitPending；用真正Box::pin拥有admit future，与本Store通知共同poll，先注册/enable接收或notify_one持permit防漏；确认实际所选分支到达且future仍Pending，drop整个Box拥有者 → 原faulted、持久count=0、随后真实连接BEGIN IMMEDIATE/ROLLBACK及原资源close断言全部保留，旧25ms不作阶段证明。 |
| C-SYNC02/L4 | 同法仅选AfterCommitPending；通知必须来自真实COMMIT.execute.await成功后且既有Pending之前，仍确认future Pending再drop拥有者 → 原faulted/count=1/真实BEGIN IMMEDIATE+ROLLBACK锁释放及close全部保留；若此时仍count0，登记真实产品失败，不放宽oracle。 |
| C-SYNC03/L3+L4 | 审读/编译cfg(test)端口：每Store独有通知，仅所选原Fault分支发，先前proof/control不留下假permit；正常构建字段/分支/接口均排除；操作若提前Ready、错阶段/漏信号必须明确失败 → 无生产yield/sleep/公共工厂、无只drop Pin引用或detach任务；防挂上限仅测试失联失败，不变产品期限、不串行整库。 |
| C-SYNC04/L4+L2 | 固定新源/端口/独立C候选SHA，逐hunk只改同步，不删计数/faulted/锁释放/close；先定向双阶段、再默认并发完整库及实际根check/build → 按真实日志逐项记录，原512首次失败确切落点仍未知，不把受控反例或单项绿冒充其恢复/组合已过。 |
