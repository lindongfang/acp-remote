# 独立 NA022 协议探针

使用真实 TLS、DPAPI、SQLite、owner IPC；无浏览器声明。默认普通产品 NodeRuntime 模式保留原红灯复现方式。固定版本回归优先设置以下环境变量，让节点实际运行经过摘要核对、复制到tester自有目录的binary。

```powershell
$env:CARGO_TARGET_DIR='D:/Project/acp-remote/.tmp/approval-acceptance-20261030/probe-target'
$env:NA022_RUNTIME_ROOT='D:/Project/acp-remote/.tmp/independent-na022-r12-20261030'
$env:NA022_BINARY='D:/Project/acp-remote/.tmp/independent-na022-r12-20261030/bin/acp-remote.exe'
# 先独立校验新固定清单及binary摘要，再创建新runtime根并复制binary；不得直接使用主方target中的exe。
cargo run --manifest-path testing/probes/establish-node-access-na022-independent/Cargo.toml --locked --offline
```

测试占用专属30452，保留用户30447。取消/拒绝/重启每组建立自己的节点与数据，9行旧材料断言全部满足才exit0；另检查绑定key的新status证明可读取取消/拒绝终态、ID为null、实际hostProof验签。每组owner stop/join、检查确切pipe不存在后删除自己的运行data/cert/key；保留结果JSON。runtime根不得使用任何既有用户目录，组目录必须不存在。

2026-10-30：R12固定binary模式已实际执行，默认3组9行通过。设置NA022_APPROVED_ONLY=1会另跑批准结果受控丢弃后留存内新proof重取分支，保存approved-results.json；该分支也已通过。正式证据见2026-10-30-r12-na022-independent-acceptance.md。自然过期及max留存边界由独立运行对应冻结cfg(test)测试补足，不冒充此网络探针已覆盖时间边界。
