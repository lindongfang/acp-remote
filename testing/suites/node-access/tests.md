# node-access Acceptance Suite

- Source change: establish-node-access
- Source definitions: openspec/changes/archive/2026-10-30-establish-node-access/tests.md
- Long-term specification: openspec/specs/node-access/spec.md
- Stable IDs: NA-001、NA-002、NA-009、NA-011、NA-012、NA-013、NA-014、NA-015、NA-016、NA-017、NA-032

共用执行条件与当前变更用例保持一致，运行结果另存 testing/runs。裸 specs 路径在本长期套件中指 openspec/specs。旧 NA-033 安全汇总合并至 NA-002/012/014；旧 NA-034 对应当前 NA-033，仅由用户确认可理解性。

归档后执行 NA-001 时，将原活动变更验证命令替换为分别执行 `npx.cmd openspec validate --specs --strict` 和 `npx.cmd openspec validate --archived --strict`；以下归档时用例定义文字保留，其余步骤和预期不变。

## Cases

### NA-001 构建及工程检查入口

- 用例级别：L1
- 操作步骤：在仓库根目录按 README 与实际工程配置执行 npm run check、npm run build:rust、npx.cmd openspec validate establish-node-access --strict 和 git diff --check；核对 Rust/Web 实际测试数量与构建目标。
- 预期结果：Rust 格式、Clippy、测试及 Web 类型检查、测试、构建和节点构建真实通过；OpenSpec 与差异检查通过仅代表文档结构和格式。零目标、缺失入口或未执行均不得当作编译或功能验收通过。

### NA-002 静态合同与供应链

- 用例级别：L2
- 操作步骤：在仓库根目录执行 npm run check 与 npm audit --audit-level=high；按 .github/workflows/ci.yml 检查密钥扫描与供应链作业。BASE/HEAD 为真实可解析提交时运行 npm run lint:commits -- --from BASE --to HEAD --verbose。按 README 的浏览器 JWS 入口在真实 Chromium 复核固定向量，并按 app/DEPENDENCIES.md 与实际配置独立核对密码库、产品依赖官方维护状态、许可证、平台支持、漏洞信息及锁文件；结合合同§1–7检查不自造密码学，安全边界与实现的整体符合性由独立reviewer检视。
- 预期结果：现有静态检查、协议向量及适用供应链检查通过；开发工具审计不能替代 Rust/Web 产品供应链审查，CI 作业定义不能当作已执行证据。缺少审计工具、真实提交范围或执行证据时如实记录未执行；结构检查不代表场景覆盖充分；依赖与密码库审查须有可追溯结论和问题关联，未审或未解决阻断项不得写通过。

### NA-009 身份初始化崩溃一致性

- 用例级别：L4
- 操作步骤：在隔离目录的密文临时写入、刷新、更名、meta事务各阶段中止进程；构造只有key、只有meta、key与meta不符、未知schema；重启。
- 预期结果：仅key无meta且可解密同一公钥可完成meta；其他既有身份缺失/损坏明确失败；未知schema不降级、不生成替代身份、不清空目录。

### NA-011 启动及失败释放

- 用例级别：L5
- 操作步骤：使用合同§3命令acp-remote daemon start前台启动；分别提供有效配置、无效origin/TLS、不可写存储与占用端口，失败后修正配置重试。
- 预期结果：只有完整初始化成功才就绪；单节点即可管理；失败原因可定位且不泄漏私密路径/材料，不假就绪；本次锁/监听/句柄释放，用户原数据与身份不变。

### NA-012 本机所有者与独占

- 用例级别：L5
- 操作步骤：owner与另一Windows用户分别运行daemon status/device pair/device list/device revoke/daemon stop；伪造SID、远程命名管道、模拟失败，再启动同目录第二节点。
- 预期结果：DACL及服务端token SID双校验，异用户/远程/校验失败拒绝；同目录第二实例拒绝，不能删未知PID抢锁；owner无需Web身份仍可list/revoke恢复管理。

### NA-013 网络可达不等于认证

- 用例级别：L5
- 操作步骤：分别经localhost和真实Tailscale路径打开管理连接；未认证请求node.info/device.list/订阅，再以可信未撤销Web身份执行相同操作。
- 预期结果：未认证不交付资料、不建订阅；有效身份可管理本节点，同一身份资格不因路径改变；Tailscale只证明真实网络路径，不替代应用认证。

### NA-014 认证错误与秘密保护

- 用例级别：L5
- 操作步骤：发送已知标记的无效proof、已知/未知/撤销ID；收集响应、普通日志、审计、HTTP历史、浏览器storage与网络记录。
- 预期结果：统一失败不能枚举身份，标记的code/proof/私钥不回显或入日志；允许的公钥/身份元数据与秘密区分；测试证据脱敏，不复制秘密。

### NA-015 正常关闭与重启

- 用例级别：L5
- 操作步骤：节点有未完成配对、多连接订阅及在途操作时由owner运行daemon stop；尝试新操作；检查进程/端口/管道/锁并重新启动。
- 预期结果：按合同顺序停止受理、使待配对失效、收敛工作、关闭连接/监听/存储/密钥；仅完成后报成功；记录可恢复，无孤儿任务；本change无Harness子进程树验收声明。

### NA-016 关闭失败诚实报告

- 用例级别：L5
- 操作步骤：用明确故障注入入口阻塞在途工作或令必要提交失败，触发stop并观察15秒期限。
- 预期结果：期限耗尽或保存失败明确报告未完成项，不报全部释放；尽力清理自有资源，不删除数据掩盖失败；故障注入入口待实现，不能拿普通正常关闭代替。

### NA-017 持久信任与损坏保护

- 用例级别：L5
- 操作步骤：配对A/B并撤销B后正常重启，A/B重新认证；只对本例复制数据目录注入key不可读/损坏、meta不符，再启动。
- 预期结果：nodeId和A稳定身份保留、B仍拒绝；损坏启动明确失败，不重置/清空/替代身份；DPAPI跨用户解密不能成功。

### NA-032 Windows Web真实闭环

- 用例级别：L6
- 操作步骤：真实Windows x64 owner启动直接TLS节点；真实Chromium A/B经可信证书HTTPS打开Web，device pair认领、本机批准、认证、list/订阅、撤销B、重启并A/B重连、owner关闭；至少一轮走真实Tailscale路径。
- 预期结果：完整节点单机闭环可观察；真实SQLite/DPAPI/named pipe/WebCrypto/IndexedDB/TLS/Tailscale均不得mock；A有效B拒绝、所有资源关闭；无Harness和移动端验收声明。
