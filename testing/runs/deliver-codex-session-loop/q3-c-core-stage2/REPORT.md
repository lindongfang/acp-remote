# WP4 C 有限核心 stage2 候选交付

截至 2026-11-02，隔离 workspace 已完成 v3 A–E 范围内的 C 核心实现，共 795 个源码文件、20 个改动路径（14 新增、6 修改，无删除）。这是包含真实业务 SQL 的编译候选；尚未安装 schema9、默认 D 工厂或 F 控制器。开发测试通过不代表独立验收通过。

## 实际实现

- 可信 CatalogService：使用真实 NodeRequestAuthority 校验当前 actor 和同一原始 RegistryOwnerScope，提供 register/apply/query；私有 CurrentCatalogActor 不可由外部构造。仓库和服务共享同一个 provider Arc。
- register：先短事务校验身份/权限并关闭原连接；完整 profile 解析、标准化和保护在写事务之外；最终 BEGIN IMMEDIATE 重新核对身份、权限与引用后登记命令。
- apply：先短事务获得完整原始命令快照并关闭连接；解密和完整解析在写事务之外；最终事务重新核对完整命令、DB actor/代次和引用。实现 InstallHarness、PublishProfile、BindTargetInitial。提交时将原始 cipher 与 digest 原样移入不可变 profile、清理命令 cipher，并保存必要结果事实。Next revision 验证原 profile/revision；绑定同值可接受、不同值冲突。有效业务拒绝保存 rejected，损坏/鉴权/存储错误不虚报业务结果。
- query：校验命令及必要结果关系，无需解密已清理的命令密文。只读 resolve_read 使用原 proof、同 registry、同完整操作互斥门，读取选中关系后关闭原连接，再解密生成 WindowsLaunchPlan/SetupSnapshot。
- profile codec：保留 ACP 原始片段、数值词法、对象顺序和未知字段；拒绝解码后重名。Windows 路径使用规范 base64 UTF16LE，保留未配对代码单元；revision 支持完整正 u64。注册 envelope 元数据和 profile 键一致，未知 envelope 字段被拒绝。
- SQL effect/cleanup：原 RegistryConnection 在 BEGIN await 前即有 RAII 所有者。先标注可能 effect，再进行 BEGIN/COMMIT poll；只有真实提交成功才确认提交，真实 rollback 成功才确认回滚。保留原 close waiter，清理确认要求实际 ACK 和 join。未清偿前阻断普通操作，不建立新 owner/pool/任务。
- reconcile：显式调用先处理原回执，再按需最多一个新的只读核对 slot。使用完整原始记录前像、真实身份、预期键/代次/时间及原密文字节核对；缺失 apply 或改写字段失效关闭。只读核对关闭未确认时保留该回执，下一次不盲目重查。owner 封闭后可确认原回执，但不发新 SQL。
- 同原执行连接检查 main.foreign_keys、TEMP shadow 和不明 trigger；业务 SQL 使用 main 限定。没有修改 schema/version/migration，也没有改变原 registry_connection_owner 算法。
- 可选 audit 尚未接入；成功 receipt 明确返回 AuditWarning::Unavailable，不伪造审计成功。

## 已执行验证

| 验证 | 实际结果与范围 |
| --- | --- |
| 编译 | 最终 check 退出 0；另实跑 all-targets + all-features check 退出 0 |
| 开发测试 | 18 通过、0 失败、665 筛除；codec 5、intent 3、保护上下文 1、完整命令 SELECT 1、SQLite scratch/原 owner 7、真实 Node 配对与撤权 1 |
| 格式 | fmt check 退出 0 |
| strict clippy | 整体退出 101；真实基线 298 条，候选 381 条；按诊断代码、消息、主文件及原始代码文本多重集对照，新增 83 条全部 dead_code，新增其他类别 0、原诊断减少 0 |
| 补丁 | git apply --check 与实际 apply 均退出 0；重建后的全部 795 文件与 source-manifest 逐字节相同 |
| stage1 | 原 freeze 未动；810 个成员全部重新核验相同 |

新增 dead_code 来自尚未连接默认 D 工厂的私有 C 核心；未通过 allow、放宽公开性或虚假调用消除它们。因此不能报告 strict 通过。补丁复现应显式使用 git -c core.longpaths=true -c core.autocrlf=false apply，防止 Windows 路径或本机自动换行改变字节。

## 未执行与后续门

schema9 业务 CRUD 实际执行 0；真实 DPAPI 0；COMMIT-poll 未知/VM 故障场景 0；原独立 34 与补充 12 执行 0；F 控制器/新 hooks 安装 0；默认 D 工厂接入 0。SQLite 开发测试使用 scratch 表和原 owner 故障屏障，不是 schema9 CRUD 验收，也不代签独立审查。完整 CRUD 源码已落地，仍需精确候选独立安装门后执行这些验证。

本候选基于90de781，未重基到 A 的另一组合。主仓库产品文件写入 0、提交/推送 0、新依赖安装 0。冻结仅供后续候选审查，不授予主仓库安装、schema 安装或独立测试修改权限。S09 精度 overlay 保持原完整操作序列化，未改变互斥范围。

## D 接入的当前真实接口

```
// impl NodeRuntime 内的当前方法：
pub(super) fn catalog_core(
    &self,
    protector: Arc<dyn SecretProtector>,
    metadata_wait: Duration,
) -> Result<
    NodeCatalogCore<impl CatalogRepository
        + ReadConfigurationPort<LaunchPlan = WindowsLaunchPlan, Setup = SetupSnapshot>>,
    CatalogFailure,
>;

impl<R: CatalogRepository> NodeCatalogCore<R> {
    pub(super) fn configuration(&self) -> &R;
    pub(super) async fn register(&self, actor: &AuthenticatedActor,
        draft: CatalogDraft, d: &RegistryDeadline) -> Result<CatalogRequestId, CatalogFailure>;
    pub(super) async fn apply(&self, actor: &AuthenticatedActor,
        id: CatalogRequestId, d: &RegistryDeadline) -> Result<CatalogReceipt, CatalogFailure>;
    pub(super) async fn query(&self, actor: &AuthenticatedActor,
        id: CatalogRequestId, d: &RegistryDeadline) -> Result<CatalogFact, CatalogFailure>;
    pub(super) async fn reconcile(&self, d: &RegistryDeadline) -> Result<(), CatalogFailure>;
}

fn resolve_read<'a>(&'a self, proof: &'a ReadTargetProof)
    -> BoxFuture<'a, Result<ConfiguredSourcePlan<WindowsLaunchPlan, SetupSnapshot>, DispatchError>>;
```

当前 core.service.repository 和 core.configuration 是同一个 Arc<R> 的克隆。configuration() 目前仅借用 &R，没有 owned Arc 导出；DF-ABI03 的普通工厂 owned 装配尚需在 D 接入范围内完成，不能重新构造 registry/provider。SqliteNodeConfiguration 类型保持私有，未临时公开构造器或放宽 authority 边界。存储根的 crate-only node_catalog_repository 返回实现 CatalogRepository + ReadConfigurationPort 的 opaque 类型；它接收同一原 registry、protector、Node 身份字段和 metadata_wait。

## 交付索引

source/ 为完整候选，before/ 为6个被修改文件的原始字节，changes.patch 为完整20路径补丁。source-manifest.json、changes.json、patch-verification.json、validation.json、strict-summary.json 与 stage1-integrity.json 分别记录源码、差异、实际重建、验证范围、诊断对照和旧候选保护证据。authority/ 与 authority-manifest.json 保留本次使用的设计/有限编码门/测试精度依据。freeze.json 的 members 覆盖本包所有成员（不含它自己）。
