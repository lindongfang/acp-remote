# ExtensionFields 空对象构造最小 API 更正

2026-10-31。绑定 revision2 主合同 `64cc19beaeb9fd96e66500aeb9b0048519b2038705b2717fc82a5ccfb924a0cb`、limits `8bb5d345f34952d47f08f50571d1ebc85cfcbefc7b05abc0ac3aaa4d4bd3e3cf`。两份冻结原文不变。

独立 reviewer 已核准此工程更正：RawDocument 只提供可能 Allocation 失败的 parse，RawValueRef 只能借已验证文档。原 infallible empty()->Self 无法同时保持 typed Allocation、无 panic 和 as_raw()->RawValueRef 的有效借用。

唯一 API 差异：

```rust
impl ExtensionFields {
    pub fn empty() -> Result<Self, SetupError>;
}
```

实现解析静态 `{}`，Allocation 映射 SetupError::Allocation；调用显式 `?`。from_raw/as_raw 和其余公开接口保持原合同，不新增假 raw 值或静态 unwrap，不改 raw_json。对外语义仍是明确空扩展，不改变配置/授权/协议规则。tester 将原 empty() 调用机械加错误处理；Allocation 错误映射静态核对，不强制制造 OOM。候选源及最终独立资产必须绑定本补充 SHA。
