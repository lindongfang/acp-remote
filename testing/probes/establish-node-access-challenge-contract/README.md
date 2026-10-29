# 挑战合同开发探针

使用普通产品 ChallengeRegistry、JWS/ES256 和公开合成客户端 key；仅时钟和随机源是明确可控适配器。没有真实 HTTP/WSS、浏览器或管理资格授权声明。探针不读写用户数据，不创建节点/后台任务。

仓库根运行（与其他构建隔离）：

```powershell
$env:CARGO_TARGET_DIR = 'D:/Project/acp-remote/.tmp/challenge-contract-probe-target'
cargo run --manifest-path testing/probes/establish-node-access-challenge-contract/Cargo.toml --locked --offline
```

预期退出0并输出 `PROBE COMPLETE: 22 controlled challenge scenarios passed`。断言失败非零退出；编译成功不等于场景通过。

- 13项认证字段/用途变异：nodeId、origin、connectionId、clientId、持有另一个key、clientNonce、challengeId、nonce、iat、exp、版本、nodePublicKey和跨用途typ。拒绝后同一作用域原证明也不能再用。
- 4项认证14/15秒边界与墙钟回拨组合，成功/失败后重复提交均拒绝。
- 2项配对29/30秒边界与墙钟回拨，重复提交拒绝。
- claim/status/auth三条挑战签发入口的随机源失败均关闭入口；恢复fixture随机源后正常签发。

这是 coder 维护的可执行开发资产；独立tester需审查断言、实际运行并决定NA-004采纳范围，不因22项全绿自动签署整例。未覆盖的重取/跨连接隔离可按未变产品字节复用已有对应测试证据，不将本探针扩写成全协议或真实随机源质量证明。
