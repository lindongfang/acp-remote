# ADR-0006：拆分 `identity-auth` 的平台 keystore

- 状态：Accepted
- 日期：2026-09-18
- 影响范围：crate 边界、`identity-auth` 的职责与依赖、workspace crate 数量

## 背景

`MODULE_ARCHITECTURE.md` §4.8 把「设备身份、配对、认证、授权」和「平台安全存储」写在同一个 `identity-auth` 里，§12 长期挂着「是否拆成纯状态机与平台 keystore 两个 crate」这个开放项。

两类东西的依赖与可测性完全不同：配对状态、challenge-response 的 transcript 组装与校验、scope/grant 展开、撤销都是纯计算；而密钥落地要调 Windows CNG/DPAPI、macOS Keychain、Linux Secret Service（D-Bus）。后者各自拖原生依赖、需要 `cfg` 分支，在没有桌面会话的 Linux 和多数 CI 容器里根本不可用。混在一个 crate 里有两个后果：状态机无法在所有平台无条件编译与单测；只关心协议逻辑的消费者被迫继承平台依赖与平台构建条件。

## 决策

1. 新增第 12 个 crate `identity-keystore`，唯一职责是实现平台安全存储（DPAPI/CNG、Keychain、Secret Service）；不做业务逻辑、不解析协议、不做授权判定。
2. `identity-auth` 收敛为**纯状态机**：keystore 以本 crate 内部的端口（trait）表达，由组合根注入实现。`identity-auth` 不得直接调用任何平台 API，也不为平台差异写 `cfg` 分支。
3. 依赖方向固定为 `identity-keystore -> identity-auth`；除 `app` 外没有 crate 依赖 `identity-keystore`，`identity-auth` 不得反向依赖它。
4. 端口类型与错误类型由 `identity-auth` 拥有；平台实现只负责存取与平台错误映射，避免平台类型泄漏到状态机。
5. 端口必须允许"非硬件保护"的实现存在（导出型密钥、文件后端），以便将来为没有 Secret Service 的 Linux 提供降级实现。是否启用该降级由 `SECURITY_DESIGN.md` §20 的独立决定与新的 ADR 决定；在此之前正式模式失败关闭。
6. 平台差异用 `cfg` 与各平台子模块表达，不引入运行时插件、动态加载或按名称注册的 trait object 机制（与 `MODULE_ARCHITECTURE.md` §9「暂不设计动态插件 ABI」一致）。

## 结果

- 状态机可在所有平台编译与单测，平台 keystore 的行为测试集中在一个 crate。
- crate 数从 ADR-0005 的十一个变为十二个；`MODULE_ARCHITECTURE.md` §3/§4.8/§4.12/§5/§6/§11/§12、`AGENTS.md` §4 与架构图需要同步（已同步）。
- 「无桌面会话的 Linux 能不能用」从跨 crate 问题降为 `identity-keystore` 的局部问题：将来加降级实现不需要改动 `identity-auth`。
- 代价：多一层 trait 边界与一次构造期注入；`identity-keystore` 不得把平台句柄或平台回调（例如 Windows 的证书选择 UI）暴露给调用方，需要时必须新增端口方法。
