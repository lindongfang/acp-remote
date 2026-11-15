# deliver-codex-session-loop 用例追溯矩阵

- Source change: `deliver-codex-session-loop`
- Source definitions: `openspec/changes/archive/2026-11-16-deliver-codex-session-loop/tests.md`
- Long-term suites: `testing/suites/acp-platform-constraints/tests.md`、`testing/suites/harness-management/tests.md`、`testing/suites/management-audit/tests.md`、`testing/suites/management-input-upload/tests.md`、`testing/suites/native-session-recovery/tests.md`、`testing/suites/permission-approvals/tests.md`、`testing/suites/session-execution/tests.md`、`testing/suites/structured-history/tests.md`、`testing/suites/workspace-registration/tests.md`
- Long-term specs: `openspec/specs/acp-platform-constraints/spec.md`、`openspec/specs/harness-management/spec.md`、`openspec/specs/management-audit/spec.md`、`openspec/specs/management-input-upload/spec.md`、`openspec/specs/native-session-recovery/spec.md`、`openspec/specs/permission-approvals/spec.md`、`openspec/specs/session-execution/spec.md`、`openspec/specs/structured-history/spec.md`、`openspec/specs/workspace-registration/spec.md`
- Definition totals: 9 capabilities, 133 stable cases (CL-001–123 + ACPI-R01–R10).
- 覆盖含义：下表保留源用例中的 Requirement/Scenario 关联关系及稳定 ID 分布；执行结果另存 `testing/runs/deliver-codex-session-loop/`。

## Coverage Matrix

| Capability | Requirement | Scenario | Stable cases | 定义覆盖 |
| --- | --- | --- | --- | --- |
| acp-platform-constraints | 额外限制有明确归属与依据 | 新增或保留限制 | `CL-103` | 已引用 |
| acp-platform-constraints | 不以历史能力统一封锁其他能力 | 无重放但能执行 | `CL-047`、`CL-106` | 已引用 |
| acp-platform-constraints | 长连接不设累计请求次数上限 | 长连接超过原累计阈值 | `CL-108`、`CL-109`、`CL-116` | 已引用 |
| acp-platform-constraints | 不新增输入正文和输出缓存容量上限 | 超过旧平台正文或视图预算 | `CL-026`、`CL-049`、`CL-095` | 已引用 |
| acp-platform-constraints | 核对节点接管与协议真实性边界 | 原提交者撤销与协议结果未知 | `CL-092`、`CL-093`、`CL-100`、`CL-104` | 已引用 |
| acp-platform-constraints | 兼容性核对覆盖组合时序与证据 | 新旧规则冲突检查 | `CL-103`、`CL-107`、`CL-108`、`CL-116` | 已引用 |
| harness-management | 历史能力不作为统一接入门槛 | 对端只有无重放恢复 | `CL-047`、`CL-106` | 已引用 |
| harness-management | 历史能力不作为统一接入门槛 | 标准重放无需厂商适配 | `CL-048`、`CL-054` | 已引用 |
| harness-management | 历史能力不作为统一接入门槛 | 重连后失去重放能力 | `CL-053` | 已引用 |
| harness-management | 按协议版本与协商能力兼容 | 不同厂商使用相同标准能力 | `CL-032`、`CL-048` | 已引用 |
| harness-management | 按协议版本与协商能力兼容 | 两个版本分别接入 | `CL-033`、`CL-034`、`CL-041` | 已引用 |
| harness-management | 按协议版本与协商能力兼容 | 无共同可用版本 | `CL-035` | 已引用 |
| harness-management | 按协议版本与协商能力兼容 | 按规范协商回退 | `CL-036`、`CL-058` | 已引用 |
| harness-management | 按协议版本与协商能力兼容 | 版本规范尚未核实 | `CL-037` | 已引用 |
| harness-management | 优先最高共同可用协议 | 双方支持高低多个版本 | `CL-058`、`CL-060` | 已引用 |
| harness-management | 优先最高共同可用协议 | 高版无共同支持时协商低版 | `CL-058` | 已引用 |
| harness-management | 优先最高共同可用协议 | 对端合法选择低版但未解释原因 | `CL-058` | 已引用 |
| harness-management | 优先最高共同可用协议 | 重连重新优先高版 | `CL-059`、`CL-060` | 已引用 |
| harness-management | 优先最高共同可用协议 | 高版业务失败不降版重试 | `CL-059` | 已引用 |
| harness-management | 优先最高共同可用协议 | 更高版本尚未实现 | `CL-058` | 已引用 |
| harness-management | 运行时按连接绑定协议版本与能力 | 同一 Harness 协商不同版本 | `CL-033`、`CL-059` | 已引用 |
| harness-management | 运行时按连接绑定协议版本与能力 | 重连后的版本改变 | `CL-016`、`CL-059` | 已引用 |
| harness-management | 运行时按连接绑定协议版本与能力 | 协商后收到不符版本消息 | `CL-033`、`CL-036` | 已引用 |
| harness-management | 调研证据不成为版本绑定表 | 未在原报告出现的程序版本 | `CL-038`、`CL-048` | 已引用 |
| harness-management | 不支持的标准操作直接报错 | 接入缺少必需能力 | `CL-040` | 已引用 |
| harness-management | 升级复验与适配分离 | 标准兼容升级 | `CL-038` | 已引用 |
| harness-management | 升级复验与适配分离 | 升级造成不兼容 | `CL-039` | 已引用 |
| harness-management | 登记可追溯的 Harness | 能力未证实 | `CL-005` | 已引用 |
| harness-management | 能力准入与证据状态分开 | 新程序版本无历史报告 | `CL-005`、`CL-048`、`CL-058` | 已引用 |
| harness-management | 能力准入与证据状态分开 | 声明了未实现能力 | `CL-005`、`CL-048` | 已引用 |
| harness-management | 能力准入与证据状态分开 | Harness 不支持标准能力 | `CL-005`、`CL-040`、`CL-048` | 已引用 |
| harness-management | 能力准入与证据状态分开 | 实际行为与声明不符 | `CL-039` | 已引用 |
| harness-management | 能力准入与证据状态分开 | 禁止有副作用的自动探测 | `CL-048` | 已引用 |
| harness-management | 历史与激活能力独立验证 | 只有恢复能力 | `CL-047`、`CL-106` | 已引用 |
| harness-management | 会话工作观测与回收能力分别呈现 | 接入可交互但不能自动回收 | `CL-069` | 已引用 |
| harness-management | 默认继承节点环境 | 默认继承并显式覆盖 | `CL-055`、`CL-057` | 已引用 |
| harness-management | 实例由节点拥有 | 页面断开 | `CL-006` | 已引用 |
| harness-management | 实例由节点拥有 | 共享实例释放 | `CL-007` | 已引用 |
| management-audit | 管理动作可追溯 | 释放失败审计 | `CL-008` | 已引用 |
| management-audit | 必要管理状态与附加审计分开 | 审计容量不足 | `CL-094` | 已引用 |
| management-audit | 必要管理状态与附加审计分开 | 安全控制后补记事实 | `CL-084` | 已引用 |
| management-audit | 审计保留与去重清理分别管理 | 审计已可清理但去重事实仍受保护 | `CL-080`、`CL-083` | 已引用 |
| management-audit | 审计保留与去重清理分别管理 | 保留期内达到容量上限 | `CL-083`、`CL-094` | 已引用 |
| management-audit | 审计保留与去重清理分别管理 | 默认保留期边界 | `CL-083` | 已引用 |
| management-audit | 审计不泄漏敏感内容 | 已撤销客户端访问 | `CL-009`、`CL-031` | 已引用 |
| management-input-upload | 上传服务原子可用 | 服务无法完整发布 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 上传服务原子可用 | 仅控制面启动 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 连接持有唯一上传所有权 | 两条连接不能共享 uploadId | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 连接持有唯一上传所有权 | 上传途中连接结束 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 四条上传命令使用稳定 JSON | 命令字段、标识与结果 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 四条上传命令使用稳定 JSON | 阶段、释放与字段拒绝 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 四条上传命令使用稳定 JSON | 查询反映真实准备阶段 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 四条上传命令使用稳定 JSON | 释放与重复释放 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 文本错误映射稳定 | 上传文本错误码 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 文本错误映射稳定 | 正文解析失败 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | ACPI 数据帧与 ACK 使用唯一出站顺序 | Data 头、偏移与 ACK | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | ACPI 数据帧与 ACK 使用唯一出站顺序 | 成功接收及最后块重试 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | ACPI 数据帧与 ACK 使用唯一出站顺序 | ACK 与随后文本结果排队 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 二进制错误和关闭优先级明确 | Binary 分类、错误与关闭码 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 二进制错误和关闭优先级明确 | 停止先于待处理坏帧 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 二进制错误和关闭优先级明确 | 上传错误时节点仍运行 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 撤销先阻断出站再完成生命周期封存 | ACK 发送前撤销先提交 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| management-input-upload | 异步命令取消不产生无主任务 | finish 已交接但响应观察取消 | `ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10` | 已引用 |
| native-session-recovery | 原生身份不依赖启动入口 | 多个入口访问同一会话 | `CL-056`、`CL-057`、`CL-086` | 已引用 |
| native-session-recovery | 原生身份不依赖启动入口 | 不同来源中的相同短标识 | `CL-056` | 已引用 |
| native-session-recovery | 原生身份不依赖启动入口 | 接入来源未配置 | `CL-056` | 已引用 |
| native-session-recovery | 原生绑定与按需恢复 | 原生状态缺失 | `CL-010`、`CL-045` | 已引用 |
| native-session-recovery | 查看允许激活进程 | 查看需要加载 | `CL-065`、`CL-085` | 已引用 |
| native-session-recovery | 查看允许激活进程 | 查看触发权限请求 | `CL-065` | 已引用 |
| native-session-recovery | 释放确认与管理事实 | 关闭或管理记录失败 | `CL-008`、`CL-011` | 已引用 |
| native-session-recovery | 释放确认与管理事实 | 存储失败但仍需安全关闭 | `CL-011`、`CL-077` | 已引用 |
| native-session-recovery | 恢复与历史读取分离 | 无重放恢复 | `CL-047`、`CL-106` | 已引用 |
| native-session-recovery | 恢复与历史读取分离 | 已准入连接选择无重放恢复 | `CL-052` | 已引用 |
| native-session-recovery | 恢复与历史读取分离 | 恢复重放旧消息 | `CL-012`、`CL-051` | 已引用 |
| native-session-recovery | 恢复和重放保留有效接入参数 | 已有非空或未知 Client 配置 | `CL-102` | 已引用 |
| permission-approvals | 保留审批语义 | 不支持的请求 | `CL-013`、`CL-034` | 已引用 |
| permission-approvals | 保留审批语义 | 伪造选项或真实来源失效 | `CL-013` | 已引用 |
| permission-approvals | 审批事实独立于聊天缓存 | 展示缓存失败时已有审批 | `CL-025`、`CL-047`、`CL-106` | 已引用 |
| permission-approvals | 唯一裁决与交付分离 | 两客户端同时审批 | `CL-014` | 已引用 |
| permission-approvals | 唯一裁决与交付分离 | 决定交付失败 | `CL-015` | 已引用 |
| permission-approvals | 决定持久化失败不授予权限 | 批准保存失败或提交未知 | `CL-078`、`CL-093` | 已引用 |
| permission-approvals | 决定持久化失败不授予权限 | 已有允许决定随后协议要求取消 | `CL-078`、`CL-093` | 已引用 |
| permission-approvals | 决定持久化失败不授予权限 | 已发决定但交付记录失败 | `CL-078` | 已引用 |
| permission-approvals | 审批关联真实原生操作上下文 | 加载审批无输入执行 | `CL-065`、`CL-085` | 已引用 |
| permission-approvals | 审批关联真实原生操作上下文 | 上下文失效后交付 | `CL-068`、`CL-093` | 已引用 |
| permission-approvals | 审批关联真实原生操作上下文 | 无法证明审批归属 | `CL-068` | 已引用 |
| permission-approvals | 请求有效性受原执行约束 | 恢复后批准旧请求 | `CL-016` | 已引用 |
| permission-approvals | 请求有效性受原执行约束 | 前台结束后后台请求审批 | `CL-067`、`CL-085` | 已引用 |
| permission-approvals | 请求有效性受原执行约束 | 前台结束时后台审批仍待处理 | `CL-067` | 已引用 |
| permission-approvals | 请求有效性受原执行约束 | 协议不区分请求的前后台归属 | `CL-065`、`CL-067`、`CL-068` | 已引用 |
| permission-approvals | 受理决定由节点接管 | 审批者撤销后交付 | `CL-093` | 已引用 |
| permission-approvals | 受理决定由节点接管 | 节点接管后原请求失效 | `CL-093` | 已引用 |
| session-execution | 输入受理与防重复 | 断线后重复提交 | `CL-017`、`CL-104` | 已引用 |
| session-execution | 输入受理与防重复 | 请求身份冲突 | `CL-018`、`CL-088` | 已引用 |
| session-execution | 输入受理与防重复 | 已清理身份再次提交 | `CL-080`、`CL-104` | 已引用 |
| session-execution | 受理授权由节点接管 | 提交者断线或被撤销 | `CL-092` | 已引用 |
| session-execution | 每会话共享三个待发送输入 | 最后一个队列名额并发提交 | `CL-087`、`CL-113` | 已引用 |
| session-execution | 每会话共享三个待发送输入 | 原生前台忙碌仍可排队 | `CL-066`、`CL-087` | 已引用 |
| session-execution | 多客户端共享查看和操作 | B 操作 A 的输入 | `CL-088` | 已引用 |
| session-execution | 队列无期限且不跨节点重启保存 | 长时间等待 | `CL-079`、`CL-097` | 已引用 |
| session-execution | 队列无期限且不跨节点重启保存 | 节点重启 | `CL-098` | 已引用 |
| session-execution | 排队输入可编辑 | 编辑与发送交错 | `CL-088`、`CL-099` | 已引用 |
| session-execution | 队列取消与发送串行 | 取消先于发送 | `CL-099` | 已引用 |
| session-execution | 队列取消与发送串行 | 发送后取消 | `CL-099` | 已引用 |
| session-execution | 同会话前台执行互斥 | 成功后顺序派发 | `CL-019`、`CL-089`、`CL-110`、`CL-113` | 已引用 |
| session-execution | 原生工作活动独立于平台输入记录 | Agent 自行开始前台工作 | `CL-066`、`CL-086` | 已引用 |
| session-execution | 中断与立即发送 | 优先发送队列项 | `CL-090` | 已引用 |
| session-execution | 中断与立即发送 | 满队列立即发送新草稿 | `CL-090`、`CL-113` | 已引用 |
| session-execution | 执行失败暂停队列 | 失败后等待用户 | `CL-091`、`CL-110`、`CL-115` | 已引用 |
| session-execution | 执行失败暂停队列 | 失败后新输入插队 | `CL-110`、`CL-111`、`CL-113`、`CL-114` | 已引用 |
| session-execution | 执行失败暂停队列 | 显式继续后旧项重新排队 | `CL-112`、`CL-114`、`CL-115` | 已引用 |
| session-execution | 非成功结束保留原因并暂停队列 | 达到上限或拒绝后等待继续 | `CL-117`、`CL-119`、`CL-120`、`CL-121`、`CL-123` | 已引用 |
| session-execution | 非成功结束保留原因并暂停队列 | 普通取消保留停止原因 | `CL-118`、`CL-119`、`CL-120`、`CL-121`、`CL-123` | 已引用 |
| session-execution | 非成功结束保留原因并暂停队列 | 立即发送的中断按已选项继续 | `CL-122`、`CL-123` | 已引用 |
| session-execution | 非成功结束保留原因并暂停队列 | 已选立即发送项失效时无回退 | — | 待补充 |
| session-execution | 非成功结束保留原因并暂停队列 | 已选立即发送项结束不自动恢复余项 | — | 待补充 |
| session-execution | 取消与异常结果 | 派发后异常退出 | `CL-021`、`CL-046` | 已引用 |
| session-execution | 插入确认与前台终态分别关联 | 终态早到且交错自发工作 | `CL-073`、`CL-100` | 已引用 |
| session-execution | 空闲取消与活动前台取消分开 | 取消与自然完成交错 | `CL-072`、`CL-074` | 已引用 |
| session-execution | 管理存储故障时限制危险派发并保持安全控制 | 附加审计失败 | `CL-075`、`CL-094` | 已引用 |
| session-execution | 异常后重新准入 | 断线后旧执行仍可能运行 | `CL-043`、`CL-045`、`CL-046` | 已引用 |
| session-execution | 节点时间与控制并发 | 校时及等待审批期间接收控制 | `CL-096`、`CL-101` | 已引用 |
| session-execution | 命令记录不成为聊天历史 | 查询排队与执行结果 | `CL-062`、`CL-095`、`CL-098` | 已引用 |
| session-execution | Windows 与 Web 操作闭环 | 一轮完成 | `CL-022`、`CL-030`、`CL-105` | 已引用 |
| structured-history | 原生会话与历史为权威 | 清除缓存后查看 | `CL-027`、`CL-062` | 已引用 |
| structured-history | 原生会话与历史为权威 | 原生历史压缩或删除 | `CL-062` | 已引用 |
| structured-history | 基础原生会话发现与读取 | 外部新建会话 | `CL-061`、`CL-085` | 已引用 |
| structured-history | 基础原生会话发现与读取 | 无会话列表能力 | `CL-061` | 已引用 |
| structured-history | 外部更新与重新校验 | 切回平台查看 | `CL-063`、`CL-085` | 已引用 |
| structured-history | 外部更新与重新校验 | 原生读取失败 | `CL-063` | 已引用 |
| structured-history | 结构身份与来源顺序 | 工具晚完成 | `CL-023` | 已引用 |
| structured-history | 结构身份与来源顺序 | 同一工具多次更新 | `CL-023` | 已引用 |
| structured-history | 结构身份与来源顺序 | 未知结构载荷 | `CL-024`、`CL-034` | 已引用 |
| structured-history | 结构身份与来源顺序 | 来源无法关联 | `CL-012`、`CL-051` | 已引用 |
| structured-history | 实时缓存不承担保存 | 首开无后续输出 | `CL-042` | 已引用 |
| structured-history | 实时缓存不承担保存 | 慢客户端与展示故障 | `CL-026`、`CL-095` | 已引用 |
| structured-history | 通用标准重放与可丢弃视图 | 只有标准全量重放 | `CL-049`、`CL-054` | 已引用 |
| structured-history | 通用标准重放与可丢弃视图 | 无重放能力仍可接入 | `CL-047`、`CL-106` | 已引用 |
| structured-history | 重放代际与完成事实 | 重放中断或实际读取失败 | `CL-050`、`CL-101` | 已引用 |
| structured-history | 重放代际与完成事实 | 重放缺少跨加载稳定标识 | `CL-051` | 已引用 |
| structured-history | 当前连接视图由节点共享管理 | 浏览器刷新或手机接入 | `CL-064` | 已引用 |
| structured-history | 当前连接视图由节点共享管理 | 正常回收后重新查看 | `CL-064`、`CL-106` | 已引用 |
| structured-history | 当前连接视图由节点共享管理 | 读取撤销 | `CL-009`、`CL-064` | 已引用 |
| structured-history | 原生内容不获得管理页面执行权限 | 实时及重放包含主动内容 | `CL-070` | 已引用 |
| structured-history | 原生内容不获得管理页面执行权限 | 审批描述包含误导字符 | `CL-070` | 已引用 |
| structured-history | 资源引用访问独立校验 | 外链媒体指向本机或私网 | `CL-071` | 已引用 |
| structured-history | 资源引用访问独立校验 | 附件路径越界或含主动内容 | `CL-071` | 已引用 |
| structured-history | 资源引用访问独立校验 | 显示链接与实际目标不同 | `CL-071` | 已引用 |
| workspace-registration | 目录登记与解析 | 目录失效 | `CL-028` | 已引用 |
| workspace-registration | 目录共享不建立目录锁 | 两个会话共享目录 | `CL-029` | 已引用 |

## Stable ID Distribution

- acp-platform-constraints（10）：`CL-002`、`CL-026`、`CL-049`、`CL-092`、`CL-095`、`CL-103`、`CL-107`、`CL-108`、`CL-109`、`CL-116`
- harness-management（23）：`CL-005`、`CL-006`、`CL-007`、`CL-032`、`CL-033`、`CL-034`、`CL-035`、`CL-036`、`CL-037`、`CL-038`、`CL-039`、`CL-040`、`CL-041`、`CL-047`、`CL-048`、`CL-053`、`CL-054`、`CL-055`、`CL-058`、`CL-059`、`CL-060`、`CL-069`、`CL-106`
- management-audit（9）：`CL-008`、`CL-009`、`CL-031`、`CL-080`、`CL-081`、`CL-082`、`CL-083`、`CL-084`、`CL-094`
- management-input-upload（10）：`ACPI-R01`、`ACPI-R02`、`ACPI-R03`、`ACPI-R04`、`ACPI-R05`、`ACPI-R06`、`ACPI-R07`、`ACPI-R08`、`ACPI-R09`、`ACPI-R10`
- native-session-recovery（8）：`CL-010`、`CL-011`、`CL-045`、`CL-052`、`CL-056`、`CL-057`、`CL-077`、`CL-102`
- permission-approvals（10）：`CL-013`、`CL-014`、`CL-015`、`CL-016`、`CL-025`、`CL-065`、`CL-067`、`CL-068`、`CL-078`、`CL-093`
- session-execution（47）：`CL-001`、`CL-003`、`CL-004`、`CL-017`、`CL-018`、`CL-019`、`CL-020`、`CL-021`、`CL-022`、`CL-030`、`CL-043`、`CL-044`、`CL-046`、`CL-066`、`CL-072`、`CL-073`、`CL-074`、`CL-075`、`CL-076`、`CL-079`、`CL-086`、`CL-087`、`CL-088`、`CL-089`、`CL-090`、`CL-091`、`CL-096`、`CL-097`、`CL-098`、`CL-099`、`CL-100`、`CL-101`、`CL-104`、`CL-105`、`CL-110`、`CL-111`、`CL-112`、`CL-113`、`CL-114`、`CL-115`、`CL-117`、`CL-118`、`CL-119`、`CL-120`、`CL-121`、`CL-122`、`CL-123`
- structured-history（14）：`CL-012`、`CL-023`、`CL-024`、`CL-027`、`CL-042`、`CL-050`、`CL-051`、`CL-061`、`CL-062`、`CL-063`、`CL-064`、`CL-070`、`CL-071`、`CL-085`
- workspace-registration（2）：`CL-028`、`CL-029`
