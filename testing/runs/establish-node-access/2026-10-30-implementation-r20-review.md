# R20批准决定等待中断修复独立检视

日期：2026-10-30。结论：**PASS（源码及恢复helper限定范围）**；**NA007-CANCEL-001真实TTY失败尚待原tester新binary动态复测关闭**。原 `approval-cancel-boundary-acceptance` 的R17失败原文保持，不以本报告改成历史PASS。

## 版本与范围

独立计算R20清单SHA256 `73727BCF617F3F96BCC24C4CBB7B38BA044A56856C6CB4D628869832C8B368A4`，对 `.tmp/frozen-implementation-r20-20261030` 83项逐条匹配。main.rs为 `D8A71C1D21E7595ECAE1E202D03351524252B1DDDF4AE5DA13B5A72439192D5E`。没有编译/运行实施中的工作区代替冻结源，也没有把后来metadata写入包含在旧清单里。

正式普通exe先复制到reviewer自有 `r20-bin/acp-remote.exe`，摘要 `561A6024E983DD5239C09D1388CA5D256625F690EE9345E64B9654BC43AAFBE3`独立匹配；本轮不把复制核对当真实TTY执行。开发全check日志已读，144库+4/Web50，SHA256 `A431B4848AEF121237C380AF74792B5FC69B8116E38206A878DD95F45D1ABD75`，不冒称reviewer全量重跑。

已读独立真实TTY失败报告：approve确已COMMIT/审计存在但批准事件被proxy持住；Ctrl-C默认退出且无revokeHint，信任仍在、同key浏览器恢复通过。其问题是提示/等待路径，不是服务器错误回滚。此次报告仅评估CLI修复，原tester正控制/predecision/committed三分支仍须分开验证。

## 源码判断

`main.rs:210–228`：decision发送后改用已有wait_for_pair_event，它select真实Ctrl-C与配对事件。None分支先drop专用配对stream，再报告中断并查询，不能将关闭stream等同撤销已提交信任；未批准任务由原服务端断连取消路径收敛。没有新增网络批准、IPC action、权限或删除信任。

`main.rs:244–296`：在任何查询前打印并flush“decision interrupted / approval may already have committed”、device list和条件revoke指引，故查询不通或超时时操作者仍有恢复信息。唯一5秒timeout包住**整个find_trusted_fingerprint分页循环**，不是每页重置的5秒。只调用现有owner device.list，逐页查status=Trusted且公开指纹精确相等；不按displayName猜身份，不在未知状态打印确定批准，也不把列表空/查询失败叫作取消成功。匹配后复用原正常clientId及完整revoke命令输出。

status分页可能遇到revision失效、节点不可用或在5秒后才完成真实批准，这些只会得到“未确认/无法检查”；提示要求重新list，未虚报取消。查询结果是该次观察时的trusted状态，不保证之后无人撤销。第二次Ctrl-C如何处理、发送decision期间系统故障等不是本轮新动态声明，须按真实TTY测试实际结果记录，不扩写成所有Ctrl-C排列已通过。

资源核对：PairingSession直接拥有NamedPipeClient，先drop流后才另开owner list，未在配对stream/actor锁内等待查询。ipc::request的pipe/exchange future均为局部所有，没有为查询spawn游离任务；外层5秒timeout取消分页future时会丢弃当页client，也覆盖其原30秒短请求等待，不累计N页×30秒。同步open未新增WaitNamedPipe或阻塞重试。此为源码取消/所有权结论，不伪称本轮实际观察Windows句柄计数。

## 独立有界helper执行

从冻结main.rs**原样提取**print_approved_client/report_interrupted_decision/find_trusted_fingerprint函数，不改其判断或timeout。使用真实Tokio和真实产品AppError/ClientSummary类型，仅owner IPC transport/page响应以明确适配器替代；stdin编译至reviewer专属target。没有用户节点、数据库或TTY。本轮不把它称为普通CLI实际接收Ctrl-C。

实际执行四场景，exit0，并独立核对输出：

1. 第一页只有同指纹revoked及其他指纹trusted，第二页才有匹配trusted：确实查两页，只打印正确trusted-match ID及完整撤销命令，不误认revoked/其他key。
2. 列表空：仅“Approval was not confirmed”，不打印确定trusted。
3. IPC错误：仅“could not be checked”，不把错误透出为虚假取消或批准。
4. 请求永久pending：真实计时达到5秒（断言>=5且<7），总预算终止等待，仍只打印无法确认与恢复指引。

另断言fingerprint=None时不调用owner list即返回未确认。四输出均含先前诚实中断提示和device revoke指引；只有场景1含确定trusted句，无场景打印cancelled before approval。

产物：`.tmp/reviewer-implementation-r4-target/r20-helper-review.exe`及`r20-helper-review.log`（SHA256 `01128DC08E741CDE396ECDE35B2AC04CC23F1B7ACE775F89575ADE1B440F300B`）。首次输出核对脚本误将PowerShell Tee的UTF16日志按UTF8解析而失败，改为实际编码后四分支输出断言通过；不是产品/函数执行失败。helper仅使用公开合成ID和owned-fixture.toml占位路径，不输出配对秘密；没有通过模拟COMMIT或模拟Ctrl-C关闭原真实TTY问题。

## 剩余闭环条件

原tester需用固定R20普通binary、真实owner透明proxy，复测已COMMIT但final事件未交付时Ctrl-C：退出可控且先有诚实指引，额外owner list能确认时显示匹配ID/正常revoke命令；未提交/无匹配及list失败/超时不能虚报成功/取消。信任保留/浏览器同key恢复可按未变负责字节及实际断言范围复用，不要求无关全链无限重跑。proxy应支持额外list请求，否则只能验证“查询失败”分支，不能冒称已验证确认批准分支。

本轮未重跑完整Rust/Web套件，不把Clippy或开发全绿当真实TTY闭环；没有新增需修源码问题。R17/R18身份与R19 writer子项、SC003资源嵌入和NA002供应链范围结论保持。维护active review索引时明确R20是新的普通CLI运行时变更，原tester不能拿R19 binary代验该修复。

只新增本报告和reviewer自有适配产物，未改产品/冻结源/规格/旧报告/manifest/plan/tasks，不操作用户30447或日常浏览器，不提交推送归档。
