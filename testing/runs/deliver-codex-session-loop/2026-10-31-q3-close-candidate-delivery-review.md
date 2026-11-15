# SQL关闭最终r3：独立修复子范围交付复核

2026-10-31。**最终candidate-v3-r3独立修复子范围PASS，可纳入9个固定验收文件及3处cfg(test)声明。REG-INTEG-SQL-001的产品修复已确认，最终组合关闭仍待根实际默认全库/工程终验；不预签正在运行的root检查。** 原172首次失败、旧v2/154报告及中间版本留原字节，旧六问题不自动重开。

## 同版输入亲核

- 产品清单 `f5f8b93b0879398bc0dbdbe7d4a792c30fc41e83105d077e282cd9b2af6ded59`，close模块c21e7ff0…；reviewer此前逐137核验，本次独立pristine137当前仍全部匹配。before/after无差异，锁文件87f27b61不变。
- tester v3-acceptance `5716dcb54c34220518714aedd3081103711d4930f123660fefd1ee55008383b2`，HANDOFF `66cf496accc47a4c6c21c6d0f71c1b1dd05b497bf49bed2448efc2e736308c63`，delivery-manifest `e637a5c997560b026dc5916ce58a45f0190772983c1a0390afa551ffe66e41e8`；22项证据全部同hash，已读原日志及实际退出0记录。
- portable-close-v2/assets `0c9a34a922157fb0f4ec8cc1262fe8656c21ee11561ca3e6eb2a79e7dbba9c34`，9/9文件匹配；完整新14用例及实际Node/私有槽/SQL authority/helper已亲读。3声明patch `5d3dda987e8cdfd326c263c045fe51de1dc803ab950bdb22e85aaebefc48cb14` 仅挂测试，原生产可见性不变。
- compat审计 `ae4299a2a03058b90a0ecc121ec7492cb4137570c03d9fc046243a7619218adf` 仅原F helper的at/resource加const，逐hunk无断言/DDL/body改变。9文件已包含该精化，复制后不能再重复apply审计patch。原v4字节保留，不回填旧154报告。

## 实际默认并发结果

executed-tests共168条、168个唯一完整函数，均能对应实际日志；新14和原154同最终r3重跑，未用r2开发绿代签。

| 实际范围 | 原日志结果 |
| --- | --- |
| F21/M7/C24＋新14 | samecrate-r3-default：66/66，默认并发 |
| 原公开队列/owner等 | public-r3-default：102/102，含原39/P51/OWNER2/factory/shutdown/restart/QEXT |
| 最终lint精化后的F21＋新14 | final-assets-default：35/35，默认并发；重跑不增加168计数 |
| 原F02/F03最小红循环 | tight-default-1/2/3：连续三轮各2/2 |
| 严格检查/可移植挂载 | 实际lib/tests Clippy及相对落点Clippy/fmt均退出0 |

命令没有以test-threads=1绕过组合并发缺陷。首轮style lint及旧红日志保留；最终变化是作用域/const/显式短锁drop等测试工程精化，最终35实际重跑。默认目标中的0测试binary未计入168。

## 六组关闭证据

| 定义组 | 复核结论 |
| --- | --- |
| G1 默认并发 | 原pair从红变为连续三轮绿，原154与新14默认通过；根完整组合尚须root另验。 |
| G2 真实物理关闭 | 正常24轮、node-owned真实BI/BA取消、Register/Begin/Settle前后提交错误，Storage.close后任何额外async I/O之前share_mode(0)通过；另连接核无operation/Applying/Settled事实。无删除retry/sleep、未吞Os32。 |
| G3 等待方取消 | BeforeClose与真实AfterAck两停点，实际公共Future首poll Pending；丢operation及首次close等待方后，同owner第二/第三close继续持责任。6秒受控负例越过旧5秒仍Pending，gate释放后真join/Complete。这里的6秒不是清理等待。 |
| G4 seal/槽/运行时 | acquire前lease不假0，seal后NodeStopping；真实32轮完成后正常reap移除已join槽、pending槽仍保留；真实RegistryConnection跨无ambient Tokio线程Drop后捕获Handle继续关闭。全future旧红/绿另属开发证据，未混写为独立例。 |
| G5 原事实与错误 | 真ack后模拟报告丢失，原register/Applying/Settled及actual owner原回执不变，同ID不二D，Unconfirmed不因重复close洗白，物理share0仍过。此hook不是伪造真实IO失败。 |
| G5 Node实际路径 | 2例调用真实NodeRuntime.start/shutdown及合成本地证书/目录/IPC：正常Completed对照、Unconfirmed产生固定静态Failed；真实SQL close Pending时原deadline失败，不报成功。没有新增public授权入口，Pending防御分支的不可达部分按静态分列。 |
| G6 同pool resolver | 真Authority查询同Storage max1池的clients/product_targets；Begin BI未放行前，driver失败观察与当前/其他key读取真实可达，失败先挂旧XY、新D不挂。没有拿旧静态F17替身代签。 |

Node消息不含SQL/正文/合成敏感标记；物理文件释放、事务rollback/commit事实、task实际join、Future内Weak分别证明不同层。没有用“调用任务结束”替代SQLite worker确认，也没有用内置5秒当关闭成功。新源码的首await lease、同锁转槽、取消安全句柄及最后完成围栏已由同源静态报告3a599bcd等证明。

## 问题状态与根终验

REG-INTEG-SQL-001（tester CLOSE-001）在此固定候选中的产品修复与独立验收子范围已通过，没有剩余候选源码/资产阻断。按已冻结a376及1511门禁，**根原失败路径的完整默认组合检查是最后关闭条件**；该条件实际通过后再记录问题关闭，不把root默认production build0等同功能终验。

根可精确纳入9文件与3声明，核新组合清单、原v4业务断言、锁文件及Raw无变化，实际执行默认完整owner check/build与显式fixture feature Clippy/restart。尚未执行的部分不由本报告代签；原172/e479失败证据须保留。

本次只修registry连接关闭责任。不签schema6/ACP writer、WSS鉴权、原生资源资格/终态关联、真实Harness或整体WP2/WP3。旧原始JSON基础层独立结论保留，真实组合仍按新清单另验；Web audit未外发。
