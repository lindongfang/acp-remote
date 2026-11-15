# P0：v1可空messageId独立子用例

2026-10-31。依据外部p0-v1-message-id-contract.md SHA256 `833c1124b7257cfaf796f045fa3b7e72bceb7d343ae1204e6836beccec7e1981`；Strategy基线`cb91ed1d4395ac70af35fb422a8c97e1b2edf93f76feb39b10f638f3cf43debf`。本轮仅规划，以下7组全部not-run，产品源码未改。采用CL-034及CL-032/036/058–059的纯协议L3/L4子范围，不新增稳定CL编号/标题，不签完整CL或真实Harness。

使用现有公开Connection双工peer：实际initialize/协商后，peer写完整JSON-RPC session/update帧，调用Connection::next读取。不能只调用Strategy::update。固定三个合法v1 tag为user_message_chunk、agent_message_chunk、agent_thought_chunk；帧带真实params.sessionId、params.update.sessionUpdate及合法content。每个错误输入使用独立已初始化连接，避免上一错误关闭状态污染下一断言；10秒以内固定deadline/受监督peer，结束后Drop，不涉及Node/SQL/WSS。

| 组 | 操作步骤 | 预期结果 |
| --- | --- | --- |
| P0-A v1三tag正常矩阵 | 明确offer/selected v1，对每tag输入messageId省略、null、普通字符串及空字符串 | Incoming::Update内SessionEvent::Message的session/kind正确；省略/null为None，普通及空字符串为Some且字符串逐字相同。空串沿当前MessageId::parse允许行为，不加新规则，不聚合chunk |
| P0-B UTF8边界 | 每tag分别输入ASCII4096、UTF8共4096字节（如2048个é）、4097字节字符串 | 两类4096成功Some且字节不变；4097 InvalidMessage。限制按UTF8字节而非字符数，不截断、不转None |
| P0-C 错类型 | 每tag分别数字（整数/小数）、布尔、数组、对象作为messageId；每次新连接经next进入 | 均AcpError::InvalidMessage，不进入null/省略分支，不吞错或自动重试/reinitialize。保持Connection原错误处理；本组不新增错误后恢复规则 |
| P0-D 以selected版解释 | 使用initialize_preferred观察唯一offer v2帧，peer合法返回selected v1，随后对三个tag输入null/字符串；与明确v1协商对照 | null均None、字符串Some，按实际selected v1而非offer v2或程序名判定；一次initialize，不降版重发。此组不改协商规则 |
| P0-E raw/order/origin/继续读取 | 同连接连续发送三tag含null及后续字符串，混合未知字段/_meta/不同content并保留输入空白；分别在Live、真实同session replay请求pending、普通resume pending窗口输入，再完成标准响应后继续通知，另session更新对照 | raw.bytes逐字等于去掉帧分隔换行的原JSON行，raw.fields保留null/省略及未知字段；next接收顺序不变，null不关闭连接，后续合法通知正常读取；origin按现有Live/DuringReplay/DuringResume窗口和目标归属不变。DuringReplay不被当每条都是历史证据，不新增Run/轮次/聚合 |
| P0-F v2保持原行为 | 实际selected v2，对三个chunk输入省略、字符串/空字符串、null及P0-C错误类型，并检查4096/4097边界 | 省略在当前粗投影仍None，字符串/空串Some，null/错类型/超长仍InvalidMessage。明确这只是保留现有粗SessionEvent行为，不签完整v2必需ID校验；v2收紧留P2，不混入P0 |
| P0-G 同版回归与改动范围 | 同版执行原§6A独立17项、权限主题独立12项及适用ACP开发测试；核对实际数量，fmt/严格clippy；读取实际diff | 不删/ignore/弱化原预期，发现/权限/原raw和连接错误边界保持；仅Strategy消息分支的v1 null判定及开发测试/必要声明变化，model/transport/domain/DDL/依赖/广告不变，不签P1/P2、视图预算、可信页、真实node/WSS或Harness |

复用资产：app/tests/acceptance/acp_discovery_replay.rs（17项，SHA48ac5cb9...）与acp_permission_subject.rs（12项，SHAcccda47e...）。这些是后续同版回归义务，本定义未实际重跑。官方依据只读本地固定SDK2.1.0 v1 schema和generated types（ContentChunk.messageId?: MessageId|null）；无需浏览器、空间重建或Web审计，不外发元数据。

正式验收前固定修改后的Strategy/开发测试与未改文件SHA，公开API独立资产和Cargo target仍独占；结果按7组及实际循环分支记录，包含真实命令/退出码/原帧/投影及未覆盖。全部只用合成数据；raw证据不引入用户正文。当前无未决产品决策或需要等待P1/P2的前置；独立reviewer审定本窄定义后方可编码，不以计划签通过。
