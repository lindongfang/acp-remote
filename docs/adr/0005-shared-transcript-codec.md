# ADR-0005：共享 transcript codec 下沉为叶子 crate

- 状态：Accepted
- 日期：2026-09-18
- 影响范围：protocol crate 边界、`identity-auth` 的依赖、workspace crate 数量
- 修订记录（2026-09-18）：表驱动层（泛型表类型与按表校验）也落在 `acpr-transcript`，协议 crate 只保留 `DOMAINS` 数据，因此点 3 的依赖由 dev-dependency 改为正常依赖。

## 背景

[SYNC_PROTOCOL.md](../SYNC_PROTOCOL.md) §6.2 与 §6.3 定义了签名/HMAC 输入的长度前缀二进制 codec（magic `ACPR`、`codecVersion 0x01`、`domainTag`、递增 `fieldTag`）以及 Sync 用途的 domain 与字段 tag 表；[NODE_LINK_PROTOCOL.md](../NODE_LINK_PROTOCOL.md) §9.1 明确 Node Link 复用**同一套 codec 结构**，只新增自己的 domain 与 `fieldTag` 含义（tag 表独立，不得跨协议套用）。

但 ADR-0003 与 `MODULE_ARCHITECTURE.md` §2/§5 规定：协议 crate 各自拥有 codec，且三个协议 crate 彼此不直接依赖。于是这份安全关键的编解码实现无处安放——放进任一协议 crate 就形成协议间横向依赖，分别实现两份就是重复安全代码。

[ADR-0004](./0004-local-admin-transport.md) 之后，同一个问题在本地通道上再次出现：本地管理载荷与 ACP facade 流也需要一套自己的编码，同样不应该复用一个"别人的"协议 crate。

## 决策

1. 新增第 11 个 crate `acpr-transcript`，作为**叶子 crate**：不依赖任何其他项目 crate，只拥有 codec 结构本身——magic/`codecVersion` 校验、长度前缀字段的编码与解码、字段顺序与唯一性检查、错误类型——以及其上的表驱动层：泛型表类型（`DomainSpec`/`FieldSpec`/`FieldType`/`Proof`）与"按表校验并编解码"（字段数量、tag 成员、定长宽度）。它不包含任何 `domainTag` 取值、`fieldTag` 取值或协议语义。
2. **domain 与字段 tag 表留在拥有它们的协议**：Sync 的表继续在 `sync-protocol`（对应 `SYNC_PROTOCOL.md` §6.3），Node Link 的表继续在 `node-link-protocol`（对应 §9.3/§9.4）。协议 crate 只导出 `DOMAINS: [DomainSpec; N]` 常量（纯数据），表与表驱动的校验/编解码函数由调用方组合；`acpr-transcript` 仍然不知道任何 `domainTag`/`fieldTag` 取值。
3. **依赖方向**：`sync-protocol` 与 `node-link-protocol` **正常依赖** `acpr-transcript`——表的元素类型与"按表校验并编解码"都由叶子 crate 提供，协议 crate 只导出 `DOMAINS` 数据；把校验留在 codec 里，字段宽度与校验逻辑才只有一份实现。`identity-auth` 依赖 `acpr-transcript` 与两个协议 crate，取表后调用表驱动编解码。协议 crate 之间仍然互不依赖。
4. 后续若本地管理通道也需要同样的长度前缀编码，复用它而不是复制；若它需要不同的 framing，则在自己的 adapter 内实现，不得为此给 `acpr-transcript` 加上协议语义。

## 结果

- 安全关键的编解码与按表校验只有一份实现（字段宽度不再随协议复制），六个 Node Link domain 与六个 Sync domain 的固定向量在两侧（Rust 与 WebCrypto）继续作为独立判据。
- 协议升级互不牵连：改 Sync 的 domain/tag 表不影响 Node Link 的编译与发布节奏，反之亦然。
- crate 数从 ADR-0003 的十个变为十一个，`MODULE_ARCHITECTURE.md` §3/§5、`AGENTS.md` §4 与架构图需要同步。
- `identity-auth` 依赖三个 crate（codec + 两个协议的表）；这是"依赖传递"而非横向协作：它拿到的是数据与纯函数，不是任何适配器行为。
- 不引入任何"万能 common/util" crate：`acpr-transcript` 的语义边界就是"ACP Remote 的签名 transcript 编码"，不是通用序列化工具。
- 后续：[ADR-0006](./0006-identity-keystore-split.md) 把 `identity-auth` 的平台 keystore 也拆成独立 crate（`identity-keystore`，依赖 `identity-auth` 定义的端口），crate 总数为十二。
