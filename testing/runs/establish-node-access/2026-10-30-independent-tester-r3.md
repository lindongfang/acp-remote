# establish-node-access r3 独立阶段验收

日期：2026-10-30，Asia/Shanghai。已完成本轮获授权的有界定向验收和清理。**当前33例：4 PASS、27 PARTIAL、2 NOT-RUN，0个本轮确认产品FAIL。** PARTIAL保留未执行的必要断言；本报告不构成可归档或reviewer代签结论。

## 版本及证据边界

- 旧版清单27AAB…、r2清单409A…的结果分别保存在同日independent-tester、independent-tester-r2报告。旧结果不改写为新版本通过。
- r3固定清单：`testing/runs/establish-node-access/2026-10-30-acceptance-r3.sha256`，77项，自身SHA-256 `351D086478979DFE2BDF4FE7B47B3B9B46FD67058BB786DCB9CE43877EA263A2`。开始时独立核对77/77匹配。
- r2→r3只有`clients/web/src/main.ts`发生产品内容变化。本轮定向覆盖其恢复调度修复；Rust不变，复用已核对同范围原始断言，不重跑全量Rust。
- 自有target `.tmp/independent-acceptance-20261030/target`，`cargo build --manifest-path app/Cargo.toml --locked --offline`退出0。r3 binary SHA-256 `EBD469BD370E939C547E556928B9768F71318EEB0D6657E17DD18E4EF1B5799F`，嵌入已生成的r3 Web生产资源。
- 真实Windows owner节点30452、新data-r2，与用户30447隔离。ego沿用本轮TaskSpace4（independent-node-access-r2-20261030）；未另建空间绕过超时。正常访问受信TLS/Tailscale origin，无忽略证书参数。

## r3 已执行的两项定向

探针源码：`testing/probes/establish-node-access-independent-20261030/r3-restore-scheduler.mjs`。实际执行一次、退出0、无fixture故障或重试。

使用真实Chromium执行实际生产页面main及真实IndexedDB；只把WebSocket故障响应、Date.now和长定时器替换为可控适配器，pagehide/pageshow/online为合成事件。这是页面协调测试，**不声称真实BFcache导航通过，也未实际等待60秒**。

| 场景 | 实际刺激与断言 | 结果 |
|---|---|---|
| 限流deadline跨页面恢复 | 4429后推进到第10秒，pagehide→连续两次pageshow(persisted)→online；异步boot完成后连接数仍1，剩余定时器恰50000ms。推进49999ms仍1，第60000ms只新建第2条连接。 | PASS（定向子项） |
| 终端停止不被页面恢复复活 | 尚未验证hostProof时4401关闭，pagehide/pageshow/online，推进120秒并再次hide/show；界面保持“管理连接已停止”，连接数始终1，保留原key/clientId/node pin。 | PASS（定向子项） |

两例最后都比对真实IndexedDB中的clientId/publicKey/nodeId与初始值一致；恢复原生WebSocket并reload后，原身份仍成功认证、显示管理列表。原始状态：`.tmp/independent-acceptance-20261030/r3-restore-scheduler.json`。

## 与r2证据的组合

- r2真实已批准身份在线收到4410后直接进入新key配对页，仍等待真实owner TTY明确y批准；新ID认证成功。
- r2离线撤销后reload，原生WSS明确收到4401/authentication failed，进入新key配对页；再次明确批准获得`ZRM22m9bN-uLRcvglc3ToA`并正常认证。r3只变恢复调度，沿用该已批准身份继续运行，未重复整条配对矩阵。
- r2真实生产页面内，未验证hostProof前的4401/4410故障关闭均保留原key/pin、停止重试；该恶意传输是适配器模拟，与上述正常服务器关闭分开记录。
- r2可控延迟真实IndexedDB initialize和reset回调，在hide/show交错下完成隐藏期不写界面/不开连接、reset串行化、恢复只创建单一身份/连接的断言。原始JSON及真实/模拟边界见r2报告，不扩展为全部生命周期组合。
- reviewer已独立关闭R01/R02/R03/D01的消息属于独立角色结论；本tester仅陈述自己执行的上述断言，不代替其关闭决定。

## 工程证据复用

独立读回主方原始`.tmp/acceptance-r3-check.log`：npm check实际命令链包含Web类型检查、7文件46测试、生产构建、Rust格式/Clippy、118库+4JWS测试，所有实际测试零失败。日志SHA-256 `B2FC4220AFDA7979E5AE2D289CCA7946CB73A14AE96DBDFE709FA2D26522A876`；主方已报告命令退出0。本tester没有将命令定义或零目标当成功测试，按当前规则复用同版本同范围工程检查。

本tester另有r3独立cargo build退出0；前序r2本tester实际strict OpenSpec与git diff --check均退出0（r3未改变这些结构定义），最终再查差异格式。上述组合支持NA-001。工程检查不会填补其余用例的行为矩阵。

## 当前33例逐项结论

以下按当前精简tests.md的完整必要断言判定；NA-033采用用户原始通过记录，主观理解不替代技术断言。旧版逐例的详细缺口仍保留于同日independent-tester报告；本表明确最新实际补测带来的收缩。

| ID | 状态 | 剩余必要断言或通过依据 |
|---|---|---|
| NA-001 | PASS | r3原始完整check日志复核+独立build，strict与diff同范围证据有效。 |
| NA-002 | PARTIAL | gitleaks与Rust公告库扫描未执行，独立依赖官方维护/许可/平台/漏洞核验及真实提交范围仍缺；audit零漏洞不能替代。 |
| NA-003 | PARTIAL | 原Rust/Web向量有证据；真实Chromium双向完整变异矩阵仍缺。 |
| NA-004 | PARTIAL | 原绑定/过期/消费可复用；全部签发入口随机故障矩阵仍缺。 |
| NA-005 | PARTIAL | 状态/格式/容量子集可复用；碰撞强制重生、全终态表、非交互CLI仍缺。 |
| NA-006 | PARTIAL | 六类预算完整小于/等于/大于边界、1024来源桶、空闲淘汰与300秒流式寿命仍缺。 |
| NA-007 | PARTIAL | 真实存储提交失败/回滚可复用；批准已提交但响应丢失+同时取消、浏览器null-ID恢复的确定性组合仍缺。 |
| NA-008 | PARTIAL | 原撤销协调/队列/存储失败子集有效；全部调度栅栏和多连接组合仍缺。 |
| NA-009 | PARTIAL | 原崩溃恢复证据有效；密文写入/flush中途终止全部阶段仍缺。 |
| NA-010 | NOT-RUN | 缺已实现的应用策略测试适配器入口，属于实现缺口；没有动态审批验证，不交给用户。 |
| NA-011 | PARTIAL | 正常启停及故障测试可复用；各失败修正重试和全部敏感输出检查仍缺。 |
| NA-012 | PARTIAL | 真实跨SID DACL/pipe/DPAPI可复用；远程pipe、token/revert故障及全部CLI路径仍缺。 |
| NA-013 | PARTIAL | Tailscale待批准三类管理拒绝已补；同一有效身份跨localhost/Tailscale完整矩阵仍缺。 |
| NA-014 | PARTIAL | 统一错误子集有效；标记秘密在网络/storage/日志/审计的完整扫描仍缺。 |
| NA-015 | PARTIAL | 本轮多次owner stop与锁/pipe/监听释放；未完成配对+多订阅+在途命令同轮组合仍缺。 |
| NA-016 | PARTIAL | 原真实stop投递失败有效；真实15秒deadline耗尽和多项未完成报告仍缺。 |
| NA-017 | PARTIAL | Rust损坏/DPAPI及旧闭环可复用；新版两个独立profile的A/B撤销重启整链未重跑。 |
| NA-018 | PASS | 本轮旧27AAB实际TTY/Web认领、指纹/名称/到期和批准前三类管理拒绝；r2/r3未改变待批准协议边界，可复用。 |
| NA-019 | PASS | 本轮r2实际显式批准、新WSS身份认证和列表读取；r3恢复调度变更不影响该正常批准路径。 |
| NA-020 | PARTIAL | 原取消状态测试有效；真实TTY拒绝/回车/Ctrl-C/IPC断开及网络批准完整矩阵仍缺。 |
| NA-021 | PARTIAL | 原claim重试/冲突子集有效；两真实profile同码并发与完整proof矩阵仍缺。 |
| NA-022 | PARTIAL | 原过期/重启清理子集有效；批准响应丢失与max留存边界仍缺。 |
| NA-023 | PARTIAL | 原限额/4429有效；逐端点在线耗尽及慢连接两个出站上限仍缺。 |
| NA-024 | PARTIAL | 未验证hostProof关闭不会删pin已动态补测；真实替代节点key/无效hostProof全矩阵及第三方请求审计仍缺。 |
| NA-025 | PARTIAL | 原证明/连接上限子集有效；15秒不发proof独立实际观察仍缺。 |
| NA-026 | PARTIAL | 正常保存/刷新及原存储失败有效；故障下浏览器状态和节点重启管理组合仍缺。 |
| NA-027 | PARTIAL | 原58断言存储probe和真实撤销重连有效；实际磁盘满、可控24小时精确边界、WSS只读/审计膨胀矩阵仍缺。 |
| NA-028 | PARTIAL | 真实单连接撤销/重连拒绝补齐；多B连接并发命令/推送和真实自撤销完整组合仍缺。 |
| NA-029 | PARTIAL | 原分页/游标和当前列表有效；真实浏览器65+条、60秒续读、完整审计/公开页安全头及全部秘密出口仍缺。 |
| NA-030 | NOT-RUN | 同NA-010，缺需要运行时审批的测试能力调用入口；未拿静态字段缺失当动态拒绝证据。 |
| NA-031 | PARTIAL | 自动原身份恢复、正常4401/4410新key再批准、未验证节点关闭保pin、initialize/reset受控交错及r3调度均有动态证据；私密/写失败/坏key/清站点/新origin、结果未知命令及完整生命周期组合仍缺。 |
| NA-032 | PARTIAL | 新binary真实TLS/Tailscale/ego、配对/撤销/重连与清理已执行；未在本轮以两独立profile重跑完整A/B闭环，保持PARTIAL。 |
| NA-033 | PASS | 采用同日user-acceptance原始用户“通过”记录，限定主观可理解性。 |

统计：4 PASS（001/018/019/033），27 PARTIAL，2 NOT-RUN（010/030），共33。r2/r3定向子项PASS不提升NA-031/032整例。

## 清理

- r2节点PID32292、r3节点PID35976均owner CLI正常stop退出0。最终自有节点进程数0。
- 自有30451/30452无监听；用户30447仍由PID31228监听。
- data及data-r2锁均可由真实owner以FileShare.None独占重开；两种canonical路径计算的专属pipe均不存在。
- ego TaskSpace4 ownership=agent，已finish({keep:[]})；没有启动额外TaskSpace绕过故障，没有独立Edge/CDP进程。
- 全部target/data/DPAPI密文/SQLite/配置/运行日志/JSON均在.tmp；探针目录只留源码。短配对码输入文件已删除，自有受保护data保留供复现，未删用户身份或数据库。
- 本轮没有修改产品代码、规格、tasks、plan、清单或现有探针，未提交/推送。
- 报告落盘后再次核对r3清单自身SHA-256及全部77项，77/77匹配、0 mismatch；最终逐例表机器计数为4/27/2，差异格式检查退出0。此核对发生于主流程随后可能追加plan摘要之前；未来元数据变更应独立记录，不能隐改原清单。

当前仍有必需PARTIAL/NOT-RUN，不能按本报告宣布可归档。主流程可根据上述具体缺口落实验证入口及安排后续技术检查，无需用户重复已确认的技术步骤。
