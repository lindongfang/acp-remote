> Q-001（2026-10-31）：本文保留为既有实现子合同，非本轮新接线依据。与当前specs/design冲突的正文、输出、实体/快照容量预算、重放门槛、签名期限及派发再验原actor均已停用；旧测试仅说明原范围。非冲突协议事实可复用，但须按新阶段独立复核。

# P0：v1可空messageId入口窄修合同候选

2026-10-31；仅供独立tester/reviewer预审，未编码。与P1/P2、VPROJ-PLAN-002/003分别实施，不等待anchor/scanner方案，也不据本包关闭它们。

核心规则（6条）：

1. 只在Strategy::update现有消息分支解释update.messageId：**协商Version::V1且JSON null→None**；v1省略仍None，字符串仍经MessageId::parse→Some。
2. v1数字、布尔、数组、对象及超过4096 UTF-8字节的字符串仍返回AcpError::InvalidMessage；空字符串按现有MessageId::parse原行为保留，不新增ID规则。
3. 协商v2的现有解析完全不变：null/其他错误形状仍InvalidMessage；字符串仍Some；省略在当前粗SessionEvent投影仍None。本包不改变v2必需ID的后续结构校验责任，不放宽或顺便收紧v2。
4. 处理真实method=session/update、params.sessionId、params.update.sessionUpdate及params.update.messageId；保留原raw字节、未知字段、origin与原接收顺序。不聚合chunk、不造消息/Run/Turn/审批或可靠轮次。
5. 覆盖三个v1合法chunk tag：user_message_chunk、agent_message_chunk、agent_thought_chunk；不改tag分派、其他字段解析、协商/重放/取消/连接错误策略。
6. 写范围仅app/src/adapters/acp/strategy.rs该分支及开发测试（tests.rs或独占tests/message_id.rs和必要mod声明）。model/transport/Domain/DDL/依赖/广告/WSS不改；异常仍由Connection::next原有路径传播，不新增吞错/重试。

## 独立验收范围

使用现有duplex协议peer，实际initialize并协商v1/v2，再经Connection::next输入完整原帧；不能只调用Strategy::update代替真实入口。

- v1三个chunk各测省略/null/正常字符串，投影Message.message分别None/None/Some，kind/session及Incoming.raw原字段保留；null后继续合法通知可正常接收。
- v1错误类型及4097字节字符串各在独立连接返回AcpError::InvalidMessage；不得因为“None”分支接纳错误形状。
- v2正常字符串仍成功，null和错误类型仍InvalidMessage；省略粗投影保持现有行为，明确本包不宣称它符合完整v2结构适配。
- v2 offer合法selected v1使用v1规则；不能按offer或程序名判断null语义。
- 原ACP开发及发现/权限独立合同回归，fmt/严格clippy；未运行不得称通过。此包不签结构聚合/视图预算/可信分页/真实Harness。

## 固定来源

SDK2.1.0 v1 ContentChunk.messageId?: MessageId | null（generated types约2392–2411行）。本轮只用本地固定官方文件，无网络/浏览器/Web audit。

| 文件（仓库相对） | SHA256 |
| --- | --- |
| testing/probes/codex-session-loop/node_modules/@agentclientprotocol/sdk/schema/v1/schema.json | 1c57095acf9c0844054dd1fdc3624c121b3fdc24209ecc987fd6748c705fdef5 |
| 同SDK dist/v1/schema/types.gen.d.ts | 53da804f1c8e61755e696fd8fbd7be2e22419e58f8714245882e6f07eb7cc780 |
| app/src/adapters/acp/strategy.rs | cb91ed1d4395ac70af35fb422a8c97e1b2edf93f76feb39b10f638f3cf43debf |
| app/src/adapters/acp/transport.rs | 93bd37e419387eefbb5f5a8819bbbf9c678f44a0baccec89b9edfc61d34f535b |
| app/src/adapters/acp/model.rs | a14f33cf708aa1a93b8291fbf0e5d55f18627190d54f086505f4f1c883bb2b60 |

本候选只修已知入口拒绝合法v1 null，不改变任何已确认产品规则。等待独立定义/预审后才可在授权路径实现；root与旧候选/冻结copy均未修改。
