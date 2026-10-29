# establish-node-access 独立验收（2026-10-30）

状态：本轮阶段验收结束。结论绑定旧 `27AAB…` 冻结源码，不覆盖主流程随后解冻的 Web 修复。33 例重新映射为 3 PASS、28 PARTIAL、2 NOT-RUN；未把剩余必需矩阵视为通过，当前不能据本报告归档。

## 固定版本与边界

- 工作目录：`D:/Project/acp-remote`。
- 固定清单：`.tmp/2026-10-30-acceptance.sha256`，69 项；自身 SHA-256 `27AAB89FA0092439FB9FC16754C21113921620166C179263F481F8D4C7B40932`。
- 本轮独立校验：69/69 匹配、0 missing、0 mismatch。
- 已读取当前 AGENTS、OpenSpec config/tester 职责、变更 proposal/design/access-contract/specs/tests/plan/tasks/review。当前用例是 NA-001–033；旧安全汇总 NA-033 已删除，旧 NA-034 对应当前主观理解 NA-033。
- 探针源码目录：`testing/probes/establish-node-access-independent-20261030/`；运行材料、日志与独立 CARGO_TARGET_DIR 为 `.tmp/independent-acceptance-20261030/`（target 子目录），不与 `.tmp/acceptance-20261030-target` 共用。初次构建误置于探针源码目录；已先 owner stop，再仅迁移本轮 target/data/config/log/JSON 到 .tmp，data DACL 保持仅 FARADAY\\jack。
- 使用独立端口 30451/30452；保留用户 30447 节点、身份与日常浏览器。

## 证据复用与重测判断

历史报告的 PASS/PARTIAL 总计不继承。每个当前 Steps/Expected 按原始断言重新映射；开发报告仅用于定位入口，不能代替本轮实际验收。

自动恢复相关 main.ts、连接恢复/身份清理、4401/4410 返回配对页及新 key 再批准是本轮优先重测范围。历史存储、DPAPI、IPC、Rust 协议断言须核对负责文件与当前清单字节后才复用。NA-033 用户记录见 `2026-10-30-user-acceptance.md`，只采纳用户主观理解结论，不代签技术或 reviewer 验收。

## 实际执行

1. 清单自身及全部 69 项校验通过。
2. 独立 target 的 `cargo build --manifest-path app/Cargo.toml --locked --offline` 已完成，退出 0；运行于显式非沙箱真实 Windows jack 用户。binary SHA-256：`15DA7BE7534D538E9C2DB57DB924ABD0A53AC5C72D4E6F4732D9583ECE5A70E8`。
3. 真实受信 TLS + Tailscale IP 的 ego 页面成功配对，owner TTY 明确批准；批准前 node.info/device.list/subscribe 各返回 4400 且无消息。Node ID `a564a0a7294c8287af43209722d2fb6512bbad30e70562db7e2356bb0ccd81cd`；A clientId `AazCS4gbqYKcFCKYWXdm_w`，私钥 extractable=false。未使用证书忽略参数。
4. owner stop 后页面收到 1001/node stopping，显示自动重连且没有重新尝试认证按钮；同数据目录重启后，不刷新、不点击，原 clientId/公钥/Node ID 全部不变，自动认证并恢复列表。

## 逐例状态

以下为本轮阶段结束的逐例映射；PARTIAL 包含尚未完成的新增实测，不继承旧总计。

| ID | 当前状态 | 可复用证据与剩余必要断言 |
|---|---|---|
| NA-001 | PARTIAL | 本轮独立 build 成功；同版主方完整检查日志待核对，独立工程检查补充中。 |
| NA-002 | PARTIAL | 历史及主方审计作为候选证据；供应链、秘密扫描、真实提交范围、浏览器 JWS 分别核验，不能仅凭 CI 定义通过。 |
| NA-003 | PARTIAL | Rust 字节未变，复用固定向量/严格解析断言；真实 Chromium 双向变异矩阵待补。 |
| NA-004 | PARTIAL | challenge Rust 未变，可复用绑定/过期/单次消费；全部签发入口随机故障矩阵仍缺。 |
| NA-005 | PARTIAL | Rust 状态/格式/容量原测试可复用；码碰撞、终态全表、非交互待逐项补。 |
| NA-006 | PARTIAL | 原预算/JSON测试可复用；六类预算完整边界、1024桶和300秒流式等仍缺。 |
| NA-007 | PARTIAL | 独立存储实际提交失败/回滚可复用；提交成功后丢响应、同时CLI取消与浏览器null-ID恢复待补。 |
| NA-008 | PARTIAL | 原撤销协调/队列测试和存储回滚可复用；全部调度栅栏组合仍缺。 |
| NA-009 | PARTIAL | 原身份崩溃/元数据测试可复用；写入及flush中途终止覆盖需复核。 |
| NA-010 | NOT-RUN | 缺少已实现的应用策略/后续能力测试适配器入口，属于验收入口实现缺口，非TLS/用户/工具环境限制；未执行动态授权验证。见下节依据。 |
| NA-011 | PARTIAL | 原启动故障测试可复用；本轮正常启动已完成，各失败修复重试仍需补。 |
| NA-012 | PARTIAL | 原跨真实SID DACL/pipe/DPAPI拒绝可复用；远程管道、全部CLI及token/revert故障仍缺。 |
| NA-013 | PARTIAL | 原localhost拒绝及本轮Tailscale待批准三类管理拒绝已有证据；有效身份两路径及同资格组合待补。 |
| NA-014 | PARTIAL | 原统一错误断言可复用；完整标记秘密网络/storage/日志/审计扫描待补。 |
| NA-015 | PARTIAL | 本轮owner stop正常；未完成配对+多订阅+在途组合与释放核验待补。 |
| NA-016 | PARTIAL | 原真实stop投递失败和缩短deadline断言可复用；实际15秒耗尽及多项报告仍缺。 |
| NA-017 | PARTIAL | 原损坏/DPAPI可复用；新版A/B撤销/重启闭环本轮待完成。 |
| NA-018 | PASS | 本轮真实TTY、Web认领、节点/客户端指纹与待批准管理三类拒绝均已执行。 |
| NA-019 | PASS | 本轮终端明确y批准、Web新WSS证明/稳定ID/list；存储提交失败断言按相同Rust字节复用。 |
| NA-020 | PARTIAL | 原取消状态测试可复用；真实TTY拒绝/回车/Ctrl-C/断开和网络批准拒绝待执行。 |
| NA-021 | PARTIAL | 原claim幂等/冲突及已撤销key拒绝可复用；两个真实profile同码并发及完整proof矩阵待补。 |
| NA-022 | PARTIAL | 原过期/清理/重放可复用；批准响应丢失及max留存边界待补。 |
| NA-023 | PARTIAL | 原HTTP/WSS限额及4429可复用；逐端点线上耗尽与慢连接两预算仍缺。 |
| NA-024 | PARTIAL | 原Host/Origin与节点替换单测可复用；新版浏览器替换节点不签proof/不覆盖绑定待补。 |
| NA-025 | PARTIAL | 原证明/连接上限断言可复用；15秒未发proof独立补测。 |
| NA-026 | PARTIAL | 原存储失败证据可复用；故障下浏览器状态及重启管理复核待补。 |
| NA-027 | PARTIAL | 同Rust原58断言存储probe支持容量/幂等/事务；实际磁盘满、可控24小时、WSS只读/审计和新版B重连仍缺。 |
| NA-028 | PARTIAL | 原撤销队列断言可复用；新版多B连接/自撤销和owner恢复待补。 |
| NA-029 | PARTIAL | 原分页/游标可复用；真实浏览器65+记录、60秒与敏感输出检查待补。 |
| NA-030 | NOT-RUN | 同NA-010，缺少需要审批的测试能力调用入口；未以源码没有审批字段替代动态验证。 |
| NA-031 | PARTIAL | 本轮实际通过：同origin保存身份自动认证；真实停止/重启后同key/clientId恢复；4410及4401停止旧身份重试、换新key直接配对、再次明确批准才得到新ID并认证；无重试按钮。仍缺坏记录/私密模式/写失败/清站点/新origin/节点不匹配/结果未知命令及完整生命周期竞态；R01/R02待新版本复测。 |
| NA-032 | PARTIAL | 本轮新binary受信TLS/真实Tailscale/ego A已运行；B撤销/重启与最终资源清理待补。 |
| NA-033 | PASS | 用户记录原话与当前主观理解项对应；技术断言仍归各NA用例，不转交用户。 |

## 本轮优先浏览器验收的实际结果

本轮使用 ego-browser，单一 agent TaskSpace 3（`independent-node-access-20261030`），页面 p1；没有操作日常浏览器页或30447 origin。这里是一个独立origin的A身份连续更新实验，不冒充两个独立profile的A/B完整L6。

- 真正运行的 binary 来自本轮独立构建，SHA-256 `15DA7BE7534D538E9C2DB57DB924ABD0A53AC5C72D4E6F4732D9583ECE5A70E8`。监听真实Tailscale IP `100.119.123.57:30451`，HTTPS origin `https://faraday.tail74842e.ts.net:30451`；沿用用户已保护的证书部署材料（只读，未复制私钥）。ego页面 `isSecureContext=true`，无证书忽略参数。
- 首轮 `device pair` 为真实owner TTY；节点指纹 `a564a0a7294c8287af43209722d2fb6512bbad30e70562db7e2356bb0ccd81cd`。Web认领后终端名称及客户端指纹与页面一致。尚未批准时，页面内原生WebSocket分别发送node.info/device.list/subscribe，均收到4400、无资料/订阅消息。随后终端明确输入y，得到 `AazCS4gbqYKcFCKYWXdm_w`，页面认证并读取列表。
- 正常owner stop返回0、1001/node stopping；同data重启后页面不刷新、不点击，自动重连。JSON比较断言clientId、公钥、Node ID与初次值完全相等，私钥extractable=false。中途为迁移自有运行目录再次正常停止/重启，同身份继续恢复；观测到1006期间重试，最终回到管理页。精确退避全部时间点不是本轮新增实测。
- 在线撤销首个A，真实关闭4410/revoked，页面自动清除失效配对、生成不同公钥，记录status=unbound、clientId=null，直接呈现配对表单。online/pageshow刺激后观察3.5秒，没有新WebSocket。新key认领停在pending_confirmation，终端再次明确y后才取得 `ttwEClM52QxT0isxLD-G8w`，页面认证成功。
- 为使下次打开走4401：在已认证页面触发pagehide（合成生命周期事件，记录为故障刺激，非真实导航验收），使管理连接关闭；owner离线撤销，然后页面reload。页面直接呈现新配对表单且换新公钥；再次认领/批准后得到 `588Kczm1PWiSU-uXsvURxA`，管理列表显示前两个身份已撤销。首次记录close码的观察脚本因CDP页面钩子跨调用未保留而失败（TypeError，退出1），未把该脚本算通过。
- 随后同样的离线撤销场景重新执行，在同一ego调用内安装close监听并reload，明确捕获4401/authentication failed。页面clientId=null、公钥不同；online/pageshow后观察3.5秒，opened仍为1，无旧身份循环。再次用新key真实认领，终端显示新指纹并待批准；显式y后取得 `pfNbzGH2j1MGIlMdkmBA8Q`，真实WSS认证、稳定列表读取成功，私钥extractable=false。
- 这些正常4410/4401闭环只证明真实服务器拒绝后的产品流程；不证明未验证hostProof时恶意关闭码可以安全清除pin，也不证明boot/reset异步生命周期竞争已经安全。reviewer R01/R02保持待复现/待修复，新版本需定向验证。

### 已保存的原始证据

全部运行产物位于 `.tmp/independent-acceptance-20261030/`：

| 文件 | 实际内容 |
|---|---|
| a-approved.json | 首次批准的clientId/publicKey/Node ID、私钥不可导出与管理正文 |
| pending-denial.json | 批准前3类管理帧4400且无消息 |
| disconnected.json | 1001、恢复提示及无重新认证按钮 |
| recovered.json | 不刷新恢复后原ID/key/node一致及连接事件 |
| revoked-4410.json | 4410事件、新公钥、unbound/null身份及无循环连接 |
| renewed-4410-approved.json | 4410之后新key再次明确批准后的认证状态 |
| after-4401-pair-form.json / after-4401-approved.txt | 首轮离线撤销后的配对页和再次批准管理状态 |
| rejected-4401.json | 重做后实际4401/authentication failed及无循环连接 |
| final-approved.json | 最终新key再次owner批准、认证、不可导出私钥和最终列表 |
| node*.log / restart*.log | 本轮节点ready/stop输出；不保存配对码或完整proof |

测试源码保留在 `testing/probes/establish-node-access-independent-20261030/`，只含mjs；历史执行时使用的短配对码已从源码移除，重跑通过.tmp/pair-input.json读取新鲜owner终端码。脚本日志只保留公开元数据和协议类型，不导出CryptoKey、DPAPI明文或私钥。

## 复用判断与未执行项

与原71项源码清单逐路径比较，新69项中已有路径变化为main.ts、management/client.ts、styles.css、management.test.ts、接入合同、client-trust增量规格及两个测试定义。Rust app文件内容保持原摘要，因此原独立118+4、stop投递失败、真实跨SID DACL/pipe/DPAPI、身份故障与存储58断言的同范围证据可复用；这只支持其已运行子断言，不能补齐旧报告明确缺失的大矩阵。

本轮读取了 `2026-10-30-integrated-development.md`：其同清单的npm check（Web38、Rust118+4）、build:rust及根/Web网络audit为退出0的可追溯集成执行记录。本tester自行构建和真实浏览器行为另有上文证据；NA-001仍记PARTIAL，因本轮没有独立运行整套工程命令或读回所有原始工程日志。NA-002仍缺gitleaks执行和Rust公告库扫描，不把安装失败或CI定义计作扫描通过。

NA-010/030：仓库公开lib模块、CLI帮助、管理action分派及README所列能力只有节点接入、配对、list/revoke；源码搜索中的approve是本机配对批准，不是后续能力运行时审批。未发现spec要求的策略测试适配器或需要审批的测试能力入口；因此当前是测试入口实现缺口，NOT-RUN。没有在本轮构造一个虚假Harness请求来宣称动态授权边界已验证；后续应提供可执行测试能力，分别实测有效身份+无审批/过期审批的拒绝。

按主流程最后指令，本轮只收敛已启动的浏览器与清理，不扩展大矩阵。表中PARTIAL的具体缺口保留；不是要求用户重复技术验收，也不代签reviewer。

## 清理与交接

- 三次本轮节点均通过真实owner CLI正常stop，退出0；最终PID36012已退出，自有node进程数0。
- 30451/30452无LISTENING；用户30447仍由PID31228监听，未停止或修改其身份。
- 基于当前owner SID与自有data canonical path计算的专属pipe（普通/扩展路径两形式）均不存在；data/.node.lock可在真实owner下以FileShare.None独占打开，data DACL仍仅FARADAY\\jack。
- 本轮没有启动独立Edge进程或CDP服务；仅有ego TaskSpace3已检查ownership=agent并finish({keep:[]})关闭，不关闭日常浏览器。
- 运行target/data/DPAPI密文/SQLite/config/JSON/日志均在.tmp，testing/probes仅留探针源码；未将运行身份材料混入交付文件。
- 源码已由主流程宣布解冻，后续Web改动使本轮旧快照的相关行为结论不再覆盖新版本。待新摘要后复测R01/R02及受影响浏览器流程；Rust未变时继续按具体断言复用。
- 收尾读取持久清单，自身摘要仍为27AAB…；当前工作区已有8条与旧摘要不同：`clients/web/src/main.ts`、`clients/web/src/management/client.ts`、`clients/web/src/pairing/identity.ts`、`clients/web/tests/management.test.ts`、`clients/web/tests/pairing.test.ts`、`testing/suites/client-trust/tests.md`、`testing/suites/node-access/tests.md`、`testing/suites/traceability/establish-node-access.md`。这是主流程已明确通知的修复解冻范围，记录版本边界，不沿用本轮旧binary结果证明新字节。开始时69/69匹配与结束时8项变化同时保留。
