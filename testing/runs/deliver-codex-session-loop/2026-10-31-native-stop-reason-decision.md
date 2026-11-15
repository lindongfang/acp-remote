# 原生非成功结束的用户决定

用户于当前实施会话明确选择：

> 保留结束原因，暂停旧队列，手动继续（建议）

所答问题明确限定 token/请求轮数上限、拒绝执行及普通取消；说明影响为 Run 显示结果及是否自动发送下一条。明确成功继续自动推进，明确失败按既有规则挂起。

已将行为权威补入 session-execution/spec.md 的“非成功结束保留原因并暂停队列”，design.md 与 implementation-contract.md 只链接引用。未知事实不补造完成，立即发送仍按已有独立授权流程处理；原生权限取消义务不变。

本记录仅保存用户决定和规范落点，不是实现或验收通过。独立用例更新与工程合同复核仍需完成。

固定输入：

- session-execution/spec.md：d2c38379b6edba8c7556f13dc7cdf5ccdffd48a9de018caad2d31c398ce516c7
- design.md：dea6d897889b6b007082d28480c17b137f8337ff21e4942f05f86f491298efaa
- implementation-contract.md：cc4f9ef3cc80e2494af63312364cd157ebdb163d3caa10c9135880b0fe8a54ec

严格 OpenSpec 结构校验 exit0；该结果不代替语义复核、测试或真实 Harness。
