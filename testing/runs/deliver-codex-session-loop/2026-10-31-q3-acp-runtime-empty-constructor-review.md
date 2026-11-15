# ExtensionFields空构造器最小工程更正

2026-10-31。**限定准许：empty()->Result<Self,SetupError>。** 这是可实现性修正，不改变产品、配置或授权规则。

亲读当前raw_json.rs：RawDocument仅提供fallible parse(Vec<u8>)，索引可返回Allocation；RawValueRef/OwnedRawValue私有字段只能来自已验证文档。原合同empty()->Self配as_raw()->RawValueRef在不改raw基础、无正常路径panic的前提下无法返回该实际错误；Option/无声fallback也不能凭空提供合法RawValueRef。

已核作者补充`revision2-supplements/empty-constructor.md` SHA256 `7f682592452cc836fd2177e87388d0cf8ad8a9584596f3fef5a1b82783924482`。唯一公开签名变化为empty返回Result<Self,SetupError>，保from_raw/as_raw。静态{}的Allocation映射SetupError::Allocation，其余错误仍用静态类别，调用加?或测试显式错误处理。禁止静态unwrap/panic、新假raw值、OnceLock失败兜底或改raw_json/依赖。

主合同64cc及限制8bb5原字节保留；候选源码、开发测试和独立资产最终绑定该补充。tester对CFG07及相关构造调用另补同版定义，原f27定义不覆盖。实际OOM不强制制造，错误映射静态核对，正常空扩展编码须真实验证。本许可不声称实现或测试已通过，也不扩大既有编码范围。
