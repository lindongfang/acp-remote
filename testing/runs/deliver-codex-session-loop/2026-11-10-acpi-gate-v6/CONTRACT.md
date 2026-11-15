# ACPI 路由 v6 编码前审查包

日期：2026-11-10。状态：待独立复核，未放行编码、未合入活动规格。

本包补齐 v4 的设计同步缺口，并适配当前已经接入 queue_management 的源码。v5 保留原样；本包不把历史 tester 的有条件接收改写成实际验收。文件摘要见 MANIFEST.json，产品输入摘要见 SOURCE.json。

## 完整输入与优先关系

- spec-candidate.md、tests-candidate.md 是 v4 原字节副本。
- CONTRACT-v4-reference.md 保留全部 v4 技术选择、八组时序和被动观察约束，不以摘要替代。
- REVIEW-v4-reference.md、TEST-DEFINITION-REVIEW-reference.md 保留原审查，均非本版通过证据。
- 本文只在下述“当前源码适配”与“准确写集”取代 v4 的旧安装范围及旧外部路径。其余合同全部保留。
- DESIGN-DELTA.md 是待插入 change design.md 的实际正文，插入 Decisions 末尾、Risks / Trade-offs 之前。
- tests-candidate.md 内 CONTRACT-v4.md 的引用合入时改为活动 change 内的 management-input-upload-contract.md，内容为本合同和完整 v4 技术正文的单份集成文件；不改变验收步骤或预期。候选中的“Root当前没有”“v4候选”等历史状态导语应在正式合入时仅更新为增量验收定义，不改变用例。

## 当前源码适配

当前 management_ws.rs 有两个独立 OnceLock（queue_reads、queue_management）；lifecycle.rs 先后安装两者。v4 仅提 queue-read/upload，已不足以描述当前完整服务集合。

采用一个 OnceLock<NodeManagementSessionServices>，包含原队列读取、原队列管理、原上传服务。该 facade 在 node/session_services.rs 定义，字段私有，只能由原 OwnedNodeSessionServices 的方法构造，不接受三份外来句柄拼装。它保存原 socket_authority；提供 belongs_to、queue_reads、queue_management、open_upload 方法。ManagementService 的单次安装先验证 authority，再对唯一 OnceLock set；任何失败不改变原集合。仅控制面不安装。

node.rs 仅增加此 facade 以及 UploadOwner 的 crate 内重导出，以跨模块使用私有实现；不公开 NodeInputUploads 或增加它的可变安装入口。现有 UploadOwner 的方法与状态类型均可透过返回值投影使用；如果 Rust 可见性要求额外重导出 UploadPurpose/UploadPhase，则仅重导出这两个现有类型，不改其语义或算法。

原 install_queue_reads/install_queue_management 生产入口由单一 install_session_services 取代。旧“单次安装/foreign node”开发测试改为对完整 bundle 断言，保留 duplicate/foreign 两类错误与原服务继续可用的证据。queue_reads 与 queue_management 命令改从 bundle 取得原服务，不改变登记/consume 业务规则。

四条上传命令与 Binary 共用 attachment 中 canonical UploadOwner。文本错误直接使用 UploadError::code；队列管理里已有的 Upload(_) → invalid_request 不应被当作上传路由错误映射依据。此次不修改独立队列错误语义。

## 准确写集与串行顺序

本阶段只接 Rust WSS 上传，不将尚未定义的 Web 上传协调、普通输入 UI、审批或恢复接线纳入本次编码门。它们仍由现有 WP4 后续任务承担，不从完整 change 删除或视为完成。

方案通过后按下列顺序合入：

1. 新增活动规格 specs/management-input-upload/spec.md，来自 spec-candidate.md；仅删除导语中尚未合入候选的状态描述。
2. proposal.md New Capabilities 增加一条该 capability 的登记。
3. design.md 插入 DESIGN-DELTA.md 正文。
4. 新增活动 change 的 management-input-upload-contract.md，集成本合同与完整 v4 技术正文并更新同包引用；implementation-contract.md §6 增加指向该子合同的链接。
5. tests.md 追加测试候选全文，仅更新合同链接和候选导语；CL-001—123 原文保留。该机械集成不授权 coder 改验收预期。
6. plan.md 记录独立复核、WP4 串行写集和 WP5/WP6 同版验证；tasks.md 只给现有 4.1 增加 ACPI-R01—R08 完成判据，不新增 checkbox，不勾任何任务。
7. 产品文件：management_ws.rs、management_ws/session_commands.rs、node/session_services.rs、node/lifecycle.rs、node.rs 以及 node/session_services/queue_read_development_tests.rs 或该目录独立上传开发测试叶。如需要模块声明仅在上述现有文件添加。不改 upload/preparation 算法、ACPI codec、connection_owner 或 Source facade。

产品修改前再次核对这些文件的实际摘要和已有修改，若变化则重新评估差异，不能整文件覆盖用户改动。正式独立验收和交付检视使用实现后新的同版源码快照，不使用本次规划摘要替代。

## 证据与待审事项

R01—R08 的历史 tester 有条件接收不代表 allocation_failed / necessary_storage_unavailable 两类业务错误已取得 WSS 故障注入证据。本版不删减该缺口、不声称全错误表已覆盖；独立 tester 须明确适用层级或提出最小注入方案，reviewer 再核对。新增 seam 沿完整 v4 合同，只在允许叶内实现；若做不到，先报告具体超出范围原因。

此包没有执行 Cargo、产品测试或真实 Harness，没有改活动 specs/产品代码、Git index、数据库、密钥或原生会话。历史阻断是否在当前版本得到解决由独立 reviewer 判定；主执行不代签原报告的问题关闭。
