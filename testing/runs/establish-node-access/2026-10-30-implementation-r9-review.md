# implementation-r9 独立交付检视

日期：2026-10-30。结论：**PASS（NA-RV-001/002修复限定范围）**，两问题由原提出reviewer独立关闭。未发现本包新问题；不代签NA008/027/028/029整例，也不宣布整个change可归档。

## 固定版本与执行边界

- r9清单 `2026-10-30-implementation-r9.sha256` 独立摘要 `7876C7803106193C476EE0C23DAC27AA25036154CDF293D6A7EB7ECD2FEA6DB7`，83项逐文件匹配，无缺失/异常。
- management_ws.rs：`E529AAC61F8D719E0D0BF6435DD0DDB201A7156FA24FF984DAA322AAAE2197B5`；node.rs：`39A2ABC82F61A563EC63BF9CBC7C676FD1A4328DF1070081275FD7255BB87AF2`。执行后再次核对两文件一致。用户后续仅pairing_http测试包不使这两文件证据自动失效，版本判断按实际内容与作用范围。
- 本轮独立编译/定向执行使用 `.tmp/reviewer-implementation-r4-target` 和测试自有真实节点、SQLite、TLS/WS/owner管道；测试SecretProtector为明确fixture，不声明此次验证DPAPI或宿主证书信任。用户30447和日常浏览器未动。
- 仅新增本报告，不修改源码、旧报告、清单、规格、plan/tasks，不提交推送归档。主方全量131库+4向量/Web46通过为交接开发证据，不写成reviewer全量重跑。

## NA-RV-001 / Medium / Closed

位置：management_ws.rs:760–774、819–856、867–873、970–1017；node.rs:1926–1985。

`lock_current_actor`先取得同TrustStore的目标actor gate，在锁内调用真实资格检查，返回持有的OwnedMutexGuard。`admit_command`以该权威检查作为线性受理点，返回时释放锁；后续执行撤销才取得被撤销目标gate。没有跨actor持双锁，也不在自撤销时递归等待自身gate。先受理后撤销的动作仍可按真实结果完成，这符合现合同，不应新加回滚所有已受理动作的要求。

认证完成帧和command结果均由 `enqueue_authenticated`重新在actor gate内查资格并同步try_send；缓存结果也走相同路径。subscribe（包括非法资源错误响应）和revision通知在gate内查资格并入队。writer继续持目标gate核对并sink.send；revoke提交持同目标gate，提交后不可能穿过旧资格结果新入队。握手challenge位于认证前，不是受保护管理结果，不能以其不经actor gate认定漏修。

独立执行 `revoked_actor_cannot_be_admitted_or_enqueue_a_completed_result`：1/1通过。内部真实三场景：撤销先于受理；动作完成后撤销先于入队；真实revoke原结果缓存命中后撤销先于入队。每次强制wait栅栏→owner revoke完成→释放→close4410；admitted增量分别0/1/1，enqueued不增。测试确实命中普通产品门，非自行模拟资格判断。

另外重新执行 `revoke_commit_discards_an_already_queued_management_result` 1/1通过，真实出队后、writer gate前暂停，撤销提交后释放，只见4410。既有保护没有因新锁丢失。关闭的是原受理/入队协调差异；不是本次已经实测所有连接的队列即时释放。物理队列丢弃和多连接最终资源收敛仍由NA028完整场景取证。

## NA-RV-002 / Medium / Closed

位置：management_ws.rs:801–809、819–856；node.rs:1989–2045。

首次订阅提取为subscribe_devices，SQL读r后不再borrow_and_update；因此在读r之后publish的新值仍未读，回到事件循环后由changed分支读取/入队。last_sent_revision对应已发送r，后续新值不被吞掉。保持watch允许合并的设计，不要求发送所有中间revision。

独立执行 `initial_subscription_does_not_consume_a_newer_revision_unseen`：1/1通过。实际在订阅SQL读旧r=1后置栅栏，另一客户端通过真实配对批准提交新信任并publish r=2，释放后依次收到subscribe成功、devices.changed=1及=2；新通知只有3个字段，再主动device.list得到revision2和两条记录。没有靠额外第三次变更唤醒，精准覆盖原丢失窗口。

## 执行结果与限制

首次使用cargo test独立重编译r9后，执行上述受理测试；随后同一r9测试exe精确执行订阅、已入队writer栅栏、`management_websocket_authenticates_queries_subscribes_and_revokes_devices`。**4个定向测试全部通过，0失败**。最后一项真实覆盖node.info/list、首次订阅、A撤B、后续revision、A继续操作、B关闭与新认证拒绝，确认正常管理通路未回归。

没有独立动态执行同时自撤/互撤；本轮其无嵌套gate循环为静态锁生命周期结论，不伪称真实竞争通过。新计数只测helper路径，不能冒充所有subscribe/notification入队总计数；这些门的正确性本轮静态核对，完整多连接受控场景仍需tester。没有新增未经验证的严重性或无限组合门禁。

## 交接剩余范围

同日revocation-gap-review保持历史发现，不回写；其RV001/002以本报告最新关闭结论为准。G01已收缩：受理、普通完成、真实缓存命中→入队及撤销先于writer门已有定向通过；剩余发送先赢、多个真实B订阅连接及所有连接失效/自撤owner恢复等具体必要断言不能由此自动关闭。G03已有86399/86400边界、容量probe；r7已补真实pager SQLITE_FULL。G06/07仍须按当前Steps/Expected完成70条Web完整ID集合、游标59/60秒/跨范围、连续revision变更与主动取回等有限范围。

本包有普通运行时行为变更，最终tester E2E必须对应本次management/node内容及r9产物，不能只用r8二进制代签。用户交接普通binary摘要 `AB97418E98D67A71E25BBF62B1E780F2F767530CA4290FB9C78E220A222A6024`，与后续仅测试资产变化分开记录。原CON/LIFE/SC/I01及r7/r8限定关闭保持。
