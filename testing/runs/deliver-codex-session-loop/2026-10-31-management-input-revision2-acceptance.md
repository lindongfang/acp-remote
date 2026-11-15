# 管理候选 revision2 独立复验

结论：原29项独立管理验收全部通过，MGT-CODE-001/002的原动态失败已修复；另加的MGT-CODE-003两个原因分类分支实际失败，revision2整候选仍不通过，不能据此合入或签完整L5/L6。

## 固定版本与隔离

读取HANDOFF.revision2.md、真实修订代码及`implementation-source.revision2.json`，SHA256=`003850e2f9c7594dd9b4805c4479e62a774b24deed873f1e3c2d010d7d8ba5d8`；自行核验78份输入实际源执行前后全部匹配，见`management-revision2-source-initial.json`、`management-revision2-source-final.json`。原版11文件/manifest及原失败日志未修改。

新复制至本报告同目录`management-revision2-copy/`。从原独立副本逐字复制29项验收源，测试复制相同摘要见`management-test-unchanged-revision2.json`；001/002的500ms剩余/700ms观察及零writer预期均不改变。仍仅在copy的product_commands.rs添加cfg(test)include，不改生产逻辑。原29通过后，新增独立child测试文件`management_deadline_semantics.rs`及仅test的包含声明来验证另一个疑点；原29测试函数和断言未改。相关测试资产摘要见`management-revision2-test-assets.json`。

使用原独占`target-management`、既有Cargo.lock、offline；真实隔离SQLite与合成可信actor/owner/时钟/writer，不接WSS/Web/真实Harness、不读取秘密、不外发Web审计。原独立L3/L4定义与阶段边界维持原归属；未以开发33例代签。

## 实际执行

| 检查 | 实际结果 | 原始证据 |
| --- | --- | --- |
| lib independent_acceptance原29项 | 29 pass、0 fail、0 ignored，exit0，15.70秒 | management-independent-revision2.log |
| 原公共模型integration | 21 pass、0 fail，exit0 | management-revision2-model-regression.log |
| 外部proof/grant/ticket/permit负向编译 | 11项预期E0277/E0423，cargo exit101为预期拒绝 | management-revision2-proof-negative.log |
| 生产lib clippy -D warnings | exit0，仅lib范围，不冒充all-target | management-revision2-lib-clippy.log |
| 追加原因分类2项 | 0 pass、2 fail，exit101，10.42秒 | management-revision2-deadline-semantics-expanded.log |

前三项及clippy退出汇总为`management-revision2-final-exit.json`。命令均明确使用本目录`management-revision2-copy/app/Cargo.toml`或外部`management-negative-revision2/Cargo.toml`。负向crate首次以产品锁为种子offline解析本地根包；没有新增registry依赖。正常库/测试执行使用--locked。PowerShell正常stderr标签不代替真实退出值。

## 问题关闭与新增问题

| ID | 独立复验结论 |
| --- | --- |
| MGT-CODE-001 P2 | 原用例通过：公开受理后保留ticket跨签名exp，实际supervisor运行后正文及预算retained=(0,0)。真实对象deadline按fresh采样前单调锚点加剩余期限收紧，并取原upload截止最小值；本次先读实现，再以原动态断言验证。执行失败关闭；静态修复结论由reviewer另签。 |
| MGT-CODE-002 P1 | 原用例通过：final_fence COMMIT后的refresh_owner暂停窗口中共享store故障，恢复后writer=0、dispatch拒绝；旧健康信号仍faulted。其余29项回归维持，包括可能传输后的unknown不改未发送。执行失败关闭；静态修复结论由reviewer另签。 |
| MGT-CODE-003 P2 | **仍open**。合并后的write_deadline不是签名截止；必须区分真实请求exp与操作/阶段/输入对象寿命。两分支如下，均已独立确认token仍可由原ES256 verifier验证有效，且writer=0、正文=(0,0)，问题在结果/错误分类，不是新增发送。 |

003操作预算分支：公开issue/submit/claim后，dispatch在最后refresh_owner暂停。恢复前仅arm可信TimeSource，让下一次同步最终采样耗时10.3秒并返回NOW+10300；原24小时签名仍有近24小时有效期、body仍有效。操作10秒预算耗尽，实际command.status="expired"，预期"failed"；不能把零发送的操作超时伪报原请求已过期。独立用例`m_operation_timeout_does_not_expire_still_valid_signed_request`。

003输入对象寿命分支：使用公开MemoryInputs.reserve的500ms本地对象与有效24小时签名，经真实submit/claim。在最后refresh_owner暂停700ms后恢复，实际command.status="failed"正确，但返回Error::Expired（其领域错误描述为product request has expired）；预期不把bodyTTL超时报告为请求期限到达。独立用例`m_body_ttl_does_not_expire_still_valid_signed_request`。原始token在该结果后再次验签仍通过。两例使用公开应用API，未篡改产品私有deadline或伪造ticket/permit。

revision3须保留原29预期及上述两个新预期共同复验，不能把003混成001/002未修，也不能把前29通过签作全部候选已无问题。

## 仍未执行

原management-acceptance.md各M组细分not-run、真实认证/trust/资源来源、实际ACP writer、SQL审批、WSS/Web、故障安全控制、正式恢复新epoch/最终reconciliation/ready、清理、真实Windows节点/进程树与完整L5/L6仍未交付。本报告不改变默认100000 audit自然路径不可达与缩限SQL/算术边界的区分，也不把原21模型回归作为真实SQL/资源证据。

测试pool/task/supervisor已结束或显式close/shutdown；合成库在独占copy中保留用于审查，未触及用户资源。等待revision3冻结；未修改主仓库生产。
