# permission-approvals Acceptance Suite

- Source change: deliver-codex-session-loop
- Source definitions: openspec/changes/archive/2026-11-16-deliver-codex-session-loop/tests.md
- Long-term specification: openspec/specs/permission-approvals/spec.md
- Stable IDs: CL-013、CL-014、CL-015、CL-016、CL-025、CL-065、CL-067、CL-068、CL-078、CL-093

共用执行条件与归档源用例保持一致，运行结果另存 `testing/runs/deliver-codex-session-loop/`。裸 `specs` 路径在本长期套件中指 `openspec/specs`。

## 需求关联

| 长期规格路径 | Requirement | Scenario | 用例 |
| --- | --- | --- | --- |
| openspec/specs/permission-approvals/spec.md | 保留审批语义 | 不支持的请求 | CL-013、CL-034 |
| openspec/specs/permission-approvals/spec.md | 保留审批语义 | 伪造选项或真实来源失效 | CL-013 |
| openspec/specs/permission-approvals/spec.md | 审批事实独立于聊天缓存 | 展示缓存失败时已有审批 | CL-025、CL-047、CL-106 |
| openspec/specs/permission-approvals/spec.md | 唯一裁决与交付分离 | 两客户端同时审批 | CL-014 |
| openspec/specs/permission-approvals/spec.md | 唯一裁决与交付分离 | 决定交付失败 | CL-015 |
| openspec/specs/permission-approvals/spec.md | 决定持久化失败不授予权限 | 批准保存失败或提交未知 | CL-078、CL-093 |
| openspec/specs/permission-approvals/spec.md | 决定持久化失败不授予权限 | 已有允许决定随后协议要求取消 | CL-078、CL-093 |
| openspec/specs/permission-approvals/spec.md | 决定持久化失败不授予权限 | 已发决定但交付记录失败 | CL-078 |
| openspec/specs/permission-approvals/spec.md | 审批关联真实原生操作上下文 | 加载审批无输入执行 | CL-065、CL-085 |
| openspec/specs/permission-approvals/spec.md | 审批关联真实原生操作上下文 | 上下文失效后交付 | CL-068、CL-093 |
| openspec/specs/permission-approvals/spec.md | 审批关联真实原生操作上下文 | 无法证明审批归属 | CL-068 |
| openspec/specs/permission-approvals/spec.md | 请求有效性受原执行约束 | 恢复后批准旧请求 | CL-016 |
| openspec/specs/permission-approvals/spec.md | 请求有效性受原执行约束 | 前台结束后后台请求审批 | CL-067、CL-085 |
| openspec/specs/permission-approvals/spec.md | 请求有效性受原执行约束 | 前台结束时后台审批仍待处理 | CL-067 |
| openspec/specs/permission-approvals/spec.md | 请求有效性受原执行约束 | 协议不区分请求的前后台归属 | CL-065、CL-067、CL-068 |
| openspec/specs/permission-approvals/spec.md | 受理决定由节点接管 | 审批者撤销后交付 | CL-093 |
| openspec/specs/permission-approvals/spec.md | 受理决定由节点接管 | 节点接管后原请求失效 | CL-093 |

## Cases
### CL-013 不支持的请求

- 用例级别：L5
- 操作步骤：协议对端给未知主题、未知选项提示种类但真实请求及 optionId 可验证的请求；人工选择原广告选项。对照结构错误、伪造选项、已取消请求，以及无 Run/精确前后台类别但归属真实的请求。
- 预期结果：未知展示扩展安全通用展示并说明限制，用户可选择真实原选项；未知内容不自动允许。错误结构/伪造选项/失效请求拒绝，缺 Run/工作细分类本身不构成来源无效。

### CL-014 两客户端同时审批

- 用例级别：L5
- 操作步骤：两个可信客户端屏障同时提交相反决定，再以原身份重试；统计决定记录与实际 writer 响应。
- 预期结果：最多一个最终用户决定，另一方得到原裁决事实；同身份不二次交付，不把传输写入当原生执行成功。

### CL-015 决定交付失败

- 用例级别：L5
- 操作步骤：唯一裁决保存后在交付前、可能写入后分别断连接或写失败；重试原决定并查询交付事实。
- 预期结果：保留唯一决定，未交付/未知/可证发送分开；未知不自动重发，不向新连接复用旧授权。

### CL-016 恢复后批准旧请求

- 用例级别：L5
- 操作步骤：异常后恢复同原生目标，操作旧连接待审批记录；再令新连接产生同标量 ID 的新真实请求。
- 预期结果：旧请求失效且零旧决定交付；新请求重新校验并独立裁决，不复用旧允许。

### CL-025 保存失败

- 用例级别：L5
- 操作步骤：使展示缓存更新/分配失败，同时管理存储及 Harness 连接健康、真实审批有效；观察工作、审批和错误。
- 预期结果：缓存故障报告读取限制且不当原生保存失败，不仅因此取消工作；有效审批不依赖聊天强刷仍按唯一裁决处理。

### CL-065 查看激活与无 Run 加载审批

- 用例级别：L5
- 操作步骤：对已准入会话查看历史需启动/加载，产生可靠会话/连接/请求归属的审批而无平台输入；审批前后检查原生调用、Run 与名额。
- 预期结果：允许激活并计入实际资源，不伪造输入/Run，查看不默认批准；真实审批按选项独立裁决，无精确前后台类别不拒合法归属。

### CL-067 前台结束不批量失效有效后台审批

- 用例级别：L5
- 操作步骤：先独立模型再实际节点+协议对端：同目标同连接真实请求分别无Run、关联已结束Run、加载已返回及前台idle；对照请求取消、已答复、连接换代、可靠专属工作结束导致请求失效。按固定规范检查会话取消实际范围。
- 预期结果：前四事实不单独失效真实请求，不伪造Run；仍有效的后台请求可独立裁决并保护所属资源。对照事实按真实生命周期失效，不因无法细分前后台漏掉协议要求取消的请求；模型不替代节点实际来源/交付或L6。

### CL-068 真实审批来源与失效交付围栏

- 用例级别：L5
- 操作步骤：伪造目标/连接/请求与仅缺Run/精确类别对照；决定保存后交付前分别让审批者撤销、真实请求取消、连接失效及可靠专属工作结束导致请求失效。
- 预期结果：真实归属足够不借最近Run；审批者撤销本身不抑制已保存决定，由节点交付，且其新读取/操作拒绝。真实请求先失效则零允许，已写字节不声称收回，新连接不复用旧决定。

### CL-078 批准提交故障及协议安全响应分离

- 用例级别：L5
- 操作步骤：用户允许时提交失败/未知；另在允许已保存未交付后使请求失效/需 cancelled，及响应可能写入后交付结果写失败；重试并对照原决定/实际响应。
- 预期结果：未可靠保存绝不发送允许或接受冲突裁决；可安全拒绝/取消等待并报告缺失，不覆盖已存用户决定；已发/未知响应不重复，安全响应非第二个人工最终决定。

### CL-093 批准严格保存及节点接管后交付

- 用例级别：L5
- 操作步骤：真实请求的决定事务成功后挂起交付，撤销或断开审批者再恢复writer；对照决定提交失败/未知、原请求取消/已答复/连接换代，并竞争相反决定。
- 预期结果：成功保存且原请求仍有效由节点交付一次，不重验原审批者；提交不可靠零允许，至多一人工最终决定。真实请求失效抑制允许，安全响应及交付事实与原决定分开。
