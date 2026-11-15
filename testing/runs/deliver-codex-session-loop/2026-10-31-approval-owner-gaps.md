# 审批 owner 编码前缺口清单

2026-10-31；初读0882a1bc...，已追加核对§3A候选4b9c4f81...。本轮只读准备，所有owner/schema4产品验证not-run，整体不能编码。暂无需用户新增产品决定；以下均为既定产品边界下待冻结的工程路径。AP-PLAN001/002已关闭的定义不重开，本清单要求其落实到可测试合同。

| 缺口 | 现状与实际影响 | 冻结时需提供的可观察依据 |
| --- | --- | --- |
| AP-TG01 schema4布局/迁移候选已提供，等待独立闭环 | 当前工程仍schema3、没有attempt表。§3A现明确STRICT DDL、复合FK、planned/actual显式NULL分支和1/2/3→4顺序/故障点；已纳入定义K，不能继续称没有DDL | reviewer核对最终DDL与K的真实非空副本/4重开/FK/STRICT/CHECK/NULL/真实COMMIT/WAL覆盖并冻结测试接口。旧3事实原样保留，新增attempt为空不凭旧delivery推断actual；纯布局可单独评估子阶段，不等于owner整体开工 |
| AP-TG02 固定v1/v2取消矩阵 | SDK2.1.0本地v1描述session/cancel prompt turn须cancelled；v2明确同session全部pending request_permission cancelled、其他session不受影响。不能将v2“全部”直接补成v1未证实范围，也不能依据tool/Run猜后台类别 | 各版本触发（session/cancel、$/cancel_request、close、disconnect、专属工作失效）、请求范围、协议响应/零响应和先后顺序，列本地固定规范字段路径/文本来源及owner实际可观察事件。协议允许忽略的通知不能冒充强制生命周期证据 |
| AP-TG03 不可制造Observation与单次ReplyCell API | 当前PermissionRequest可见字段可改、raw/fields可Clone；私有connection tag和server_pending摘要只在Connection.permission_response重验。它移除pending后直接write，尚不是节点owner的持久ticket/ReplyCell/回执链路 | 原始Incoming→内部Observation发行的唯一入口、当前pending关联/重复冲突行为；target/实例/ResourceEpoch由谁可信绑定；节点与Connection共用的单次消费所有者；非Clone危险ticket/permit；禁止裸字段转换，不以测试constructor绕过来源 |
| AP-TG04 planned/actual/传输三者一致性 | 定义已分离，但尚无实际writer消费/回执持有接口。claim后抑制selected并转cancelled、实际种类尚不明、可能已交字节不能改种类等需要可观察屏障 | 首次可能传输边界、Consumed实际种类的原子发布、回执内容与所有者/寿命、写成功/失败/Drop/队列晚消费语义；selected与safe cancel竞争必须同ReplyCell至多一次，不以两套writer路径各计一次 |
| AP-TG05 完整预算账本 | 1MiB/8MiB/32MiB及64引用仅有上限，尚无raw+解析Map/字符串+索引/options/display/writer借用+旧lease固定费用与转移计量。现有RawMessage同时拥有raw Arc与parsed Map，32MiB帧上限不是保留预算 | 明确保守费用/计数公式、首次复制前预留和传输交接时机、连接/目标/节点共同账本、无计费Clone防护、leases最后Drop退账/失败原子性、节点监督任务与绝对writer截止；合法64/65和大小边界可达性 |
| AP-TG06 SQL/COMMIT故障与结果/恢复范围 | 候选要求观察/决定/claim/result失败可safe结束，且原actor撤销不妨碍已发生结果；但详细事务/CAS/receipt补记顺序未定。schema3 delivery unknown要求claim，schema4 attempt也要求已有对应delivery claim；无approval/claim的safe响应不能测试时自造危险claim | 每个事务读写/预期rowsAffected、成功证明和COMMIT未知隔离规则、真实deferred-FK COMMIT失败与“成功回执丢失”独立分支；无数据库safe路径、无先前attempt时safe回执是否仅留内存/何时补记及claim类型含义、失败关闭危险状态及有限回执可观察性。后续完整reconciliation仍排除，不能临时只读一行恢复ready |
| AP-TG07 类型化固定摘要与时钟边界 | action/数组已经给出，尚需对应正式typed DTO/冻结向量；raw摘要必须实际Connection bytes，不得经过任意Value重序列化替代 | 正式DTO/紧凑数组编码、所有ID/epoch/digest规范格式及向量；typed RPC区分数字/字符串；受理/claim/最后writer时钟采样与floor、native有效期事件、返回前过期、已发生report不受exp/revoke阻挡的具体顺序 |

上述缺口不要求新增产品行为；需要工程合同把既定规则变成可执行、可观测边界。方案可分层先冻结纯模型或schema布局，必须明确子范围和前置；不以整体候选通过为由同时接数据库/网络危险响应。

独立计划资产：approval-owner-definition.md逐一映射permission-approvals全部14个Scenario及A–K断言；本文件仅列编码阻断，不把尚未实施记为产品缺陷。reviewer可据此要求最终合同逐项闭环，测试资产实现只在获授权独占路径进行。
