# 原始 JSON candidate-v1 静态审查

2026-10-31。同源静态审查通过，未发现交付阻断；**独立动态验收及最终组合工程检查待完成**。不签实际帧门槛移除。

亲核 source-manifest.json SHA256 `7b1a6d69f0c931adf5d3e3bf1010f5fe85bd5877dfac0d374e67d80a30e65287`，127/127文件匹配。相对124基线仅6路径变化：新增raw_json.rs/scan.rs/tests.rs，mod.rs加公开模块，Cargo.toml仅已有serde_json启用raw_value，ACP README说明未接线范围。Cargo.lock仍 `87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324`。原RawMessage/Strategy/transport/P1/P2与管理strict_json不变。

## 亲读代码结论

- `RawDocument::parse`先String::from_utf8，再完整 `from_str::<&RawValue>`，两类错误统一静态InvalidJson；只有成功才调用index。完整语法优先，不泄漏serde诊断。
- scanner对全文单次迭代，显式Vec<Frame>，每个object独立HashSet保存解码键，成员另按原顺序保存。只对单个键用serde解码，不逐层重解子树；标准随机散列通常为期望线性处理，非耗时硬保证。
- 所有字符串先经string_end逐escape检查。高surrogate必须真实紧跟第二个反斜杠-u和低surrogate；孤立低拒绝，合法pair跳过完整两段，字面反斜杠不误当escape。无效未知值也在parse时拒绝，扫描首错顺序与定义一致。
- `Inner`保存原String及Vec<Node>；Node只含range、kind及成员/元素下标，Frame也不拥有子Frame。成功/失败Drop均不沿JSON深度递归。Arc同时共享文本与索引；Owned克隆Arc和下标；Ref/Object/Array借用Arc/成员slice，迭代器'a绑定文档，临时句柄不会缩短借用。
- 任意大数仅保存Number片段，integer/unsigned按Rust精确整数解析且unsigned显式排除负号；不转f64，-0及浮点/指数规则相符。未知节点没有Value/Deserialize/Serialize回退。
- compact先try_reserve完整原片段容量，单次字符串/escape状态扫描只删四种字符串外JSON空白；保数字/escape/顺序，不修改原text，不插literal LF。其本身不是实际NDJSON writer接线。
- 公开五句柄手写Debug只输出计数/种类；错误静态文案无source正文。私有索引/边界只由验证后indexer生成；public get用Option，无unsafe/unchecked取片段、正常unwrap/expect/panic或固定内容阈值。cursor/hex运算checked，集合增长显式try_reserve；String/Arc/serde等依赖分配并未全部可失败化，合同已准确限制保证。

新源SHA：raw_json.rs `76e45ec68ebaa35b3a7d46a9f41b810c852996e3d2fc22ad15eee500d54d2bd6`；scan.rs `e203e7318ab3aede46fce8b99d77e3a50bfd544d4003e91377f8c9b5b7c3f88f`；开发tests.rs `4e1c866075c806fb6b001a354619b1060adba0081d2e0432391bcdcba1f4c919`。已读开发8例，只作为实现验证资产，不代签独立30定义ID。

独立验收须绑定本清单及d172289c定义，实际完成20000层成功/失败/clone/drop的小栈子进程、32MiB外正文、131072外结构、全部类型/迭代借用/错误顺序/脱敏等；真实OOM继续not-run。尚未得到正式独立结果，不将主执行报告的开发检查写成reviewer实跑或产品通过。先R3后raw集成，再按组合源执行fmt/严格clippy/check/build及旧资产回归。
