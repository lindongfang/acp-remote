# WP2-2.2 §3A 独立公共 API 验收

结论：纯请求身份签发/验签及时间模型通过。实际执行12项测试，12通过、0失败、0忽略；文档测试0项不计功能证据。未发现本范围待关闭问题。CL-018/079/082仅纯模型子范围有证据，完整L5仍未通过；本报告不签SQL、派发或整项产品完成。

## 版本与环境

- 独立tester未继承coder实现对话；依据七份当前spec、设计及当前implementation-contract §3/3A/4/5与已复核阶段定义。阶段定义SHA256：`1560d9d0f737b44d8cc418f83f5d65c84751e406a54f986e07e21e128fad0df0`。
- 受测六份源码在执行前后与主执行冻结摘要逐一相同。完整列表及关联类型、claims、strict_json、identity、Cargo与测试资产摘要见`final-manifest.json`。主要两份：domain/requests.rs=`74373f501d5f012fbcd643218f0ae154235d248bcaf51938dc21a88c24036cb1`；adapters/product_requests.rs=`0e75f0d1dff02fa25b76f5e2fdab5c75dd2f38e76ef63115b34e11f150f5f3ea`。
- Windows NT 10.0.26300.0；rustc 1.98.1、cargo 1.98.1，实际版本及UTC时间见`execution.json`。
- 独占外部`probe/`和`target/`；通过path依赖真实产品公共库API，未复制或替换产品实现。合成固定P256测试key实现公开ProductRequestSigner，未读取NodeIdentity、DPAPI、用户凭据或用户数据库；未启动Harness、节点、WSS或浏览器。
- 先复制产品Cargo.lock为种子，用`--offline`解析外部探针；正式保存证据的复跑另加`--locked`。外部非探针237个包的名称、版本、source、checksum均与产品261包中的对应项一致，无新增或更换registry包；仅删去24个外部探针不需要的产品开发依赖图条目，并增加本地探针根包。见`lock-comparison.json`。

## 实际命令及证据

在`D:/Project/acp-remote`运行，`CARGO_TARGET_DIR`固定为本报告同目录`target`：

```powershell
cargo test --offline --locked --manifest-path C:/Users/jack/.codex/visualizations/2026/10/30/01a18dc1-45b2-7f82-981e-964264221cef/deliver-codex-session-loop-request-tester/probe/Cargo.toml -- --nocapture
```

首轮全量编译及12项测试exit0，随后同范围保存日志复跑exit0。`public-api-test.log`记录真实输出；PowerShell对cargo正常stderr显示NativeCommandError格式标签，cargo实际退出值为0且12项全部ok，此标签不是产品失败。

## 逐子断言实际结果

以下P标签对应已复核定义的12行，并非新CL ID。

| 子断言 / 实际测试 | 状态 | 实际结果 |
| --- | --- | --- |
| P-shape / p_shape_exact_claims_default_lifetime_public_signer | pass | 实际ES256 Compact JWS、64字节P1363签名、精确10字段、默认86400000ms；公开verify返回同request/原期限，Debug不泄露compact |
| P-identifiers / p_identifiers_canonical_lengths_and_hex | pass | 四种128位字段错误长度、padding、非法字母、非规范尾位拒绝；node/digest非64位小写hex拒绝；合法对照通过 |
| P-enums / p_enums_and_exact_schema | pass | 9动作及5目标种类分别公开issue/verify往返成功；未知枚举、每一字段缺失/错类型、未知字段及有效签名的重复actorId均拒绝 |
| P-signature / p_signature_purpose_isolation_and_envelope_regressions | pass | 三段逐一改字节、错key、旧六用途作为产品、产品通过旧六实际验证入口、错误算法、未知/重复header、DER签名、padding/第四段/超16KiB均拒绝 |
| P-bindings / p_bindings_independent_expected_context | pass | 分别改变已验证上下文的actor/action/targetKind/targetId/digest/epoch拒绝；合法签名但错误nodeId拒绝；全匹配通过 |
| P-millis / p_millis_raw_integer_forms_safe_bounds | pass | issuedAt/expiresAt各自负值、1.0、1e3、字符串、null、超过安全整数拒绝；零毫秒合法短期限通过；安全上界与默认24h加法界限正确 |
| P-window / p_window_future_iat_short_and_default_cutoffs | pass | 未来1ms签发、非正窗口、24h+1ms拒绝；1ms短期限及精确24h合法签名通过 |
| P-expiry / p_expiry_exact_millisecond_without_renewal | pass | 同token在exp-1ms通过、exp与exp+1ms拒绝，原期限未改写 |
| P-clock-start / p_clock_start_imported_floor | pass | 启动墙钟低于导入水位、超安全整数拒绝；等于或高于水位正确，getter不把候选误称已提交 |
| P-clock-effective / p_clock_effective_max_wall_and_elapsed | pass | 墙钟停滞时elapsed推进有效时间；墙钟超前及elapsed再超前均取正确最大值；相同采样允许；墙钟未到截止但单调推至截止时旧token过期 |
| P-clock-sticky / p_clock_sticky_failures_missing_regressing_overflowing | pass | 墙钟/elapsed回退、缺样本、墙钟越界、Duration转换或锚点加法越界均故障；其后前调或恢复合法时间仍拒绝 |
| P-watermark / p_watermark_candidate_never_claims_imported_commit | pass | 候选单调推进，imported_watermark始终保留原导入值；安全上界合法零elapsed样本正确 |

## 未验证与交付边界

没有执行SQL事务、CAS/COMMIT未知、去重单次认领、持久水位、正常重启/备份恢复新代际、10000保护容量/100000审计容量、安全清理、在途输入消费、真正的新鲜认证/目标授权/审批、实际受理或派发。它们仍由CL-004/018/079–084等完整用例及后续阶段覆盖。逐action DTO与摘要编码尚未冻结，本次只验证预先计算摘要的严格形状与绑定，不能凭本次通过派发业务动作。

时间输入来自可控纯模型，不是操作系统改钟或真实重启证据。返回RequestTime/VerifiedProductRequest不能证明水位已持久提交或当前具备危险动作资格。调用方在签发/受理/派发边界重新采样的接线未建立，本次未代签。所有真实Harness及原L5/L6义务保持not-run。

验收执行仅写独占外部目录，未改产品文件、索引或用户未提交内容；无测试数据库、网络端口、子进程或用户资源待清理。保留探针、锁文件、日志与外部target供同版本复核。

## 验收资产交付

主执行者随后授权独占新增`app/tests/acceptance/request_identity.rs`，已从外部`probe/src/lib.rs`原样复制，仅执行Rustfmt，无预期或断言变更。仓库交付文件SHA256：`416059cbc017c276169a65f1fdd7732525ef1bac1f9f1ed6cf7c1721f9689784`；见`repository-delivery.json`。Rustfmt检查及git diff --check exit0；六份产品源码再核对仍为冻结摘要，见`delivery-source-recheck.json`。根集成测试模块由主执行者接线，其后的仓库执行结果不由本次外部执行冒充。

### 入仓风格修正及实际仓库复测

父执行接线后指出clippy `manual_string_new`，tester在其独占验收文件内将两处`"".to_string()`改为`String::new()`。仅修正空字符串构造，测试输入和预期不变。上述初次交付SHA保留历史；最终交付文件SHA256为`30813e05bc76412cc9c52e31419354ab324dd8b04f845bf8f65f6d622b44b283`，见`repository-lint-fix-manifest.json`。六份产品源码复核仍全部对应原冻结摘要。

使用同一独占外部target，实际在仓库执行：

```powershell
cargo test --offline --locked --manifest-path app/Cargo.toml --test session_loop_acceptance request_identity -- --nocapture
cargo clippy --offline --locked --manifest-path app/Cargo.toml --test session_loop_acceptance -- -D warnings
```

两命令均exit0；请求身份实际执行12项全部通过，另9项活动测试因过滤未执行，不计本轮覆盖；clippy检查该整个集成测试目标通过。Rustfmt检查与git diff --check也exit0。原日志分别为`repository-request-tests.log`、`repository-request-clippy.log`；退出值见`repository-recheck-exit.json`。这是仓库已接线探针的纯模型范围复测，仍不扩展到SQL/重启恢复/派发或完整L5/L6。外部初轮探针与其日志/锁/摘要保留原始归属，未被改写为本次仓库结果。
