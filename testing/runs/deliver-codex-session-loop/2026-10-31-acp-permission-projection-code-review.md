# §8A权限投影独立静态检视

2026-10-31，reviewer `/root/baseline_reviewer`，只读生产，未参与实现。

结论：**限定静态检视通过，当前投影修正未发现阻断。** 独立12组AP执行与新快照§6A回归仍待tester证据，不使用开发32/32或clippy代签。

实际源码摘要与交接匹配：

| 文件 | SHA-256 |
| --- | --- |
| app/src/adapters/acp/strategy.rs | cb91ed1d4395ac70af35fb422a8c97e1b2edf93f76feb39b10f638f3cf43debf |
| app/src/adapters/acp/tests.rs | b161e952885a4e29e9377da13352fd641bd6b1431d49d5f8c207906fa54e768d |
| app/src/adapters/acp/tests/permission_subject.rs | 8bd86e2c72ded8a297385e7e977bef682573e0ef831eb5679ba0677108157e4e |
| app/src/adapters/acp/README.md | b01b11a547bed4be307c761a0d87954706c271960ab600493072cfc18d59a3eb |

依据为已放行合同§8A及独立定义`180b42e5c80fce85e2a18a7367a642d9da6b5037fa0cd95c5023a239916ff526`，固定SDK及官方依据见规划报告，不混用旧快照结论。

- v2只有真正None/Null走合法无主题；command的command/cwd仍严格字符串，toolCallId仅省略/null为None，其他错类型仍报协议错误。tool_call必需toolCall/toolCallId，v1原必需工具结构未放宽。
- 其他subject先要求object及string type，合法未知变体只保留raw并unsupported；bool/数组/缺type等不再误入合法无主题。
- 仅将合法无主题及合法command可选工具的supported_subject判定修正。tool投影不冒充真实来源，未创建Run/Turn，不从title/description触发自动批准。
- 原PermissionRequest包含的session/RPC标量/raw不变。Connection端原私有来源tag、server_pending原raw摘要、重新解析原广告及单次移除逻辑未变，不接受公开投影字段篡改扩大选项。
- 未知kind、重复optionId、未广告所选项仍拒绝selected；安全cancelled可随后单次处理，写未知/取消不重发。没有新批准端点、SQL、actor或后台生命周期接线。
- 原“command必须有tool”的错误开发断言按已审新合同改成合法None，不弱化未知主体/错类型/真实来源；README同步边界，旧§6A报告保留原strategy摘要归属。

本次只读代码/diff/开发测试与摘要，未运行测试、真实Harness或Web依赖审计。待同版独立执行后再作最终限定结论；完整L5/L6、审批owner及持久裁决未签。

## 同版独立执行后的限定结论

2026-10-31追加：**§8A公共协议投影修正限定通过。** 已实际读取request_tester原目录的`acp-permission-acceptance.md`、`acp-permission-manifest.json`、`acp-permission-tests-final.log`、`acp-permission-6a-regression.log`、`acp-permission-clippy-final.log`及`acp-permission-exit-final.json`，并检查独立公开API资产的连接/合成writer与断言路径。没有以开发32项或父执行完整check代签。

- 独立AP12/12通过、0失败/忽略、57项过滤，真实退出0。
- 在本报告cb91新strategy上重跑§6A独立17/17通过、0失败/忽略、52项过滤，真实退出0；没有沿用61c9旧strategy的通过结果。
- 独立集成目标clippy `-D warnings`退出0。首次验收辅助函数lint失败及修订日志保留，未修改输入/预期或生产实现。
- reviewer再次独立计算manifest所列11个当前文件摘要，均相符，包括四份交接源码、model/transport、两套独立资产、集成入口、Cargo.lock及180b42阶段定义。独立AP资产SHA256为`cccda47e932366e486ec54400e320cf9973a9d8ede5f4778d425bc21aaf2b276`；源码四摘要仍为上表值。

限定证据支持合法absent/null subject和command可选tool、已理解字段错误拒绝、未知变体unsupported、安全单次cancelled、原广告重验、标量RPC/连接scope和取消未知不重发。协议selected只是对原广告的合法编码，不是持久批准、执行ACK或应用审批owner已处理全部请求。完整SQL裁决/真实认证资源围栏/后台审批生命周期/WSS/Web/真实Harness和L5/L6仍未验，Web npm audit依用户决定未运行。

外部管理SQL候选属于另一冻结快照；本次AP通过不关闭`MGT-CODE-001/002`，不批准其集成。
