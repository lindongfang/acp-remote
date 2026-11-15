# 锁定Tokio版本归属更正附录

结论：**实际锁定Tokio为1.54.1，mio为1.2.4。先前将Tokio1.53.3称为“锁定版本”是本reviewer的文字归属错误。** 当前实际锁定源码仍支持原writer期限饥饿和attempt优先修复结论；既有真实红/绿与最终Root组合证据继续有效，无需因此回滚NativeDispatch编码门槛或无目的重跑全套。

本附录更正 `writer-deadline-starvation-static-review.md`（6d7ff061…）第19行及 `writer-deadline-coding-gate.md`（7b890579…）第23行所称“锁定Tokio1.53.3”，并适用于后续引用该版本归属的说明。旧报告、缓存和历史证据保持原字节。旧1.53.3的sleep.rs摘要不是本锁定构建所用文件的摘要，不能继续据它标注实际执行依赖。

## 实际锁与源码

亲读Root `app/Cargo.lock`：SHA256 `87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324`，与最终306清单及既有受测锁一致。实际重算Root306项全部相同，没有依赖升级或产品源变化。

- Cargo.toml的`tokio = { version = "1.53.3", ... }`是允许兼容版本解析的要求，不是`=1.53.3`精确锁定。
- Cargo.lock的tokio包明确`version = "1.54.1"`，crate checksum为`b52fd3255258d26fac7264ab5a76b53c13367136dc011d3d6ad4f8ed070396a3`。
- Cargo.lock的mio包明确`version = "1.2.4"`，crate checksum为`1788edb87fdc09c7e26304471e2f5be8cdefb1b6930d6e3985fc02ff53bf86ee`。

本次亲读primary缓存根`D:/Application/Rust/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`下实际版本文件，并核两个Cargo.toml的package版本。以下为文件内容SHA，不与上面的crate包checksum混淆：

| 文件 | SHA256 |
| --- | --- |
| tokio-1.54.1/Cargo.toml | 9a8189f0dc5320ae7c66786d6414b5650fcf1b1deab634135d2306bdc28d34ec |
| tokio-1.54.1/src/macros/select.rs | a1f6cb8d20a378653f2dda59d7137567b07896d76051c19e471812aab86e5229 |
| tokio-1.54.1/src/time/sleep.rs | 45d4145b73357ebf5d6d31b33be7125666d738defdfb17f9a6e30134725bd294 |
| tokio-1.54.1/src/task/coop/mod.rs | 7d36981992a2ee30d304364ce7d7cb474d85448723428a910a768cc646557904 |
| tokio-1.54.1/src/runtime/time/entry.rs | c15cb8d2b4ea82b15cd3a43d76c63297c45391fffb0bb5e1046133cc5d532525 |
| tokio-1.54.1/src/runtime/time/mod.rs | 975fccd02630e5f159f6570abefdc1d00b083bdd85ed8b4b4fefffd03cdc5e96 |
| mio-1.2.4/Cargo.toml | a8842f7fd53967d0f6b3faa359c4354c60ad6fd2927a5e0c4894933fdca55768 |

未访问网络、更新依赖或重跑供应链审计；mio此次仅核解析身份，不以其源码证明select分支公平性。

## 对原故障与修复的影响

实际1.54.1 `select.rs`的biased入口仍设`start=0`，在每次poll按顺序扫描，第一个匹配Ready分支立即返回（约667–735、800行）；该文件SHA恰与先前读到的select.rs相同。coop预算不足在扫描分支之前使**整个select**返回Pending；预算恢复后仍从0开始，不把优先权轮换给最后attempt。

`sleep.rs`约394–434行先poll_proceed，再初始化/轮询实际timer；timer Ready时made_progress。`entry.rs`约141–160行读取已完成状态返回Ready，`time/mod.rs`约437–438行对已经Elapsed的注册同步fire。因而旧writer循环不断重建过期sleep并把它放在attempt之前，可能反复完成deadline分支而仅loop；coop让出任务并不保证后面的attempt获得poll。此结论不要求断言所有线程绝对不能调度，也不把每次timer poll均无条件Ready作为公理。

原writer真正的期限/取消/终态收敛在attempt中；单纯deadline分支loop不能替代它。把attempt移为biased首分支，仍先在同原锁内检查真实终态、取消、期限，再按原围栏调用实际IO，解决被前置过期sleep饿死的责任路径；未改变Zero/Unknown/Written、实际poll锁或期限语义。此窄结论在实际锁定1.54.1 primary上成立。

## 动态证据继续有效的理由与限制

既有最小Pending0实际红、同字节probe转绿、WD/legacy29与最终Root六阶段均绑定同一87f27锁文件和实际源清单；执行走`cargo --locked`，不是以报告里写出的版本号选择依赖。Root package.json的check/build及归档run-owner脚本也实际指定locked；本次只读确认这些命令与锁身份，没有重新冒充执行。

因此这是静态来源版本标注更正，不是受测源码/依赖变化，也不撤销已证明的红绿行为、Root最终306组合或已关闭WRITER-DEADLINE问题。原`beb7675f…`报告的实际结果和范围保持。若未来锁文件或writer代码确实变化，再按新同源影响补验证；此次无需重复既有全套。NativeDispatch仍按其f9539c09…编码门槛进入实现，未执行的新NativeDispatch功能不借本更正附录冒签。
