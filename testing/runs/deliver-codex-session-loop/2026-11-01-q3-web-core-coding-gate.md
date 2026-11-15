# Web 非视觉解码内核 v2：独立编码门槛

结论：**限定编码 PASS**，按固定 v2 有效合同在 Root 自有隔离 workspace 实施，仅新增 views 解码/记录/raw-json 模块与自有开发测试。没有剩余具体设计阻断。不是实现、独立验收、浏览器安全或 Shared83 通过；实际 Node/WSS、鉴权、Native 桥和页面仍须各自后续验收。

## 输入和权威

`session-loop-q003-root-web-core/design-v2/freeze.json` SHA256 `7607043b785c033d842679b1af28267a4f958597bc03ce430b1d1029c999697a`，全50成员亲自逐 hash 匹配。完整读取有效 prior-v1/CONTRACT.md `ec82470e2dc293fc6e954817a45b082c64a0ccb5741c9b981340637f863797cc` 与 precision.md `cd25ab9a65609ddde4a9867e95cc3871a833bb250356818badbb85ecea52410f`，核对原 assembly §3/5、有效 send-and-credit B4、structured-history、SV-067/068/079，以及实际 package/tsconfig 和现有 base64url 工具来源。

原83定义 `d2eaac1ff350dcc88c4c7bbeadf19f5fb2b674babf724a502437ef23c73bc717` 未改。research/shared-wire.rs 是 WIP 研究输入，不能替代固定 wire 表或当成最终双端动态证据。本轮没有执行基线或新代码；原 baseline check/50/build 是 Root 既有证据，不是本报告重跑。

## 精确许可

- 只新增 `clients/web/src/views/{errors,binary,records,raw-json}.ts`、同目录必要私有模块及 `clients/web/tests/views/`。现有入口、页面、源、锁、依赖、Rust、Node/WSS、管理客户端不改。不是引入 UI 渲染工作，当前不需要以视觉 skill 为编码前置；后续页面/renderer 按原 UI 和真实浏览器门槛。
- codec 严格 ACPV/1 固定72字节，u16 flags 大端只有 bit0，header u64 全程 BigInt，payload≤16384。精确72+n、全部保留位、kind、record>0、checked offset+n/last/空 SnapshotEnd 均要运行时校验。v2 的 Data.packet≥1；Credit≥0；**Detach 只有 streamId，packet 与所有非 stream 字段为0**，有效 `encodeViewDetach(streamId)` 和 `ViewDetachFrame.packetSequence:0n` 取代 v1 候选的任意 packet。streamId 是规范16字节 base64url，不能因复用工具给内容增加 JWS/控制 JSON 限额。
- decode 复制 payload，assembler 再复制到自己的私有所有权；输入或返回 Uint8Array 后续突变不得改原物。TypeScript readonly 不是运行时数组只读保证，实际 token/record 必须有私有储存、冻结 token 表与返回 copy；不暴露原 backing ArrayBuffer。只返回格式数据，不 mint Scope/读取权限/FlushReceipt，也不因可构造接口产生安全权。
- 每 assembler 只原 stream，packet 从1逐个精确递增，不接受错 offset、跨 record interleaving、换 kind/total。完整验证并完成必要复制后才推进状态。结构/分配失败使本地 assembler sealed；不得部分接受后返回错误还能继续。seal 幂等、旧半 record 不会借 rebind 复活；rebind 必须新实例。不同 namespace 的 recordId 不增加全局递增限制。
- 不按 totalLength 预分配、不添累计 record/view/引用/叶数/深度/TTL quota。只增长实际已到字节；bytes() 才 checked 转 Number 并分配实际完成的连续 copy。u64 表示超过 JS 安全整数用真实 codec 可测，现实无法分配全量不能宣称动态已测；局部可恢复 allocation RangeError 与 fatal OOM 分开，不把任意 RangeError 归为 OOM。
- scanner 先拥有输入 bytes，严格 UTF8；可同一 fatal TextDecoder 分 CPU 块验证末尾 flush，不逐网络 chunk 重置，也不强迫为验证构造一条巨大全文 string。后续单次迭代显式栈+flat tokens，完整语法检查、无递归 object DOM、无 raw 全文 JSON.parse/stringify、eval/Function。只有已验证 string literal 的显式 stringValue 可 JSON.parse；巨数/-0/指数、未知结构、键序/重复键、转义原词法始终保原片段。JSON 转义文法的保留不表示原生 parser 必须接受某个不合法 native frame，更不授予内容执行权。
- 平台 ItemProjection/Status 浅 JSON 与未知 raw 的职责分开；本阶段不新增其可信 authority 解析器，也不把 raw 重复键映成选择目标。错误 code/message/name及附带 cause/调试值不得含远端正文、原生ID、URI或错误；任意内容（含 __proto__、constructor、脚本、管理指令）只能产 token/原字节。

## 验证和后续边界

允许先写可编译模块/开发测试。交付须冻结新增源及对应开发测试、相对 Root322 精确新增 patch，并运行实际 `npm run check`、原50及新增 vitest、build，保原失败/修正日志和前后 hash；不改独立83 oracle。模块静态检查含直接/间接依赖、无 DOM/fetch/WebSocket/storage/管理动作；只调用既有 base64url 纯工具时也要在分配/输入失败处映射静态错误。

至少真实验证：72字段坏矩阵（含 Data0/Detach非0）、BigInt>2^53/u64极值及溢出、合法/非规范 streamId、SnapshotEnd空与正常空拒绝、跨 chunk 四字节 UTF8、原 bytes/copy/token 不可变、错 packet/offset/kind/跨 stream/半 record seal、错误后不能继续；raw 深20000层/大量叶迭代、语法/UTF8坏向量、巨数/转义/重复键原 lexeme、只有字符串 literal 才走 JSON.parse。大型动态应真实构造而非调小预算；fatal OOM/地址空间上限分列静态或 not-run。

SV-067/068/079 的本地子断言可据同版源码独立复用验收；**它们原本同时含 L3/L5，纯 Web 模块测试不能签全部这些 ID 已通过**。双端协议真实向量、current auth/真实 Socket 来源、信用窗口/实际 flush、Status 通知、新 stream 绑定、DOM/资源访问安全、Node/WSS/Harness 均未由此内核实现。浏览器收到任意格式 bytes 也不等于已经认证来源，不可将 decoder 的成功升级为权限。

无新增产品决策，无新依赖许可。原设计、冻结文件、83预期及 Root 都未修改。
