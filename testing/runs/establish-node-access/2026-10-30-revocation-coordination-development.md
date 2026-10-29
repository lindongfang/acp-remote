# 撤销协调与首次订阅竞态开发修复

日期：2026-10-30。来源为独立reviewer的NA-RV-001/002，原发现见同日revocation-gap-review。修复按既有合同执行，不改变授权规则。

R9固定清单83项：`2026-10-30-implementation-r9.sha256`，SHA256 `7876C7803106193C476EE0C23DAC27AA25036154CDF293D6A7EB7ECD2FEA6DB7`。普通节点binary SHA256 `AB97418E98D67A71E25BBF62B1E780F2F767530CA4290FB9C78E220A222A6024`。

- 命令受理在同一客户端gate内重新检查权威资格。受理后释放actor gate，再执行目标撤销，避免自撤销及互相撤销时递归/交叉持锁。
- 认证结果和命令结果入队前再次在actor gate内核对资格；包括幂等缓存命中。首次订阅及后续revision通知的资格检查和入队同样处于该边界。
- 首次订阅不再在SQL快照之后无条件borrow_and_update；并发提交的新revision仍保持未读，后续循环会送达。通知仍只包含v/type/revision。

开发实际执行真实TLS/WSS配合测试专用调度栅栏：

1. 撤销先于命令受理，受理计数不增且4410关闭。
2. 命令已完成、结果尚未入队时撤销，新增入队计数为0且4410关闭。
3. 重放已有revoke结果，缓存命中后、入队前撤销actor，新增入队计数为0。
4. 首次订阅读revision=1后暂停，另一信任提交revision=2再释放：无需任何额外变更即收到1和2，主动分页也得到revision=2及两条记录。

定向测试通过；最终真实owner `npm run check` 退出0，Web46、Rust库131及固定向量4全部通过，fmt/Clippy/类型/构建通过。随后普通 `cargo build --locked --offline` 退出0。日志：`.tmp/implementation-r9-check.log`、`.tmp/implementation-r9-build.log`。此版本普通构建无故障开关。

前一R8完整owner检查亦通过（129库+4向量、Web46），包括五种模拟pipe校验失败后的实际线程恢复。受限shell中更早R7全量因DPAPI子进程和pipe环境异常中断，原日志保留；不作为通过证据，也不将失败抹为产品正常测试。

NA-RV-001/002仍待原reviewer独立复查关闭；开发通过不代签NA008、全部管理用例或整体待归档状态。用户30447体验节点未替换，未提交、推送或归档。
