# r5a 独立定向验收

状态：本轮r5a定向完成并清理。真实TTY回归、CON001生产解码回归、原Ctrl-C退出、50条审批/裸Drop断言和默认关闭预算均实际通过；NA-016按当前原文整例PASS。其他未完成用例保留，不代签reviewer或整体验收。

## 版本

- 清单`2026-10-30-implementation-r5a.sha256`，79项，SHA-256 `96F7B25DA1B7ABF3C5F65D069723CA87B095800F08967720AADBD4FC621975EC`，独立逐文件79/79匹配。
- 用户给定主binary摘要独立核对为`2DC0AFA04B55376797587136048F5EFBE7D665FF7EE69525DA248FA4D73305F2`，已复制至`.tmp/approval-r5a-acceptance-20261030/bin/acp-remote.exe`再次核对后运行，避免占主target。
- 运行材料全在新owner-only.tmp根；独立30451/30452，保留用户30447。开发检查日志只按可读原始范围核对，不代替本轮动态行为。
- r5-product候选先前node.rs测试行与摘要不符，已按主流程通知保留为未验收候选，不计产品bug。本轮只以r5a为准。

## 实际结果

## 真实TTY与CON001

每例新配对由真实owner TTY创建，独立客户端以实际HTTPS/P-256/JWS认领。终端显示名称/公钥指纹/到期后才发送输入，随后查询签名终态并建立真实WSS持有证明。测试使用已核对并复制的r5a binary；客户端私钥仅留内存。

| 输入 | TTY实际结果 | 协议实际结果 |
|---|---|---|
| 原Ctrl-C | 约0.106秒会话结束，无额外Return；取消后无自有pair进程残留 | cancelled、clientId=null、管理认证4401 |
| y + Return | PAIR_EXIT_CODE=0，实际退出 | approved、authenticated |
| yes + Return | PAIR_EXIT_CODE=0，实际退出 | approved、authenticated |
| n + Return | PAIR_EXIT_CODE=0，实际退出 | rejected、null ID、4401 |
| 默认Return | PAIR_EXIT_CODE=0，实际退出 | rejected、null ID、4401 |
| n + Backspace(U+0008) + y + Return | 实际控制台退格回显，PAIR_EXIT_CODE=0 | approved、authenticated |

Ctrl-C同时传播到PowerShell，工具会话退出1；未把工具码伪写为产品子进程码0。原NA-PROTO-001要求“取消后真实进程退出、不靠额外Return”，当前binary再次满足。旧r3红灯及r4复测报告不改写。

独立精确执行`platform::console::tests::repeated_and_unsupported_console_text_cannot_collapse_to_y`：1项、退出0；实际调用生产decode_text，覆盖repeat=3的y、有效UTF16代理对、孤立代理、repeat=0不能折叠为y/yes，普通y仍保留。该项是明确的输入记录适配器测试，与上述实际TTY分开；没有声称已向真实Windows控制台注入wRepeatCount或孤立代理记录。准备的console-inject.ps1本轮未执行，不计通过。

原始六份`tty-*.json`及`console-unit.log`位于`.tmp/approval-r5a-acceptance-20261030/`。

## 审批门及LIFE001裸Drop

独立工程`testing/probes/establish-node-access-approval-independent/`在tester自有target以r5a公开产品API重新编译，build退出0；重新运行原40条及新增裸Drop前提/边界，共50条PASS断言，退出0、PROBE COMPLETE。

原40条全部重跑：真实TLS配对/owner IPC明确批准，真实AuthClient签名获取opaque actor，真实SQLite/current-user DPAPI及共享gate；缺失/过期/未来但无issuer/故障clock均拒绝，dispatcher累计0。实际runtime gate被可控clock暂停时owner revoke等待，释放后Unverified且无派发，随后撤销提交并使actor失效。lease Drop、4100次取消、作用域隔离、重放、未知/撤销key、正常stop保留policy/actor和跨节点拒绝均通过。假能力与受控clock是明确适配器，不启动真实Harness或伪造grant。

新增LIFE001动态路径：新建另一个真实NodeRuntime、再次实际HTTP/owner IPC配对，取得当前有效actor并准备尚未提交的真实签名challenge；保留原及with_clock policy、actor、lease时**直接drop(runtime)**，不是调用正常stop。尚未yield时issue已同步NodeUnavailable；准备好的有效proof、原本有效actor及派生policy均NodeUnavailable；能力请求dispatcher仍0。释放外部句柄后实际端口重绑、目录锁acquire/unlock成功。此路径覆盖新Drop同步关闭control的修复，不拿正常stop代替裸Drop。

证据`policy-final.log`。测试TLS证书为本轮自签名localhost并被客户端精确信任，不关闭TLS验证；不冒充L6受信浏览器闭环。

## NA-016 独立默认deadline回归及整例判断

独立命令：

```text
CARGO_TARGET_DIR=<tester独立target>
cargo test --manifest-path app/Cargo.toml --lib --locked --offline node::tests::owner_stop_reports_storage_failure_at_the_real_fifteen_second_deadline -- --exact --nocapture
```

实际1项执行、0 failed、123 filtered out、退出0。日志打印：

```text
configured_shutdown_ms=15000 cleanup_budget_ms=14000 observed_ms=14010
```

本tester独立读取并执行断言，非复述开发报告：

- 以真实DpapiProtector启动真实Windows NodeRuntime，通过真实owner IPC发送stop。
- 仅通过明确的测试Storage.pause_next_close屏障挂起真实Storage.close；不是普通正常关闭或缩短budget。
- stop到达节点、Storage.close确实进入屏障，关闭报告先保持Pending。
- 默认生产15000ms总预算给IPC留1000ms，因此约14010ms清理失败；owner stop必须返回错误且包含closing storage，shutdown也返回错误并指明15秒总预算。
- 关闭报告实际为Failed，包含closing storage未完成项，没有报告stopped/全部释放；节点尽力清理已拥有资源。
- 报错后实际数据目录仍存在，未通过删除身份掩盖失败。随后测试夹具自身清理不等于产品错误路径删除数据；测试及本轮结束后无自有节点/pipe/锁残留。

当前NA-016原文是“用明确故障注入入口阻塞在途工作**或**令必要提交失败，触发stop并观察15秒期限”。本例实际选择阻塞Storage.close分支；期限耗尽、明确未完成项、无假成功、尽力清理和保留数据均有上述断言。**整例判PASS**，无需另外要求所有保存失败或无穷并发组合。精确边界是默认总预算下约14秒cleanup失败，不宣称实际等待满15秒，更不把缩短250ms测试当本次证据。

## 版本与逐例收缩

- access_policy/挑战/证明/trust负责字节与r4一致，审批探针仍重新编译并实际重跑，不因未变就假定通过。
- r5a主方原始`implementation-r5a-check.log`已读回124库+4JWS+46Web通过，作为同版本工程证据复用；本轮自己的精确测试和动态probe另有上文实际结果，不以主自测代签。
- 原r4阶段9PASS/24PARTIAL中，当前NA-016从PARTIAL转PASS；NA010/030/020继续由本轮动态证据支持。当前33例阶段计数为 **10 PASS / 23 PARTIAL / 0 NOT-RUN / 0 tester已确认未闭FAIL**。PASS为NA001、010、011、016、018、019、020、025、030、033；其余23例保留前序报告的具体未执行断言，不把本轮子项扩写为整变更通过。
- CON001在本轮的生产解码/普通真实TTY范围、LIFE001裸Drop、原NA-PROTO-001均动态满足预期；独立reviewer是否关闭其问题由其自身判断。本报告不代签secret扫描/供应链检视。
- NA004新版本化probe仅完成阅读，尚未独立运行，不改变本轮NA004的PARTIAL。按主流程安排在本轮清理之后单列结果，不阻塞identity故障钩子的下一包。

## 原始证据摘要与资源清理

```text
B2FB24E8FBFE7F950BDC461E593A760491A1EBA9C7F3F6526FF3B5446CEEE43F policy-final.log
8314A55C13DCE1ECFC89F529EFD6081B0E41A08131D739076AE7081178C96883 deadline-test.log
F29F963654BC653464D6E7334FEBF8BED80DBDEAA10421F17253EBEC3945E70F console-unit.log
```

- 自有节点PID39036 owner stop退出0，所有pair/审批probe原生进程数0；30451/30452无LISTENING，用户30447仍由PID31228监听。
- tty-data、policy-final/data、other-node-data、drop-node-data四个锁均FileShare.None独占重开，专属pipe均不存在。裸Drop和正常stop均各有资源释放证据。
- 运行根owner-only；target/证书/DPAPI密文/SQLite/JSON/日志均留.tmp，短码pair-input.json已删除。没有操作用户真实密钥、数据库或日常浏览器。
- 开始及清理后清单自身摘要相同，均79/79匹配、0 mismatch。初次PowerShell摘要脚本有语法错误，已修正重新完整校验；该工具错误不记为产品失败或成功证据。
- 本轮只修改自己的探针/报告，不改产品、tests预期、规格、tasks/plan、清单；不提交/推送。当前仍有23个必需PARTIAL，不能据本报告宣布待归档。
