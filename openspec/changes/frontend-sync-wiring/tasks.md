# Tasks

## 1. 连接契约对齐（WP1）

- [x] 1.1 修正出站连接地址：由规范化来源推导的同步连接地址，其路径改为权威端点（`/sync/v1`）。完成判据：单元测试断言构造出的 URL 路径为 `/sync/v1`，且该断言在路径写错时会变红。**证据**：`sync-client/endpoint-contract.test.ts` 断言 `new URL(syncSocketUrl(...)).pathname === "/sync/v1"`（写**字面量**而非常量，改回 `/sync` 即红）；出站地址的唯一构造点是 `syncSocketUrl`（旧私有 `syncUrlFor` 已删除），组合根改调它。
- [x] 1.2 扩展 socket 端口签名以容纳「声明的子协议列表」，并在 Web 实现里作为 WebSocket 构造的第二个参数传入。完成判据：单元测试断言假 socket 收到的声明列表非空且含约定子协议；构造点全部更新后 typecheck 通过。**证据**：端口入参改为 `SocketOpenInput{url, protocols, handlers}`；`connection.test.ts` 断言假 socket **记录到的**列表非空、含约定子协议、等于 `DECLARED_SUBPROTOCOLS`（另有一条注入别的声明，证明端口只是接缝）；`protocols` 为必填字段，8 处测试构造点被 typecheck 强制补齐。
- [x] 1.3 子协议标识取自机器登记的同一来源：通过既有的合同测试读取仓库根资产并断言前端使用的值与登记逐字相等。完成判据：该合同测试在登记被改动而前端未跟随时报红。**证据**：`endpoint-contract.test.ts` 读仓库根 `compatibility/errors/v1/errors.json` 的 `protocols.sync.subprotocol` 并逐字断言（含 `DECLARED_SUBPROTOCOLS === [registered]`）。断言位置由工单建议的 `protocol/contract.test.ts` 改为本文件：分层规则禁止 `protocol` 依赖 `sync-client`（该理由被独立检视者核实为成立）。
- [x] 1.4 处理「必需 feature 未被选中」：在握手校验处比对服务端选中的 feature 是否覆盖客户端声明的必需集合；未覆盖时进入可区分的阻断态。完成判据：单元测试覆盖「覆盖 → 正常继续」与「未覆盖 → 阻断且原因可区分」两条。**证据**：判定落在版本校验之后、验签与设备证明**之前**；`connection.test.ts` 两条（覆盖 → 发 `auth.client_proof` 且无 blocked；漏选 → **不发**证明 + `blocked{version_incompatible, required_features_missing}` + 一条带缺失 feature 的 `rejected`）。
- [x] 1.5 连接被拒时的阻断态：未选中约定子协议或协议版本不兼容时，进入与之对应的阻断态并停止自动重连，原因文案与「网络断开」可区分。完成判据：单元测试断言该情形不落回普通重连路径。**证据**：新增封闭词表 `subprotocol_not_selected` / `protocol_version_unsupported` / `required_features_missing`；`connection.test.ts` 三条（未选中子协议 → 一个出站帧都没发、`blocked` 而非 `closed`、以 4406 终止；真断网 1006 → `closed` 且无 blocked；`client.test.ts` 门面级断言阻断后 `connect()` 抛错 ⇒ 结构性停重连）。

**本组实测（2026-10-16，主检出）**：`cd clients/app && npm run check` = 58 files / 686 passed + `expo export` 成功；`node e2e/run-e2e.mjs` = 49/49（含 CR-2 出站 URL 与 CR-5 升级请求声明的子协议）。独立检视 verdict=`pass`（0 finding；两处非阻断备注：e2e 检查文件里一处过期注释属 WP4 范围、`?? DECLARED_SUBPROTOCOLS` 兜底对显式空数组不生效但目前无调用方传空）。

## 2. 配对链路闭合（WP2）

- [x] 2.1 扩展配对状态查询的已批准结果：除设备标识外携带主机标识与主机公钥（协议在已批准时强制要求两者）。完成判据：单元测试断言已批准分支返回主机材料；缺材料时返回可区分的失败而非「已批准」。**证据**：`poll` 的 `approved` 分支返回 `{deviceId, hostId, hostPublicKey}`，并逐项校验公钥长度与「hostId 与二维码同一台」；`composition.test.ts` 对五种缺材料/形状不符情形断言 `kind:"error"` + `protocol.schema_invalid`（回落成「已批准」即红）。
- [x] 2.2 在已批准后写入已配对主机记录（含主机公钥与规范化来源），使后续握手能用它验证主机证明。完成判据：单元测试断言记录含三个字段；并断言旧实现「只写 deviceId」会让后续握手进入身份变化阻断态（作为反例证明缺公钥不可用）。**证据**：记录摘要 `{hostId, publicKeyBase64Url}` 取自**状态响应**的公钥（与二维码里那份刻意不同的两个常量钉住来源），键为 `paired-host/<canonicalOrigin>`（断言**只**落这一个键）；反例用例用真 `PairedHostIdentity` + 真 WebCrypto：预置无公钥记录 → 验签失败 → `identity_changed` 且**没有** `auth.client_proof`。
- [x] 2.3 配对页提供二维码内容输入入口（粘贴）与结构校验：协议版本不符、JSON 非法或字段缺失时明确失败，不尽力解析。完成判据：单元测试覆盖「合法内容进入认领流程」「非法内容明确失败且不发请求」。**证据**：入口是 `PairingPanel` 的粘贴框（页面只上交文本）；解析只有一套（复用组合根的 `readQrPayload`，含容器解包与字段长度校验）；`pairing-model.test.ts` 断言非法输入时 `claims/polls/scheduled` 全为 0，`composition.test.ts` 对真实装配断言 `fetches.length === 0`。
- [x] 2.4 读取后清除地址片段中的配对秘密，并断言秘密不进入日志、本地明文存储与可导出配置。完成判据：单元测试断言片段被清除；对秘密字面量做一次「不出现在日志与缓存快照」的断言。**证据**：`takePairingFragment` **先读后清**（只对 `data=` 形态调 `replaceState`，断言收到 `"/pair"`：路径保留、无片段），且断言清除前已进入 `claiming` 并发出认领请求；秘密只活在私有会话字段（`ClientState` 无槽位），在结束/终态/失败时丢弃。**检视 round 1 的收口**：原先声明覆盖「日志 / localStorage」的两条断言是对**空集合**的恒真断言（生产源码没有 `console.*`、也不用 localStorage），已删除、用例标题改为它真正证明的「不进缓存摘要、出站 URL 与可导出状态快照」，并保留有判别力的三条。
- [x] 2.5 配对完成后进入已配对并触发连接（不停在等待扫码）。完成判据：端到端用例（属 WP4）从「未配对」出发走到「已配对」；本任务内先以单元测试断言状态迁移。**证据**：状态机断言 `unpaired → …(claiming/pending)… → disconnected` 且 `onPaired` 在落盘**之后**；组合根断言离开 `unpaired`、恰好 1 条 socket、出站地址指向刚配对的主机且路径为 `/sync/v1`。**检视 round 1 的收口**：从**阻断态**重新配对此前永不连接（`clearBlocking()` 无生产调用方、`connectOnce()` 静默吞异常、hostId 构造期定格），已修并补断言（重新配对同一台 → socket 1→2；换一台 → 新 `hello` 的 `hostId` 等于新主机；门面仍阻断时拒绝原因被交回连接机而不是静默停在 `disconnected`）。

**本组实测（2026-10-16，主检出）**：`cd clients/app && npm run check` = 716 passed + `expo export` 成功；`node e2e/run-e2e.mjs` = 49/49。独立检视 round 1 = `changes-requested`（2 major + 2 minor，均为真缺口：配对链路两次网络 await 未收口会把页面钉在「进行中」；从阻断态重新配对后永不连接；hostId 构造期定格；两条对空集合的恒真断言），修复后 round 2 = `pass`（仅剩一处收口文案矛盾 minor，已另修：收口失败改用「配对已完成，但同步连接未能建立…」而不是复用「配对请求未能完成…」，并有页面层断言防止同屏自相矛盾）。**未做项（如实登记）**：协议 §7.1 还要求校验「当前 origin 与 secure context」，2.3 未实现（不属本次能力清单，属相邻项）。

## 3. 路由与片段争用消解（WP3）

- [ ] 3.1 核实前端实际的路由模式（读构建产物或在 e2e 中观察地址变化），把结论作为证据记录。完成判据：报告中给出核实方式与观察到的地址形态。
- [ ] 3.2 按核实结果对齐行为与源码注释：配对内容留在片段（协议要求），路由走正常路径；若实际为片段路由则改路由配置。完成判据：单元测试断言路由可达性，且源码注释不再与行为矛盾。
- [ ] 3.3 断言「二维码内容可读且清除后仍在配对页」与「片段内容不被当作导航指令」。完成判据：两条独立断言，且前者在清除逻辑被移除时会变红。
- [ ] 3.4 断言其余页面（目录页、主机页、目录详情、会话页）的可达性不因该取舍改变。完成判据：e2e 或单元测试断言各页面路径可达。

## 4. 前端自检与登记更新（WP4）

- [ ] 4.1 更新 `clients/app/e2e/page/support.tsx` 的「真实配对在运行时不可达」登记：说明配对入口现已可用，以及预置路径的保留理由（多数用例的起点），并确保该登记不再与实际不符。完成判据：`cd clients/app && npm run check` 全绿，且注释内容与当前事实一致。
- [ ] 4.2 新增一条从「未配对」出发的真实配对用例（输入内容 → 认领 → 已批准 → 写入主机记录 → 触发连接）。完成判据：该用例在配对路径被破坏时变红（不得只断言未抛错）。
- [ ] 4.3 为四处缺口各加一条可证伪断言：出站 URL 路径、升级请求声明的子协议、已批准后落盘的主机公钥、配对入口的存在与可用性。完成判据：四条断言逐条在对应实现被回退时会变红。
- [ ] 4.4 报告如实登记隔离区限制：交付是用例代码 + 隔离区能做的静态核对（无 `node_modules`、无浏览器）；真实运行由主 Agent 在合入后按 PV3/PV4 执行，不得声称已在隔离区通过。完成判据：`agent://<id>` 的 report 含该说明。

## 5. 文档同步（WP5）

- [ ] 5.1 更新 `docs/FRONTEND_DESIGN.md`：§4.1 的「主机连接与首次配对」实绩、§4.5 的 PWA 身份边界（若配对秘密清理口径需写实）、§5 的连接状态机（新增阻断态成因）、§9 的验收标准（浏览器级端到端证据）。完成判据：`npm run check:docs`。
- [ ] 5.2 更新 `README.md` 的前端现状（`clients/app` 行）与（若适用）`docs/SECURITY_DESIGN.md` 中与配对秘密清理相关的表述。完成判据：`npm run check:docs`、`npm run check:drift`。
