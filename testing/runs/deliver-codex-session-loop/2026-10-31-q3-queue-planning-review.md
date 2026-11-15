# Q3 纯队列独立编码前检视

2026-10-31。审查目标：shared-input-queue-contract.md及独立tester候选，首轮精确SHA见planning-v1-inputs.json。仅读文件和公开代码，未参与产品修改。

## 首轮结论：门槛待补，尚未放行编码

核心设计可接受：泛型结构正文不克隆/降文本，短&mut顺序协调；三未发含挂起/准备、独立活动槽；prepare保位可编辑取消，begin_send最终Ready+资源代际+项版本；旧token跨实例/活动失效；失败仅显式resume恢复，prioritize不绕过；已有新等待D在恢复X/Y之前，继续不抢占活动D。该排序属于已确认新输入优先原则下的工程精化，未发现需要用户再确认的产品分支。

| ID | 级别/位置 | 门槛与依据 | 状态 |
| --- | --- | --- | --- |
| Q3-PLAN-001 | P2，shared-input-queue-contract：公开API | 补QueueInput、等待/active借用观测字段或getter、错误分类/优先、Rejected/Submission与各方法精确Rust签名；明确同内容edit版本规则、无挂起resume是否no-op。当前语义足够讨论但独立公共测试仍不能编译，角色要求验收断言可执行。 | open，阻断本阶段编码 |
| Q3-PLAN-002 | P2，发送交接与完成 | direct/begin_send后已占活动，但真实wire前若确定零发送/事实失效，须明确不自动回队、不造第四等待项、settle不自动重新受理；原输入返还供诊断/草稿是否属于新操作由上层负责。不能用纯token承诺实际零字节或掩盖未发正文为活动。需在合同与独立定义映射同一边界。 | open，阻断本阶段编码 |
| Q3-PLAN-003 | P2，tester候选Cases与待冻结项 | tester读取新合同后补固定API映射及oracle，关闭自己的工程缺口；明确Blocked仅单一抽象事实，active/unknown/待插入/资源无效的真实区分、插入/终态关联、同目标实例唯一复用不在此L3动态证明。保留所有后续L4–L6/原tasks义务。 | open，阻断本阶段编码 |

独立tester候选正常、异常、取消、边界覆盖充分，保留CL原ID/层级并清楚限制；CL087.Q03唯一实例、CL089三种协议阻挡、CL090真实中断、CL099零字节不能由单队列抽象事实自动签已证明。公共API只能动态证明&mut线性化顺序，不能把两种串行排列标为真实并发争抢。私有极值分列开发测试/静态核对；不增加生产任意setter。

本轮未运行产品测试或声明交付通过。上述问题必须由本reviewer读取修订字节后关闭，不由coder代签。后续独立源码/验收同SHA快照另行结论。

## 第二轮合同复查

已亲读合同新增“最小可执行Rust接口”及plan的Q3恢复实施段。

- Q3-PLAN-001：closed。当前合同给出QueueInput/Rejected/QueueEntry观测、所有公开签名和错误先后；同内容edit仍推进版本、不比较泛型；revision耗尽不失效准备；普通resume完整no-op；失败模式退出才失效准备。可编写独立公共API断言，无需访问私有字段。
- Q3-PLAN-002：closed。明确direct/begin后条件失效仍保留唯一active、不造第四等待项；确定零发送由owner显式settle并另记NotSent，返还输入不自动重受理/回队；可能发送先保持归属可靠协调。纯模型不承诺真实wire零字节，不把Advance记原生成功。
- Q3-PLAN-003：仍open。等待独立tester基于当前准确合同追加映射与精确oracle后复查；产品源码尚未编码，当前仍未放行编码。

Ready和settle依赖可信上层事实属于明示边界。以后接线时必须实测同协调边界的最后围栏与owner取消，不可因本阶段通过当作已验证传输安全。

## 最终编码前限定结论：通过纯领域阶段方案与验收定义

已亲读独立tester `q003-contract-api-definition.md` SHA-256 `9e1c407e6d000b827b03bcd02482fe3d217cde18a2c75d7f83e2cbe0d966d93c`，逐行对照初始候选及固定接口：D/X/Y、三挂起单direct、编辑/取消/prepare/begin先后、资源代际、旧token、重复ID、错误优先、resume完整no-op、非Clone/Debug结构正文、32MiB+1及诊断不泄正文均有可执行oracle。Blocked只算一个抽象分支、&mut只算节点顺序、不代签注册表/并发/真实wire、MAX只私有开发与静态核对，范围表述准确。

Q3-PLAN-003：closed。固定API补充是正式阶段定义，与旧候选共同阅读；原候选中尚待冻结的措辞由补充精化，未削弱原CL L5/L6。

主合同最终SHA-256 `61987328a83927c414fba8a2b085f0796f05792a8072ae9ad11d6187d8e1c466`，已亲读确认新增初始editor=submitter，与tester固定oracle一致。此句是原始归属的机械初值，不改变产品授权规则。tester补充所列旧合同862ffc…保留其原读取版本，本报告确认这次唯一初值澄清可沿用该定义，无需改写旧文件。

三项阻断均由本reviewer复查关闭，无新增编码前阻断。允许只实现新domain/input_queue模块、必要声明、开发单测和阶段事实文档，随后在固定源码SHA执行独立公开API验收与同版交付检视。当前仍**未运行产品测试、未签交付通过**。

不放行或代签节点登记/SQL必要状态及迁移、原生事实/终态关联、真实writer owner及零发送、DTO/WSS/Web、审批owner、真实Harness、Windows完整树；原tasks及L4–L6全部保持。旧未提交实现、历史源及证据不得覆盖，不新增依赖、不外发Web audit、不提交发布。交付仍需实际新源码差异、开发极值断言、同SHA正式验收和适用工程检查。
