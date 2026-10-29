# Tests

来源变更：establish-node-access。范围为 Windows 节点与 Web，移动端和真实 Harness 不在本次验收范围。用例定义与执行结果分开；删除重复的原NA-033安全汇总用例，其检查归入NA-002/012/014及独立交付检视；原NA-034前移为NA-033，仅保留用户本人可理解性判断。历史报告仍使用旧编号，原NA-033不对应新NA-033，旧NA-034才对应新NA-033；不据编号变化迁移通过状态。

## 共用执行说明

- 在固定提交或文件摘要上执行；结果另存 `testing/runs/establish-node-access/`，实际通过、失败、未执行分别记录，不能用开发自测或结构检查代替独立验收。当前执行进度与历史证据见 `plan.md` 的 Verification Results。
- 使用 Windows x64 和独立测试账户、目录、端口、浏览器配置及公开测试密钥。CLI 按 `access-contract.md` §3 操作；Web 从本机终端显示的 canonicalOrigin 进入；协议探针按合同 §3–5 发送帧。
- NA-003–010 使用合同测试或明确的时钟、随机源、存储及调度故障适配器；密码学向量使用真实 Rust 库与浏览器 WebCrypto。NA-011–031 使用真实节点、CLI、浏览器和协议探针，故障只注入隔离副本或明确适配器；缺少具体入口时记未执行，不用正常路径代替故障检查。
- NA-032 使用真实 Windows、受信且名称匹配的 TLS 证书、SQLite、DPAPI、本机命名管道、WebCrypto、IndexedDB、Chromium 与 Tailscale，不以模拟替代。局部测试不代替完整链路。
- 成功、失败或取消后均关闭本次连接、浏览器与节点，释放测试锁和端口；只清理本次拥有的临时目录及测试身份，保留脱敏证据，不操作用户真实数据库、密钥或配置。
- 安全与依赖检查由独立Harness按NA-002/012/014及交付检视执行；缺工具、环境或权限记受阻/未执行，不转为LX。NA-033仅由用户判断提示的可理解性，只需回复“通过”或“不通过”。Agent 负责记录用例编号、受测版本和用户原始结论，必要时跟进问题；不要求用户填写技术报告、环境、证据或签署表。未回复记待确认，不得代签；必需用例失败、未执行或待确认不算验收通过。

## 需求对应

以下路径均指本变更内的增量规格，不回退长期规格；全部用例同时受 `access-contract.md` 约束。相同需求关联集中列出，避免在用例内重复。

| 用例编号 | 规格与需求场景 |
| --- | --- |
| NA-001、NA-032 | specs/node-access/spec.md — Requirement: 完整节点独立启动；Scenario: 正常启动 |
| NA-002、NA-014 | specs/node-access/spec.md — Requirement: 接入失败不泄漏敏感信息；Scenario: 认证失败 |
| NA-003、NA-004 | specs/client-trust/spec.md — Requirement: 身份必须经过验证；Scenario: 冒用已知客户端标识 |
| NA-005、NA-022 | specs/client-trust/spec.md — Requirement: 配对限时且不可转授；Scenario: 重放失效配对 |
| NA-006、NA-023 | specs/client-trust/spec.md — Requirement: 配对限时且不可转授；Scenario: 配对码猜测 |
| NA-007 | specs/client-trust/spec.md — Requirement: 信任建立必须可靠保存；Scenario: 保存失败 |
| NA-008 | specs/client-trust/spec.md — Requirement: 撤销覆盖已有连接；Scenario: 撤销与操作并发 |
| NA-009 | specs/node-access/spec.md — Requirement: 重启保持可信状态；Scenario: 身份资料损坏 |
| NA-010、NA-030 | specs/client-trust/spec.md — Requirement: 信任不代替运行时审批；Scenario: 后续模块请求权限 |
| NA-011 | specs/node-access/spec.md — Requirement: 完整节点独立启动；Scenario: 正常启动；Scenario: 初始化失败 |
| NA-012 | specs/node-access/spec.md — Requirement: 网络可达不授予管理权限；Scenario: 其他系统用户调用本机控制入口 |
| NA-013 | specs/node-access/spec.md — Requirement: 网络可达不授予管理权限；Scenario: 从可信网络发起未认证请求；Scenario: 已认证客户端管理节点 |
| NA-015 | specs/node-access/spec.md — Requirement: 节点关闭具有资源所有权；Scenario: 正常关闭 |
| NA-016 | specs/node-access/spec.md — Requirement: 节点关闭具有资源所有权；Scenario: 关闭超时或失败 |
| NA-017 | specs/node-access/spec.md — Requirement: 重启保持可信状态；Scenario: 正常重启；Scenario: 身份资料损坏 |
| NA-018 | specs/client-trust/spec.md — Requirement: 本机命令启动 Web 配对；Scenario: Web 输入配对码 |
| NA-019 | specs/client-trust/spec.md — Requirement: 本机确认后才建立信任；Scenario: 本机批准 |
| NA-020 | specs/client-trust/spec.md — Requirement: 本机确认后才建立信任；Scenario: 本机拒绝或取消 |
| NA-021 | specs/client-trust/spec.md — Requirement: 配对限时且不可转授；Scenario: 多个 Web 客户端并发认领 |
| NA-024 | specs/client-trust/spec.md — Requirement: 配对过程核实目标节点；Scenario: 节点身份被替换 |
| NA-025 | specs/client-trust/spec.md — Requirement: 身份必须经过验证；Scenario: 冒用已知客户端标识；Scenario: 验证有效身份 |
| NA-026、NA-031 | specs/client-trust/spec.md — Requirement: 信任建立必须可靠保存；Scenario: 保存成功；Scenario: 保存失败 |
| NA-027 | specs/client-trust/spec.md — Requirement: 可信客户端可查看和撤销信任；Scenario: 撤销其他客户端；Scenario: 撤销保存失败；Scenario: 重复撤销 |
| NA-028 | specs/client-trust/spec.md — Requirement: 撤销覆盖已有连接；Scenario: 长连接被撤销；Scenario: 撤销与操作并发 |
| NA-029 | specs/client-trust/spec.md — Requirement: 认证材料不得作为管理数据返回；Scenario: 查询客户端列表 |
| NA-033 | specs/client-trust/spec.md — Requirement: 本机确认后才建立信任；Scenario: 本机批准；Scenario: 本机拒绝或取消 |

## Cases

### NA-001 构建及工程检查入口

- 用例级别：L1
- 操作步骤：在仓库根目录按 README 与实际工程配置执行 npm run check、npm run build:rust、npx.cmd openspec validate establish-node-access --strict 和 git diff --check；核对 Rust/Web 实际测试数量与构建目标。
- 预期结果：Rust 格式、Clippy、测试及 Web 类型检查、测试、构建和节点构建真实通过；OpenSpec 与差异检查通过仅代表文档结构和格式。零目标、缺失入口或未执行均不得当作编译或功能验收通过。

### NA-002 静态合同与供应链

- 用例级别：L2
- 操作步骤：在仓库根目录执行 npm run check 与 npm audit --audit-level=high；按 .github/workflows/ci.yml 检查密钥扫描与供应链作业。BASE/HEAD 为真实可解析提交时运行 npm run lint:commits -- --from BASE --to HEAD --verbose。按 README 的浏览器 JWS 入口在真实 Chromium 复核固定向量，并按 app/DEPENDENCIES.md 与实际配置独立核对密码库、产品依赖官方维护状态、许可证、平台支持、漏洞信息及锁文件；结合合同§1–7检查不自造密码学，安全边界与实现的整体符合性由独立reviewer检视。
- 预期结果：现有静态检查、协议向量及适用供应链检查通过；开发工具审计不能替代 Rust/Web 产品供应链审查，CI 作业定义不能当作已执行证据。缺少审计工具、真实提交范围或执行证据时如实记录未执行；结构检查不代表场景覆盖充分；依赖与密码库审查须有可追溯结论和问题关联，未审或未解决阻断项不得写通过。

### NA-003 严格编码与签名互操作

- 用例级别：L3
- 操作步骤：用公开测试key与固定JWS原字节在Rust和真实WebCrypto双向签验；变异SEC1长度/曲线点、DER/P1363、零或越界r/s、high-S/low-S、base64url、header、重复键、非法UTF-8/Unicode、未知字段与安全整数。
- 预期结果：严格遵守access-contract.md §2；合法high-S/low-S均可用，非法输入拒绝；验签覆盖原始两段ASCII，不先重序列化；固定向量及可执行测试入口须实现阶段建立，不能只检查随机互签。

### NA-004 挑战绑定与一次性消费

- 用例级别：L3
- 操作步骤：在可控时钟与随机源测试边界变异nodeId/origin/requestId/challengeId/nonce/用途/版本/连接/key；重放证明、重取挑战、超过30秒、墙钟回拨与随机源失败。
- 预期结果：跨用途/连接/节点及过期/重复证明拒绝；仅服务端发出字段可接受，重取使旧挑战失效；墙钟回拨不续期，随机源失败关闭入口，不降级随机算法。

### NA-005 配对码和状态机边界

- 用例级别：L3
- 操作步骤：用可控时钟/随机源枚举300秒边界、所有合法终态转换、码碰撞、8个请求及第9个、编码大小写/中间连字符/OI模糊输入；验证CLI非交互模式。
- 预期结果：合同§3码格式及容量生效、碰撞重生成；终态不可逆，码固定不可续期；非交互不建待确认；临时秘密不落库，结果留存与consumed不改变已提交信任。

### NA-006 预算与统一错误

- 用例级别：L3
- 操作步骤：按合同§5各预算测试上限前/等于/超过值、固定60秒窗口、1024来源桶满及空闲5分钟；畸形/成功/失败均计数，伪造转发头；测16KiB/32KiB、深度8、压缩、未知路径与版本。
- 预期结果：端点、握手、连接、命令、出站、IPC上限逐项按合同断言；device pair专用流式会话不被短请求30秒期限截断，最长300秒；429和retryAfterSeconds一致；新来源不能驱逐旧桶绕限，真实TCP来源计数；错误封闭且不回显秘密。

### NA-007 批准事务与响应丢失

- 用例级别：L4
- 操作步骤：在隔离存储端口注入提交前/提交中失败、提交后响应丢失；同时取消CLI；查询数据库与浏览器恢复连接。
- 预期结果：提交前失败不报批准、不留半信任；原子保存公钥/状态/审计；提交后取消提示用device revoke，不能回滚信任；已保存key可通过clientId=null持有证明恢复ID，未知/撤销统一拒绝。

### NA-008 撤销线性化与发送队列

- 用例级别：L4
- 操作步骤：以可控调度在受理、提交撤销、缓存命中、入队和socket提交之间置栅栏，连接同时订阅/操作；注入撤销事务失败。
- 预期结果：提交后不新受理/入队/发送，清空尚未提交socket队列且所有连接失效；先已受理动作按真实结果收敛，不承诺回滚已交网络栈数据；提交失败不虚报撤销，状态和审计原子。

### NA-009 身份初始化崩溃一致性

- 用例级别：L4
- 操作步骤：在隔离目录的密文临时写入、刷新、更名、meta事务各阶段中止进程；构造只有key、只有meta、key与meta不符、未知schema；重启。
- 预期结果：仅key无meta且可解密同一公钥可完成meta；其他既有身份缺失/损坏明确失败；未知schema不降级、不生成替代身份、不清空目录。

### NA-010 运行时审批边界

- 用例级别：L4
- 操作步骤：在应用策略边界用后续能力测试适配器请求需要审批的操作，提供有效管理身份及无/过期审批。
- 预期结果：仅资格有效不能得到审批授权，失效审批不复活；后续Harness为明确mock，仅证明本change策略边界，不声称真实Harness集成。

### NA-011 启动及失败释放

- 用例级别：L5
- 操作步骤：使用合同§3命令acp-remote daemon start前台启动；分别提供有效配置、无效origin/TLS、不可写存储与占用端口，失败后修正配置重试。
- 预期结果：只有完整初始化成功才就绪；单节点即可管理；失败原因可定位且不泄漏私密路径/材料，不假就绪；本次锁/监听/句柄释放，用户原数据与身份不变。

### NA-012 本机所有者与独占

- 用例级别：L5
- 操作步骤：owner与另一Windows用户分别运行daemon status/device pair/device list/device revoke/daemon stop；伪造SID、远程命名管道、模拟失败，再启动同目录第二节点。
- 预期结果：DACL及服务端token SID双校验，异用户/远程/校验失败拒绝；同目录第二实例拒绝，不能删未知PID抢锁；owner无需Web身份仍可list/revoke恢复管理。

### NA-013 网络可达不等于认证

- 用例级别：L5
- 操作步骤：分别经localhost和真实Tailscale路径打开管理连接；未认证请求node.info/device.list/订阅，再以可信未撤销Web身份执行相同操作。
- 预期结果：未认证不交付资料、不建订阅；有效身份可管理本节点，同一身份资格不因路径改变；Tailscale只证明真实网络路径，不替代应用认证。

### NA-014 认证错误与秘密保护

- 用例级别：L5
- 操作步骤：发送已知标记的无效proof、已知/未知/撤销ID；收集响应、普通日志、审计、HTTP历史、浏览器storage与网络记录。
- 预期结果：统一失败不能枚举身份，标记的code/proof/私钥不回显或入日志；允许的公钥/身份元数据与秘密区分；测试证据脱敏，不复制秘密。

### NA-015 正常关闭与重启

- 用例级别：L5
- 操作步骤：节点有未完成配对、多连接订阅及在途操作时由owner运行daemon stop；尝试新操作；检查进程/端口/管道/锁并重新启动。
- 预期结果：按合同顺序停止受理、使待配对失效、收敛工作、关闭连接/监听/存储/密钥；仅完成后报成功；记录可恢复，无孤儿任务；本change无Harness子进程树验收声明。

### NA-016 关闭失败诚实报告

- 用例级别：L5
- 操作步骤：用明确故障注入入口阻塞在途工作或令必要提交失败，触发stop并观察15秒期限。
- 预期结果：期限耗尽或保存失败明确报告未完成项，不报全部释放；尽力清理自有资源，不删除数据掩盖失败；故障注入入口待实现，不能拿普通正常关闭代替。

### NA-017 持久信任与损坏保护

- 用例级别：L5
- 操作步骤：配对A/B并撤销B后正常重启，A/B重新认证；只对本例复制数据目录注入key不可读/损坏、meta不符，再启动。
- 预期结果：nodeId和A稳定身份保留、B仍拒绝；损坏启动明确失败，不重置/清空/替代身份；DPAPI跨用户解密不能成功。

### NA-018 Web认领不直接授权

- 用例级别：L5
- 操作步骤：owner交互运行acp-remote device pair，从其HTTPS地址打开Web，核对完整节点指纹并输入有效码；尚未本机批准时尝试管理。
- 预期结果：仅进入待确认，管理拒绝；终端展示绑定客户端名称/公钥指纹/到期；Web是本change唯一远程客户端载体，不要求移动扫码。

### NA-019 明确批准与重新认证

- 用例级别：L5
- 操作步骤：完成NA-018，owner核对后明确批准；Web验证hostProof、保存clientId后建立新WSS并完成身份认证，读取列表。
- 预期结果：可靠提交后才报告成功；批准本身不替代认证；列表出现稳定ID/名称/信任状态，管理操作归属对应身份。

### NA-020 拒绝默认回车取消

- 用例级别：L5
- 操作步骤：分别在新配对中认领后拒绝、直接回车、Ctrl-C、关闭CLI/IPC断开；再请求状态和管理；网络尝试批准。
- 预期结果：提交批准前均不建立信任、配对失效、管理拒绝；网络不存在批准入口；提交后的取消按NA-007处理不声称回滚。

### NA-021 并发认领与幂等重试

- 用例级别：L5
- 操作步骤：两真实浏览器配置文件使用同一码并发认领；重发获胜者原始proof、相同requestId不同proof和其他key新挑战proof；重复使用有效/撤销公钥配对。
- 预期结果：仅一key绑定，终端只批准该key；原始重试同结果不新建身份，变更proof冲突；其他key不能替换；有效或撤销key不能新建身份/复活。

### NA-022 失效材料与状态重取

- 用例级别：L5
- 操作步骤：分别过期、取消、拒绝、进程重启后用旧码/挑战/证明lookup、claim、status；批准后丢失响应并在留存期内重取，再超出留存期。
- 预期结果：未认证统一pairing_invalid，不恢复旧配对；仅绑定key有效proof能看终态，未批准clientId=null；批准结果按max(原到期,批准后60秒)保留，过期清临时材料但不撤销信任。

### NA-023 在线猜测与资源上限

- 用例级别：L5
- 操作步骤：经真实HTTP/WSS入口按合同§5逐端点超过阈值，包括错误码、畸形请求、status高频、握手和连接耗尽，再尝试正常客户端。
- 预期结果：按固定窗口与真实来源限制，内存与桶有界，不延长请求寿命，不泄漏配对秘密/客户端资料；慢连接到达1MiB或256帧先到者关闭。

### NA-024 节点替换与来源约束

- 用例级别：L5
- 操作步骤：以测试TLS与替代节点key返回错nodeId/origin或无效hostProof；既有Web绑定下更换节点key；构造缺失/null/多值/异源Origin、错Host及转发头。
- 预期结果：客户端停止且不保存/覆盖绑定，不为替代key签proof；所有配对POST和WSS严格同源/Host，原生无Origin拒绝；不加载第三方资源。

### NA-025 公钥持有证明与有效身份

- 用例级别：L5
- 操作步骤：以已知ID/名称/公钥但无私钥连接，发错签名、跨连接重放、认证前管理帧、5秒无hello、15秒未认证；再以有效key认证，多连接至8及第9个。
- 预期结果：伪造和重放拒绝，超时4408、提前管理拒绝；有效身份操作归属正确，最多8连接且新连接不踢旧；新鲜挑战原子一次消费。

### NA-026 持久提交成功与失败

- 用例级别：L5
- 操作步骤：正常批准后刷新浏览器/重启节点读取稳定ID；隔离存储故障令批准事务失败后读状态和管理结果；用配对证明尝试长期认证。
- 预期结果：成功记录含稳定ID/公钥/名称/状态并可恢复；保存失败不宣称持久权限，失败无半信任；配对凭据不能跨用途认证。

### NA-027 列表与撤销可靠提交

- 用例级别：L5
- 操作步骤：A列出并撤销B，重发相同requestId、相同ID新请求与同requestId不同载荷；使用可控时钟跨24小时、填满10000条结果及模拟磁盘满；另例在提交注入失败；验证B重连和状态。
- 预期结果：提交成功才确认，B认证/管理拒绝；重复撤销不复活，相同请求原结果、不同载荷冲突；失败明确且不能误标撤销，状态和审计同事务；按RFC8785规范化摘要判重，24小时内不驱逐未过期结果，满额拒绝新revoke；过期ID可新受理但撤销不复活，list/info不逐次落幂等结果，审计不逐帧膨胀。

### NA-028 已有订阅全部撤销与自撤销

- 用例级别：L5
- 操作步骤：B建多个真实连接并订阅devices；A撤销B，B并发发命令和读取推送；再单独验证A自撤销及owner本机恢复入口。
- 预期结果：撤销后不受理新命令、不提交新受保护发送、队列清空/连接关闭；提交前已发字节不作收回承诺；自撤销允许无成功响应，以list/重连核实结果。

### NA-029 管理输出最小化

- 用例级别：L5
- 操作步骤：有效Web准备超过64条测试客户端记录，以{cursor:null}调用device.list并逐页读取；检查32条上限、最后不足32条、nextCursor结束、无遗漏重复；跨认证身份/节点使用游标、篡改游标，在60秒到期前后续读；分页期间分别建立新信任与撤销令revision递增后重用旧游标。首次devices订阅并连续变更多次，观察通知合并，再主动分页查询；检查成功/失败/撤销审计输出、页面与静态页。
- 预期结果：device.list只接受{cursor:null|字符串}，每页最多32条；游标绑定节点/认证客户端/revision且最长60秒，跨范围/篡改/过期拒绝，revision变化返回409 stale_cursor并从首页重取；items仅含合同§4允许的ID、名称、状态、指纹及时间，不返回完整公钥。devices首次和后续只发{v:1,type:devices.changed,revision}，允许合并到最新revision，不发送全量列表，客户端据此分页刷新；每次取回仍检查有效身份。列表不含私钥、proof、配对秘密；公开静态页无管理数据；响应no-store/no-referrer/同源CSP生效，配对码不入URL/storage/第三方请求。

### NA-030 信任与后续审批隔离

- 用例级别：L5
- 操作步骤：在验收可见的策略测试入口以已认证身份调用需要审批的测试能力，分别无审批/已失效审批。
- 预期结果：身份仅判定操作者资格，不能自动批准或恢复审批；首期没有Harness，不虚构可运行Harness页面；入口必须在实现时建立，未建立则此例未执行。

### NA-031 浏览器密钥持久化与丢失

- 用例级别：L5
- 操作步骤：真实Chromium IndexedDB保存不可导出CryptoKey并读回签验后认领；测试写入失败/私密模式/已有记录key不可读、刷新、完全退出浏览器全部进程后重新打开（保持同一配置文件）、清站点数据和HTTPS origin改变。已有有效身份时，在同origin打开页面、临时断线及节点恢复后观察自动认证；分别触发4401、4410和节点身份不匹配，在重连等待/认证期间主动断开或离开页面，并在命令已发送但结果未知时断线。
- 预期结果：即时读写签验只证明当前存储可用，不代替完整关闭后的持久性验收；完全关闭再打开仍以原key/ID认证才可声称跨关闭持久；持久化失败不认领不假成功；key不传节点/不入localStorage；已有坏记录不静默新key；丢失提示本机撤销再新配对，新origin不迁移私钥/自动信任。同origin已有有效身份自动复用原key/clientId，临时断线及节点恢复后自动重连并重新认证，无需点击认证，页面不提供“重新尝试认证”按钮；4401/4410停止旧身份重试并直接展示配对页，使用新key重新配对且仍需本机批准；节点身份不匹配保留错误且不替换身份，不循环连接；主动断开或页面离开取消待执行重连和在途认证；结果未知的命令不自动重发。

### NA-032 Windows Web真实闭环

- 用例级别：L6
- 操作步骤：真实Windows x64 owner启动直接TLS节点；真实Chromium A/B经可信证书HTTPS打开Web，device pair认领、本机批准、认证、list/订阅、撤销B、重启并A/B重连、owner关闭；至少一轮走真实Tailscale路径。
- 预期结果：完整节点单机闭环可观察；真实SQLite/DPAPI/named pipe/WebCrypto/IndexedDB/TLS/Tailscale均不得mock；A有效B拒绝、所有资源关闭；无Harness和移动端验收声明。

### NA-033 配对提示与错误可理解性

- 用例级别：LX
- 操作步骤：Harness先验证真实终端和Web的名称/指纹展示、批准、默认回车拒绝、Ctrl-C取消、超时、批准后取消、密钥丢失和节点替换等客观行为，并准备相应实际界面。用户只查看这些界面，判断自己能否理解当前状态、应核对什么、失败原因及下一步操作；无需重复执行Harness已验证的流程。Harness无法替代用户本人的理解和使用感受，因此仅这部分需要人工介入，用户只需回复通过或不通过。
- 预期结果：用户能理解待确认/已批准/已认证/失败的区别、指纹核对及失败后的操作，批准后取消明确指向撤销，不误以为移动端已可用。默认拒绝、身份匹配与失败无假成功等客观断言由Harness按NA-018–025/031等相关用例验证，客观覆盖不足不转为用户职责；Agent记录用户原始结论，未回复记待确认。
