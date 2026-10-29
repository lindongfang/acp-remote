# implementation-r6 独立交付检视

日期：2026-10-30。结论：**PASS（本包变更及下述独立重验范围）**。未发现需修的新产品缺陷。不是整个 establish-node-access 的归档签署；当前用例最终覆盖由独立 tester 按当前 Steps/Expected 汇总，不能用本报告或开发全绿替代。CON001/LIFE001/SC001/SC002 与 I01 原关闭结论保持，不因本次测试资产修改重开。

## 版本及边界

- 独立计算 `2026-10-30-implementation-r6.sha256` 为 `1EB583A929AAB7769691FC05EF9A3A86D876E7502D94AC9CBC9F24D5EBCB10F7`；83项逐文件全部匹配，结束前再次复核0差异。
- 相对r5a清单，仅 identity.rs、pairing.rs、test_support.rs、Web main.ts 改变，新增 challenge-contract probe 的 Cargo.toml/Cargo.lock/README/src/main.rs 四项；其余原条目保持。
- 开发全量日志 `.tmp/implementation-r6-final-check.log` SHA256 `B7DD8FC0236A95180F7BCD56CAFEE23DF7F36708AC418DC8164C68DBEF1350AD`，实际记录 Rust127、向量4、Web46通过；交付exe独立核对为 `135BD9BDDABFDD530A5251AECB1DFF0DC640E81F80241EC53C8703C2336D1509`。这是开发执行，不称 reviewer 全量执行。
- reviewer 仅新增本报告；使用自有 `.tmp/reviewer-implementation-r4-target` 编译、测试，不改源码/规格/plan/tasks/其他报告，不提交推送归档，不操作30447或日常浏览器。

## NA009：写入及刷新前进程中止

`app/src/identity.rs:201–225` 的非test分支保留原 `write_all(&ciphertext).and_then(sync_all)`；新增两段写入和中止点仅在 cfg(test)。DACL先于密文写入、原错误删除本轮临时文件、flush后更名、正式身份校验顺序未变。`app/src/lib.rs:28–29` 将整个 test_support 模块限制为 cfg(test)，不存在普通构建故障环境开关或新产品API。

`identity.rs:348–366,686起` 的父进程先关闭SQLite，再启动当前真实测试exe的精确child入口；child使用真实 Windows DPAPI。父进程同时要求非零结束和 stdout `CRASH_POINT=对应点`，普通启动失败不能误算。辅助只输出 written/full长度，随后flush stdout再abort，不输出密文/密钥。

两新点实际检查：一个且仅一个非空 identity.tmp；磁盘文件长度等于child报告written；partial严格written<total、preflush为written=total；正式key不存在，重新打开SQLite后meta记录不存在。随后不清空目录，合法首次初始化，再次reopen比较nodeId及完整公钥稳定；marker原内容和旧临时文件仍在。最后显式storage.close、释放身份与fixture、断言自有目录已删除。不会以测试前清空目录掩盖损坏，也不把未提交临时私钥当既有身份恢复。

独立执行（真实用户、专有target）：

```text
cargo test --manifest-path app/Cargo.toml --target-dir .tmp/reviewer-implementation-r4-target --locked --offline --lib identity::tests -- --nocapture
13 passed; 0 failed; 114 filtered out
```

其中两个child入口在顶层无环境时只是返回，不能单独算成两个真实DPAPI场景；真实新进程/中止由父测试启动并核对。本组同时实际执行已有key-only恢复、meta-only/损坏/不符/未知schema拒绝与更名/meta阶段中止测试。新测试补齐当前NA009明确写入/刷新阶段的缺口；不要求硬件断电，不声称未sync时OS一定没有写盘，也不模拟所有文件系统故障组合。原来已提交身份不得替换的边界保持。

## 配对终态与竞争

对照reviewer保存的r5a扫描副本，pairing.rs在cfg(test)前产品字节一致。`pairing.rs:1355起` 通过真实 PairingService 生成 PendingClaim/PendingConfirmation，实际 reject/cancel/expire/approve/consume；终态重试后仅允许同终态或会话被清理，SQLite中的可信记录前后完全相等。Approved/Consumed建立信任，其他终态无信任；未认领不能approve。

`pairing.rs:1466起` 使用真实SQLite与tokio::join交换approve/cancel的初始poll顺序；按实际提交赢家检查cancel不得回滚已提交信任，或cancel先赢不得产生信任。这是同线程异步交错竞争，不是两个OS线程同时提交，也不能由交换参数位置保证每一种内部调度均发生；没有扩展声称覆盖无穷交错。

独立执行同target `cargo test ... --lib pairing::tests -- --nocapture`：**12 passed，0 failed，115 filtered out**。fixture显式close，未操作用户数据库。

## Challenge合同探针

审查并独立执行：

```text
cargo run --manifest-path testing/probes/establish-node-access-challenge-contract/Cargo.toml --target-dir .tmp/reviewer-implementation-r4-target --locked --offline
PROBE COMPLETE: 22 controlled challenge scenarios passed
exit 0
```

调用普通产品 ChallengeRegistry、真实ES256 sign/verify；仅时钟和随机源注入适配器。13种字段/用途变异拒绝后原同scope证明亦拒绝；认证14/15秒与墙钟回拨4组合、配对29/30秒回拨2组合，成功或失败后重复证明均拒绝；claim/status/auth三条签发入口实际随机源错误关闭。22为场景计数，不是22条独立断言。

明确范围：随机源恢复后代码只重新调用claim签发成功，并未逐条复测status/auth恢复；不扩展此声明。探针无HTTP/WSS/管理资格、真实浏览器、实时等待或系统随机源质量证明；不读写用户数据，不创建节点/后台任务，无秘密输出。README与实际适配边界一致。可供tester收缩NA004对应子断言，不能自动签署NA004全例。

## Web帮助及剩余范围

对照r5a副本，main.ts仅更换一行帮助文案（273）：若清除浏览器数据或丢失身份，先本机撤销旧设备再配对。该条件提示不假装检测旧身份、不增加按钮、不清理身份、不改变恢复调度或节点pin绑定。原存储丢失风险得到可执行指引；本轮未重跑真实浏览器交互，不把文字变更称为自动恢复能力。

没有新稳定问题ID；本包无新增需修阻断。r5a供应链检查可对未变配置/锁文件和历史范围复用，但不把旧当前文件扫描说成已扫描r6新增内容；本轮只完成源码/资产静态秘密检查，未另重扫r6。非交互CLI的r5a开发执行不是本轮独立执行，CLI字节未变，无新增验收声明。真实TTY、平台SID五命令/远程pipe等必要范围以当前tester证据逐断言判断；不因无关Web文字修改一律废弃未变平台证据，不恢复旧NA033/034签字要求。
