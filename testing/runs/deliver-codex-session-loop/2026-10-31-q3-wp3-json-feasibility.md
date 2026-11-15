# WP3 ACP JSON旧门槛移除：RawValue方向可行性

2026-10-31，只读盘点；**未实现、未试跑、未放行编码**。不改root/R3源或历史128MiB等视图合同；管理strict_json深度8规则保持独立。实际13项输入SHA见inputs.json，无联网或凭据读取。

## 判断

方向可行，且比自写完整JSON parser范围小：用现有serde_json 1.0.154的raw_value功能承担JSON语法/原片段保存；另外实现一次非递归decoded-key重复检查，以及ACP需要的浅层读取。**仅打开feature不够**，必须迁移当前全DOM调用和输出路径。当前没有性能、深层压力或真实Harness证据。

## cached primary实读依据

源根为`D:/Application/Rust/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/serde_json-1.0.154`，不是最新版推断：

| primary位置 | 亲读结论 |
| --- | --- |
| src/de.rs:1102 ignore_value | loop+scratch Vec保存容器层级，内部不递归构建子Value；ignore_integer/decimal/exponent只验数字语法，不转f64 |
| src/de.rs:1285 deserialize_raw_value | begin_raw_buffering→ignore_value→end_raw_buffering，借用原片段 |
| src/de.rs:1356–1385及初始化 | 普通typed/Value反序列化仍有默认128 recursion检查；RawValue的ignore_value路径本身不经过该宏。不能据此说任意serde解析都无深度门槛 |
| src/read.rs:665/769 raw buffering | slice路径检查UTF-8；str路径从原str切出borrow。使用Arc<str>前先可靠验证UTF-8可保原帧字节 |
| src/raw.rs:16起、Serialize实现 | RawValue可借用未解析子值，Serialize保留原JSON格式/数字词法；不会自动检查decoded重复键 |
| src/ser.rs:RawValueStrEmitter | 原片段直接write_raw_fragment，不先转Value |
| Cargo.toml:71 | raw_value=[]，本feature不引入新包；应显式启用当前已锁版本，不需要为此升级版本或启用unbounded_depth/arbitrary_precision |

## 最小可行结构

1. RawMessage只拥有原Arc<str>/bytes及必要浅索引；一次`from_str::<&RawValue>`+完整end校验负责整帧语法，另核root必须object。不要把指向自己Arc内容的借用引用存进同一struct；用借用返回的view，或Arc+已验证Range的自有fragment，避免自引用/unsafe转型。
2. 在已知语法合法的全文上做**一次**字节词法遍历：显式Vec容器栈，每个当前object独立的decoded键集合；只把键字符串交给serde解码，再比较解码后完全相同的键。`"a"`与`"\u0061"`须冲突，兄弟object同名合法；不做Unicode normalization。数字/值语法交库，避免重写完整parser。
3. 对ACP已知路径读取一层object/array，子值仍借用RawValue/fragment；已知字符串、bool、协议ID等只解析所需primitive。未知_meta/content/大整数/深层结构不强转Value；原字节仍独立保留。浅map默认可能覆盖重复键，只有全帧重复检查先成功后才可用。
4. 出站使用浅协议envelope+已验证raw片段/精确结构序列化，不能先raw→Value→to_writer。原始消息保存与新wire编码分开说明；当前prompt(Value)也要改，否则输入侧深层仍会递归解析/clone/serialize/drop。

这仍需要一个小型词法/容器状态机，不能仅数括号；但数字语法、JSON escapes解析和完整语法交给现有库，比另写完整JSON实现容易核验。

## 具体API影响

| 当前依赖 | 必要改变 |
| --- | --- |
| model::RawMessage::parse/fields | 删131072 token与96层全DOM前置；fields的`&Map<String,Value>`不能保留为隐式lazy全DOM兼容层。换只读raw object/field接口，Debug继续脱敏 |
| transport四处fields、route/route_response、权限原source核验 | 浅取jsonrpc/method/id/params/result/error；保原bytes digest/owner/唯一响应/关联错误，不靠重序列化计算来源。只改变JSON表示，不取消同时未决等非正文规则 |
| Strategy::select/update/permission/session_listing | 改读固定已知路径，未知内容留raw；列表/options用浅元素迭代。不能为方便继续把整个params/result转Value |
| P1 view_observations::context | jsonrpc/method/session/tag/toolId/state均可浅读；原始完整记录及来源核对不能丢 |
| P2A tool_projection::tool_patch | 字段的null/object/array/string形状及小标量可浅取，rawInput/rawOutput/_meta/content不必全DOM；既有知识状态/原位位置语义不能因换解析API改掉 |
| Request.params、prompt(Value)、tool_metadata_patch(Value) | 若接收任意深层用户内容，递归Value/clone/drop/输出仍会重建门槛或栈风险；须明确raw/浅patch接口，不能只修入口 |
| 独立与开发.fields断言 | tester另做新版本API适配，保原raw/来源/失败边界断言；原旧报告不回填。不能删断言或让浅读取失败静默降文本 |
| transport MAX_FRAME/FrameBuffer | 入站两处+出站writer上限都要移除，使用checked长度/实际分配错误。不能换一个更大固定内容阈值，再宣称没有平台容量限制 |

管理jws/management_ws/pairing_http继续用原strict_json::parse_object(MAX_PROTOCOL_JSON_DEPTH=8)，不把ACP的变化外溢到这些通道。

## 必须如实承认的风险

- **复杂度**：若为找深层重复键，逐层再次解析整个RawValue子树，深链会反复扫描到O(n²)。全文语法+单次键扫描可避免这一点；按需浅视图只读协议固定路径，不遍历未知深树。
- **Unicode差别**：read.rs:1076 ignore_escape只检查u后4个hex，不解码/验证scalar；旧StrictValue会解码全部字符串。新路线不能声称自动保持所有旧孤立surrogate拒绝行为。键解码与已知String仍需严格处理；未知raw字符串如何接受/标限须在合同固定，不默默变语义。
- **逐行framing**：RawValue输出保留内部空白；多行应用raw片段直接嵌入NDJSON会破坏帧。入站按行原片段无此literal LF，但新输入/配置raw需明确单行验证或仅规范化字符串外空白，数字词法/字符串值不变；不要宣称所有原字节原样输出都合法。
- **借用/内存**：Arc只解决持有期，不使自引用安全；Box<RawValue>会复制，字符串解码/键集合仍分配。解析器scratch/HashSet/Vec实际OOM仍可能导致进程终止，未证明所有分配失败都可优雅返回，更不承诺无限物理资源。
- **敏感内容**：RawValue自带Debug/Display会输出全文；不能derive含raw的Debug或记录serde源片段错误。沿现有RawMessage脱敏包装，错误/审计不含正文。
- **剩余递归**：任何未知子树回到Value，连drop/clone/Serialize也可能递归；只关闭递归检查不解决栈安全。RawValue路径的静态证明不等于全链路无递归。
- **真实性**：移除大小/深度门槛不证明结构投影、无限视图、writer/reader并发、协议关联或真实Harness支持；旧视图预算和P2B尚未完成部分仍须按现规格另实现，不能混签。

后续至少冻结raw视图API、decoded-key扫描算法/Unicode政策、出站framing及错误/取消，再独立定义：超过32MiB/96/128层/131072token、超大数字和指数原词法、深层escaped重复键与兄弟合法同名、畸形JSON/UTF-8/尾随数据、深链析构、输出raw LF、泄漏canary和适用ACP回归。当前未运行这些检查，不将源阅读当作通过。
