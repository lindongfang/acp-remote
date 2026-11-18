# 主树身份与子目标读取合同

2026-11-16，RB-001 的实现细化，待独立实现门复核。本文件不新增 child 原生加载能力。规格及 CP-019–023 保持原完成判据。

## 身份与存储

标准 v2 `subagent_update` 的外层 session 是立即父，update.sessionId 是 child。只有协议观察端口能够构造关系证明，证明绑定原 source instance/runtime/connection、sequence、receipt 及这些原观察身份与父子身份的摘要；原消息继续由已有观察所有者保留，摘要不声称是原消息字节哈希。页面提交的 ID 不能构造证明。v1 同名扩展、tool 文本、fork 及缓存正文不建关系。

持久关系以 `(source_id, child_native_id)` 为键，保存 parent、原观察来源与证据。后续相同父的观察幂等；不同父、同一 ID 既为已验证主会话又为 child、跨 source、环均拒绝。没有完整已验证祖先链时不允许加载：链必须终止于既有标准 new 响应形成的可靠 root 证明，不能把“查询不到 parent”当成 root。关系保留到节点重启后用于身份解析，不作为当前活动或当前连接归属依据。

增加窄 `SessionTreeRepository` 端口（可作为容量仓储的稳定子端口）：记录原关系证明、查询给定 source/native 的 root 和有序祖先链。SQL 查询和写入走现有必要事务所有者；记录失败和提交结果不明设置该来源必要管理缺口。内存先安装保护事实，再启动有所有者的必要保存任务；只有保存成功解除对应缺口，ACK 不能越过内存保护安装。

源活动账本的关系图与持久图分开。当前连接观测的子工作按完整祖先链保护 root。持久恢复的 child 没有当前快照时前台未知；未报告字段不清除此前已知活动。历史关系可恢复身份，但历史工作状态不复活也不清除当前工作。关系冲突保护原 root 及来源，不自行迁移容量。

## 读取主体与资源所有者

读取路由明确保存 `view_subject`（用户所看 native）与 `owner_resource`（root 原容量/协议资源）。路由分为 `RootResource` 和 `ChildAutoDelivered`。原 `SourceReadCapability` 是 root 协议资源能力；不能克隆它再仅替换 native 为 child。

直接打开 child 时，先从持久图解析 root，沿既有 capacity_owner 路由复用原 source/config owner；否则为 root 取得唯一名额并真实执行 root resume。root 恢复失败、关系无法验证或满额且无候选时，不发送任何 child resume/load/close。并发查看同树 children 共用同一 root 安装过程和原名额。

`ChildAutoDelivered` 只允许将已获授权的 child 作为视图读取主体，接收原连接自动投递的 child 事件。它的激活依赖 root 的原 resume 响应，响应仍保留 root 身份；它不能生成 child resume/replay/close 帧。原生 root 加载与父正文 UI 是否打开分别记录。

child 的当前事件、已有缓存和实际可得历史如实呈现。没有标准独立历史 query 能力时，读取控制返回现有“当前连接/历史不完整”结果。root replay_complete 不产生 child 历史完成证明。CP-023 必须实际看到 child 内容或事件，不能以空页或历史限制提示代替正向验收。

## 集合、冻结与返额

本路径只取得 root 原生加载资源，child 不另建容量行、不置 root_verified、不拥有独立原生关闭权。因此 schema13 的物理原集合仍捕获 root native 的全部原 reservation；不能声称它捕获了独立 child 原生资源。

所有 child 视图读取任务、在途查询和有效审批归 root 责任，登记必须在原 source gate 下重验 root authority。自动 close 首字节前按 root 当前全树事实、所有队列及必要管理检查；首个 Accepted 时封闭 root 及全部已知 child 的视图注册，后续新关系/事件只保留事实，不恢复旧 authority。

root Closed 仅证明 root 原加载资源关闭。返额还必须等待原 root/child 节点读取任务 join、原有效审批及必要管理记录收敛；不能据它宣称所有 Harness 内部 child 会话销毁。未知/失败或最后一个节点任务未收敛时保额。关闭后的旧 child 路由不可绕过 root 新一代预留，也不能对旧 reservation 追加新责任。

## 检查与合入顺序

先实现持久关系及内存活动图，再接 root 解析/恢复与 child 自动事件读取，随后让自动候选纳入完整树并启用统一准入和扫描。持久图与读取接线分包在隔离副本进行，涉及同一 `capacity_records.rs`、`runtime.rs`、`source.rs` 的支持修改由主执行者串行合入。

开发及独立测试分别覆盖同树并发一额、跨配置沿 root owner、相同/冲突/环关系、重启身份与当前活动分离、root 未打开时 child 真实读取、root 恢复失败、最后 child 任务阻止返额、首字节前关系更新与关闭后迟到关系、root 完成标记不扩展到 child。真实 Harness CP-023 仍独立执行，不由受控 peer 替代。
