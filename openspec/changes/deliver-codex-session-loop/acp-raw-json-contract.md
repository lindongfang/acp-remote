# ACP 原始 JSON 基础层合同（待独立预审）

2026-10-31，Q3/WP3。行为依据是本 change 的 acp-platform-constraints「不新增输入正文和输出缓存容量上限」、structured-history「结构身份与来源顺序」及 CL-095/103。这里只冻结表示与解析基础层；不是整个协议、缓存或真实 Harness 验收。当前是候选，未获独立用例和方案通过前不编码。

## 范围与依赖

现有 ACP 将每帧递归解成 Value，并使用32MiB、96层和131072结构 token 的旧门槛。只删常量会留下递归解析、复制和析构风险。先建立不递归保存未知子树的原始 JSON 基础层，再在后续子阶段整体替换 RawMessage/Strategy/transport/P1/P2 调用；不建立隐式延迟全树 Value 的兼容层。基础层未接线时，旧门槛仍是待实现差异，不能声称 CL-095 完成。

工程依据是已固定 serde_json 1.0.154 的本地源码和 q3-raw-feasibility 实验；参考已有只读可行性报告。不升级版本，只启用现有依赖的 raw_value 功能；它不增加 registry 包，锁文件应保持原字节。本地包清单许可证为 MIT OR Apache-2.0，现有工程已使用，feature 无平台专属代码；本轮没有另行联网复核上游维护状态或安全公告，不代替最终供应链审查。管理认证 strict_json 的八层规则及其全部调用保持不变。

WP2 登记/owner 验收可与本合同和独立定义并行。产品实现仍在 WP2 集成后进行，除非 plan 明确记录无交叉基础层独立实施的调整并经 reviewer 确认。主执行负责此合同及后续源码；tester/reviewer 只在各自隔离目录写定义/报告。新模块写范围为 app/src/adapters/acp/raw_json.rs、其子模块/开发测试、ACP 模块声明、app/Cargo.toml 的 feature 一处和模块文档；不改任何已有产品 API 或接线。新模块后续接线不能绕过另一个完整协议迁移门槛。

## 表示和准确 API

新增公开 `adapters::acp::raw_json`，不依赖 domain、SQL、授权或生命周期对象。类型不提供 Deserialize 或可修改内部索引的入口，不含 unsafe。所有 Debug 仅输出类型、种类、字节数或节点数，不输出键、字符串、数字或正文。

- `RawDocument::parse(bytes: Vec<u8>) -> Result<RawDocument, RawJsonError>` 接受任意一个完整 JSON 值，允许 JSON 首尾空白，拒绝尾随第二值和无效 UTF-8；消息 envelope 的 object 要求由后续 RawMessage 单独执行。`bytes(&self) -> &[u8]` 保留输入全部原始字节；`root(&self) -> RawValueRef<'_>`；`root_owned(&self) -> OwnedRawValue`。文档可 Clone，共享原始文本和索引，不复制全文。
- `RawValueRef<'a>` 是 Copy 的只读借用。`kind(self) -> RawKind`；`fragment(self) -> &'a str` 返回该值原片段（不含其周围空白）；`object(self) -> Option<RawObject<'a>>`；`array(self) -> Option<RawArray<'a>>`；`string(self) -> Result<String, RawJsonError>` 仅解码字符串值，否则 WrongType；`boolean(self) -> Option<bool>`、`integer(self) -> Option<i64>`、`unsigned(self) -> Option<u64>`。后三者不把字符串、浮点、指数或超界整数冒充相应类型；`is_null(self) -> bool`。integer 允许负号，`-0` 返回 Some(0)；unsigned 只接受无负号的十进制整数，`-0` 返回 None；`1.0`、`1e0` 对两者均为 None。`to_owned(self) -> OwnedRawValue` 共享整个文档，明确它会延长文档存活期。
- `RawObject<'a>` 和 `RawArray<'a>` 均为 Copy 的只读句柄，各方法以 self 接收。RawObject 提供 `len/is_empty/get(&str) -> Option<RawValueRef<'a>>/iter`，iter 按原文成员顺序产生 `(&'a str, RawValueRef<'a>)`；键是解码后的完整字符串，不做 Unicode normalization。无需通过 HashMap 暴露或改变顺序。RawArray 提供 `len/is_empty/get(usize)/iter`，按原文顺序产生 `RawValueRef<'a>`。两种 iter 均返回 `impl ExactSizeIterator + DoubleEndedIterator + 'a`，元素类型如上，不能将借用缩短到局部句柄变量的存活期。所有 get 超界或缺键返回 None。
- `OwnedRawValue` 可 Clone，`as_ref(&self) -> RawValueRef<'_>`，无递归克隆；其原文和节点不能独立修改。可以借此保存未知原结构，不依赖原 RawDocument 变量继续存活。
- `RawKind` 为 Object/Array/String/Number/Boolean/Null。
- `RawJsonError` 为 InvalidJson/DuplicateField/InvalidUnicode/WrongType/Allocation，实现 Clone/Copy/Debug/Eq/PartialEq/std::error::Error；Display 均为静态错误文案，不附原文、键名、库错误原文或位置附近片段。所有公开文档/值/对象/数组句柄的 Debug 均脱敏；RawKind 为 Clone/Copy/Debug/Eq/PartialEq。显式 try_reserve 失败用 Allocation，但不承诺依赖库或所有内存分配均可优雅失败；没有无限物理内存保证。
- `RawValueRef::compact(self) -> Result<String, RawJsonError>` 仅删除字符串之外的四种 JSON 空白（空格、TAB、CR、LF），不使用 Unicode is_whitespace。数字词法、字符串 escapes、字段顺序和全部内容保持不变，不插入 literal LF。它是新出站片段的编码工具，原始输入字节始终另保留；不提供 Serialize 派生使多行 raw 片段被意外嵌入 NDJSON。后续 transport 必须显式采用它或等价安全的浅 envelope 写法。

## 解析算法与安全边界

先验证 UTF-8，再用 `serde_json::from_str::<&RawValue>` 校验一个完整 JSON 值及结束；它的 ignore_value 使用显式栈，未知数字只验词法，不强转 f64。UTF-8 或库语法错误统一映射 InvalidJson，不带库原文；这一完整语法检查优先于后续重复键/Unicode 检查。随后对已通过语法检查的文本做一次迭代扫描，建立平面节点表：每节点仅保存原文 Range、种类、对象有序键及子节点下标，或数组子节点下标。扫描中遇到的首个孤立 surrogate 报 InvalidUnicode，首个解码重复键报 DuplicateField；不承诺一个输入多个不同错误时给出所有错误。节点不能直接包含子节点；Clone/Drop 均不沿 JSON 层级递归。父层用显式 Vec 状态栈，不设深度、结构 token、对象成员、数组长度或正文的固定阈值。

每个当前 object 独立维护解码键集合，发现相同解码键就拒绝：`"a"` 和 `"\u0061"` 冲突，兄弟 object 的同名键合法，组合字符与预组合字符不自动视为相同。键只在处理它时解码，不能逐层重解整个子树。HashSet 用标准随机散列；平面结构保持成员原顺序。语法检查加索引扫描为线性全文遍历，不采用逐深度重复读取未知子树的 O(n²) 做法。按需 get 可以线性扫描当前对象；固定协议路径读取不得触发全树递归解析。

所有 JSON 字符串（含未知值）沿用旧 StrictValue 的 Unicode scalar 有效性：原 UTF-8 必须有效；`\uD800`–`\uDBFF` 后须紧跟 `\uDC00`–`\uDFFF`，孤立高/低 surrogate 均拒绝，合法 pair 保留原 escape。这是保持已有合法性语义，不是新增内容门槛。字符串扫描需正确区分转义反斜杠、引号、字符串内括号，不能仅数括号。数值可超出 f64/u64 范围，未知数原样保留；integer/unsigned 按精确十进制整数词法和范围返回 Some，否则 None，不作浮点近似。

输入 Vec 经 UTF-8 转为共享 str，避免额外的递归值树。索引只保存受检边界；不存自引用，不用 lifetime transmute，不对未验证输入 unchecked 取片段。正常路径不用 unwrap/expect/panic；计数/长度采用 checked 运算。敏感字符串可由调用者明确读取，但 Debug/错误不能泄漏，底层不记录日志。未知结构不能静默变字符串或丢弃。

## 验证与交付门槛

独立 tester 在编码前给出固定 API/用例定义，由 reviewer 核对上述全部分支；测试实际执行后另记结果。至少覆盖：深度超过96/128且至少20000层、结构 token 超131072、正文超过32MiB；完整原字节、顺序、借用与 owned 生命周期；超大整数/指数词法和精确整数拒绝；各层 escaped 重复键及兄弟同名；畸形语法、无效 UTF-8、根类型、尾随数据；全部字符串 surrogate/escape 边界；深层 clone/drop、compact 多行输入不破坏字符串/数字；敏感 canary 不进所有 Debug/错误；管理 strict_json 八层限制保持。大内容和深度用确定性程序生成，不包含私人数据。

正式验收在固定源码副本、独占 Rust target 中执行，记录实际用例数量、逐分支结果及 SHA。开发单测不代签。完整工程 fmt/clippy/check/build 在实际集成结果执行；Cargo.lock 原字节复核。当前原 ACP/API 资产在基础层子阶段应保持原字节并回归；未来 API 替换由 tester 另做可追溯机械适配，不能删除原来源、唯一响应、错误和未知字段断言。

这一步不证明32MiB帧入口已移除、输出缓存无上限、真实协议兼容、独立 reader/writer 或浏览器能力。后续 RawMessage 和所有调用迁移、实际 outbox/framing、无上限共享视图及真实 Harness 仍需完成；任务3.1/3.2/6.3及 CL-095 不因该基础层完成勾选。
