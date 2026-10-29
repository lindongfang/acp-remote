# r6 独立定向验收及 r7 SQLiteFull 补充

结论：**NA009、NA005 PASS；NA004 保持 PASS；NA007、NA022、NA027 仍 PARTIAL，但缺口已缩小。** 阶段逐例合计14 PASS / 19 PARTIAL / 0 NOT-RUN / 0本轮确认产品FAIL，不代表全部验收完成，不能归档，不代签reviewer。

## 固定版本和实际执行

r6清单 `2026-10-30-implementation-r6.sha256` 自身 SHA256 **1EB583A929AAB7769691FC05EF9A3A86D876E7502D94AC9CBC9F24D5EBCB10F7**，独立检查83/83，0 missing/mismatch。二进制 **135BD9BDDABFDD530A5251AECB1DFF0DC640E81F80241EC53C8703C2336D1509**，核对后复制到 `.tmp/independent-r6-20261030/bin/acp-remote.exe`，没有运行/锁住主target中的exe。

本轮运行材料仅 `.tmp/independent-r6-20261030`，CARGO_TARGET_DIR 为既有tester独占的 `.tmp/approval-acceptance-20261030/probe-target`。Windows真实权限的DPAPI/进程中止/实际节点探针均显式 require_escalated，使用jack上下文。原测试只控制时钟、随机源和cfg(test)中止位置；没有把中止钩子作为生产故障API。

| 本轮独立命令（均加 --locked --offline） | 实际结果 |
|---|---|
| cargo test --manifest-path app/Cargo.toml --lib identity::tests:: -- --nocapture | exit0，13 passed，114 filtered，16.81秒；其中两个child入口在父进程运行时无环境直接返回，不把它们另算两次故障验证 |
| cargo test --manifest-path app/Cargo.toml --lib pairing::tests:: -- --nocapture | exit0，12 passed，115 filtered |
| cargo test --manifest-path app/Cargo.toml --lib storage::tests::rejects_an_unknown_schema_without_modifying_data -- --exact | exit0，1 passed，126 filtered |
| cargo run --manifest-path testing/probes/establish-node-access-challenge-contract/Cargo.toml | exit0，22受控场景通过；真实产品注册表和ES256，仅clock/random为适配器 |
| cargo run --manifest-path testing/probes/establish-node-access-r6-independent/Cargo.toml | 修正自有fixture后exit0；普通NodeRuntime、真实DPAPI/SQLite/owner IPC/HTTPS，本机30452，不是浏览器 |

另核读 `.tmp/implementation-r6-final-check.log`，确认主方全检查包含46 Web、生产构建、fmt/clippy、127库+4向量，而非零目标。该日志为同版工程检查复用证据，不冒称本tester重新执行了全量命令。本轮实际独立执行范围如上。

## NA009：PASS

已审查并实际执行两个新增crash点及原全部身份断言：

- partial-key-write：真实DPAPI密文写入正数前缀后子进程abort；父测试必须读到确切CRASH_POINT及IDENTITY_WRITE_BYTES=written/total，临时文件长度等于written且0< written < total。
- before-key-flush：完整密文已经write_all但尚未sync_all即abort；临时文件长度等于written=total。没有把“写完未flush”声称为物理介质持久化证明。
- 两者正式identity.key不存在、node_meta不存在、恰一份临时密文；这是尚未提交的新身份，不是损坏既有身份后的替换。正常首次初始化后再close/reopen，nodeId/publicKey稳定；原keep-user-marker内容和中止临时文件均保留，未清空目录掩盖故障。
- 新测试显式Storage.close，drop后断言自己目录不存在。子进程确实非成功退出且到达指定abortmarker，不以异常启动失败代替中止点。
- 原before-key-rename、after-key-commit、before-meta-commit、after-meta-commit中止均重跑通过。正式key已提交但meta未提交时恢复同密文，不生成替代key；提交meta后重开同key。
- 只有key可恢复；只有meta、key/meta不符、非法私钥、已有历史却key/meta均缺、未知schema均封闭失败。真实DPAPI跨新进程恢复通过；另独立运行未知SQLite schema测试，保留sentinel而不降级清库。

负责字节：identity.rs **8EE7C63F149B689F70324C6C0962C5F8DD303E2E76A86BA6DB9E341F841BF600**；test_support.rs **FF04D6730B1C0020CC205A381A6FD45F101A616EFBE4A91C9BD6C4116531270A**。生产原write_all.and_then(sync_all)路径与test-only分段写入明确区分。本例没有要求断电设备实验，不额外增加该前提。

## NA005：PASS

12项真实SQLite配对测试补齐当前有限步骤：

- 299秒仍可lookup，300秒失效，lookup不续原expiresAt；严格Crockford大小写、中间连字符及O/I拒绝；8个活动请求、第9个拒绝。
- 首次码碰撞重生成且不替换首请求；持续碰撞32次封闭失败，明确有界。
- unclaimed/claimed两个起点，未认领approve失败；rejected/cancelled/expired均覆盖两个起点，approved/consumed仅合法的claimed起点。各终态之后再次approve/reject/cancel/expire不能复活或改变已提交信任。
- tokio::join两种起始顺序实际调用approve/cancel、真实SQLite查询权威结果；根据实际提交赢家断言取消后无信任，或已批准不可回滚。它不是声称穷举操作系统所有调度。
- 已批准结果可用真实签名status取回稳定ID；consumed及重试不改变已提交信任。原restart/clear使临时材料失效测试也重跑通过。
- 新独立普通产品探针经真实HTTPS lookup/claim和owner IPC批准，在issued、claimed、approved阶段读取SQLite主文件及WAL，确认配对码、完整proof、nonce/challengeId/pairingId的原始/编码值不落盘（不输出这些值）。检查范围限定数据库文件，未把DPAPI身份key当配对秘密。
- 新r6复制binary实际非交互device pair返回exit1和“requires an interactive terminal”；在活节点上再次执行也相同。结合当前命令入口在IPC创建前检查stdin/stdout TTY，确认不创建待确认请求。

pairing.rs负责字节 **C73F06D8187272C5E465B37AB3F97C6005BE84001173183EBDF52613D4E35C70**。300秒/终态/碰撞是明确适配器测试；后述网络探针是普通产品实际入口，不混称真实终端批准或浏览器流程。

## NA007 / NA022：采纳新增独立响应丢失证据，保持PARTIAL

已读 `2026-10-30-response-loss-acceptance.md`，核对原始summary.json摘要 **5E107EFC4E545FF602F74A503FD0D33A26C2AA9E2D2E12D79377C594E9D18B70**。固定r5a binary，两个真实受信Tailscale origin及生产Chromium页：批准已提交，fetch适配器在实际响应验签后丢结果，本地clientId仍null；同数据重启后reload/生产状态重取恢复同key、同节点、原稳定ID。第二轮有帧级null-clientId hello，第一轮没有帧审计，不能替其补写。

该证据是受控交付丢失，不是物理网络丢包。其生产批准恢复分支可作为历史负责路径证据；r5a到r6的Web main.ts存在存储错误提示修改，**不把旧binary的整页执行冒称r6全页重跑**。本轮没有重复其profile/端口操作。原独立storage报告的提交前语句失败、延迟外键导致实际COMMIT失败及原子回滚仍按负责存储路径复用。

NA007剩余：真实CLI在批准提交交界取消时，不回滚信任且明确提示device revoke，并与DB/管理恢复结果关联。NA022剩余：过期/取消/拒绝/重启四种旧材料对应lookup/claim/status的完整有限表，以及max(原到期,批准后60秒)边界前后与仅绑定key查看终态。响应丢失成功不能替代这些断言。

## r7补充：NA027 SQLiteFull子断言通过，整例仍PARTIAL

r6运行结束后用户授权仅trust.rs cfg(test)变化；复核r6清单出现的唯一差异为该文件，不回写历史manifest。随后独立校验r7 `2026-10-30-implementation-r7.sha256`：自身摘要 **D0D20F74FE174616A7876F05664A2D993769D997928091605B603A1B6E1A9471**，83/83一致。

独立实际执行 `cargo test --manifest-path app/Cargo.toml --lib --locked --offline trust::tests::sqlite_full_rolls_back_revocation_and_allows_retry_after_capacity_recovers -- --exact --nocapture`：exit0，1 passed，127 filtered，0.05秒。实际SQLite pager容量受限，trigger额外分配导致错误码13；status/generation/revokedAt/revision/audit/幂等结果/adapterinsert全部回滚。恢复容量、删除适配器后同request重试撤销成功，再重放得到同结果。没有耗尽宿主磁盘，也没有把pager适配器说成真实磁盘物理写满。NA027仍缺后表明确的其他断言。

## 逐例及下一批具体入口

PASS：001、003、004、005、009、010、011、016、018、019、020、025、030、033。其余均PARTIAL如下；这里是本报告已采纳证据后的缺口表，其他并行tester新报告尚未核读者不擅自计入。

| ID | 剩余具体断言 | 可执行入口/所需夹具 |
|---|---|---|
| 002 | 适用产品供应链/密码库维护许可证漏洞结论、secret scanner和真实提交范围证据最终关联 | 现有独立供应链/reviewer报告与CI实际输出、真实BASE/HEAD；不是用户手验 |
| 006 | IPC/出站等各预算前等后、固定窗口、1024桶满/空闲300秒、300秒pair流不被30秒截断 | budget单测适配器+真实HTTP/WSS/owner pair流，按合同预算逐行补 |
| 007 | 批准交界CLI取消后的revoke提示及原提交保留，关联恢复结果 | owner真实TTY/console、批准提交栅栏、DB只读和实际Web |
| 008 | 受理/撤销提交/缓存/入队/socket提交各列明确栅栏及同时订阅操作 | trust/management已有测试barrier及真实WS发送队列，需把每列断言列出 |
| 012 | 远程pipe、token校验/还原失败，异SID全部五命令和第二实例拒绝关联 | 原两真实SID可用；native pipe/token测试适配器与owner CLI，不能再称缺第二用户 |
| 013 | 同一可信Web身份在localhost及Tailscale都执行info/list/订阅的完整对照 | 两TLS路径真实WS，不改变身份资格 |
| 014 | 已知标记code/proof在响应、普通日志、审计、HTTP历史、browser storage/network的逐出口扫描 | 隔离标记probe+真实Chromium网络/存储只读；不能仅查源码 |
| 015 | 未完成pair+多个订阅连接+在途请求组合下stop、释放和重启同记录 | 独立NodeRuntime/owner IPC、实际多WS及端口/pipe/锁核验 |
| 017 | A/B撤销重启证据与新版复制数据key不可读/损坏/meta不符启动失败串联 | 既有A/B证据可复用；只在专属复制data注入，再核不替换/清库 |
| 021 | 两独立真实profile同码竞争，以及赢家原proof/改proof/其他key的有限重试表 | Chromium A/B各独立CryptoKey/ID/IndexedDB，真实pair入口 |
| 022 | 四种失效原因×lookup/claim/status旧材料；留存max边界前后 | 可控clock产品pairing probe补精确边界，再真实HTTP对照 |
| 023 | 各HTTP端点线上阈值、status高频/握手/连接耗尽后正常客户端；慢连接1MiB/256帧先到关闭 | 自有端口真实HTTP/WSS，有界发送并记录close code |
| 024 | 真实页替代key/错nodeId/origin/hostProof不签proof、不覆盖pin；Origin多值等入口表 | 独立TLS替代节点夹具+Chromium/WS，保留原用户绑定 |
| 026 | 批准保存失败下真实浏览器状态/认证管理拒绝，pair proof跨用途长期认证拒绝 | SQLite故障adapter+真实页面/WSS；成功持久恢复已有分支证据 |
| 027 | 可控24小时边界和结果过期重受理；list/info不落幂等/审计不逐帧膨胀的真实WSS计数 | trust clock adapter、已有10000容量独立probe、SQLite只读快照；本轮已补SQLITE_FULL |
| 028 | B多个真实订阅连接、并发发送与撤销；A自撤销无响应时list/重连确认及owner恢复 | 多WS、真实管理revoke和owner CLI |
| 029 | >64实际记录分页、跨actor/node/篡改/60秒游标、revision变化，devices.changed合并与页面安全输出 | 65+合成记录专属DB、真实管理WS、可控clock和Chromium读取 |
| 031 | 写失败/私密/坏key/全进程退出/清站点/换origin各真实browser覆盖及未知命令不重发的证据合并 | 平台tester专属profile报告待核读；已有自动重连/终止/生命周期报告按范围复用，不重复其资源操作 |
| 032 | 当前可复用生产路径的独立A/B完整受信闭环证据与当前阶段逐项对应 | 原受信TLS/Tailscale A/B独立报告复核及必要差异重测；不要求用户重复技术验收 |

## 自有fixture失败、清理及证据

没有确认产品缺陷。首次自写Rust夹具误用pub(crate) trust_store，编译exit101，未运行产品；改用现有普通NodeRuntime/IPC/HTTPS入口。下一次扫描整个data目录读到锁定文件，exit1/Windows错误33；节点已走owner stop。限定SQLite主文件/WAL后唯一一次重跑exit0。失败日志原样保留，未算通过，也没有为探针改产品接口。

成功网络探针owner stop/join后删除自己data及生成cert/key，目录不存在。旧身份测试遗留的自有TEMP共15文件逐一独占打开成功后清除，仅限规范化的 `.tmp/independent-r6-20261030/temp`，不删除其他目录；新crash两点自身也有显式清理断言。30451/30452无监听，用户30447原PID31228仍监听。本轮没有新浏览器空间。r7使用独立r7-temp，测试显式close后删除数据库，后续一并核查空目录。无提交、推送、归档。

| 原始证据（均在.tmp/independent-r6-20261030） | SHA256 |
|---|---|
| identity.log | DCA6ABFED774B8EA03B4DCB1DC087F019D6A3934D414B5AFCE1D092272C582EE |
| pairing.log | 250319AA7C2AA757B00B4AD43E64D0C97D6519ABA56EC86AD07F748EE0A3CAD8 |
| persistence-final-2.log | 17FF046F97D06FF1D8789CC42C17EB4DE12326C034DAD3B2E2D4F12B3BF6A31D |
| challenge.log | 84FD2330C66AA3E2C052D3234FA71FC8301A476CDB180ACB1D34B69FFF0068D7 |
| schema.log | 85BE1591265BFBF1BB782942C3FDAF1335F7BBE1D60E97554B58FC0CBA68FC00 |

PowerShell把cargo stderr正常进度包装为NativeCommandError文本，实际成功判据采用真实LASTEXITCODE及断言结果，不把该包装当产品失败。probe编译警告为复用helper的unused imports/dead_code，不是产品Clippy结果；本轮未声明probe -D warnings通过。

最终补记：r7-temp实际为空后已删除；r7清单复核83/83、0 mismatch，自有binary仍135BD9...1509。r7-sqlite-full.log SHA256为720F30442177DB9A9ABD0B6EA27E1DB981E84046D9AEA8942892A32722BDED4B。范围内git diff --check exit0。本轮成功网络fixture已owner stop/join且整个data删除成功，因此没有保留其数据锁；未另做删除后确切pipe名称枚举，不将此额外观测写成已执行。
