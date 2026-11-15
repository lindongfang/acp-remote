# FR001 三叶安装后复核补记

日期：2026-11-13。独立 reviewer；仅只读检查，未构建、未执行测试。本补记追加于 `DELIVERY-20261113-ENVIRONMENT.md`，不改写其中安装前的历史状态。

**结论：Root 已安装的三叶与获放行的最终候选逐字节吻合；WP4-FR-001 / P1 保持 closed。** 允许合入范围没有扩大。WP4-FR-002 / P2 仍 open；其服务端映射方案见 `FR002-PLAN-REVIEW.md`，不包含在本次环境修复的通过结论内。

| Root 实际文件 | 实际 SHA-256（与候选相同） |
| --- | --- |
| app/src/platform/process_owner.rs | ebb16e0f24e243afe6d42306b9747f9dd573e9a3653b3ca8740b2e8c7f768b1b |
| app/src/platform/process_owner/environment_tests.rs | d8d3b5d2e72a7ed4bfc80715db9a930736ef00ee0999002eafaee7759055b511 |
| app/tests/platform_owner.rs | a8a8a36d1efd8acc944becad359a1dfceaea4622b18f44c36cee65745fe893af |

候选身份仍为 `SOURCE-FINAL-v2.json` 自身 SHA-256 `f1f96636ed110b6786523ccd2f7a0ff50f2ebb442c5465fb5e6b305308beb90a` 和全部 514 叶吻合；新增明确规范的 canonical 身份 `3d1a45f714b38e91d83912ae958093fc36cdc7d3843cfed66dedfbdc2029bff1` 也由本 reviewer 从实际叶重新生成并逐字节核对。旧 243e 聚合声明未被追认。

读取 `../2026-11-13-environment-inheritance-fix-v1/root-post-install-v1/RESULT.json` 的实际安装后收据，并核对三组原始 stdout：

- process-owner：13 passed / 0 failed / 0 ignored，exit 0；09:50:36–09:51:52 +08:00。
- native-release：7 passed / 0 failed / 0 ignored，exit 0；09:51:52–09:51:54 +08:00。
- platform-owner：13 passed / 0 failed / 0 ignored，exit 0；09:51:54–09:52:31 +08:00。
- cargo fmt --all -- --check：收据 exit 0；09:52:31–09:52:32 +08:00；stdout/stderr 均空。

这是主执行者对 Root 集成结果的真实回归证据，经 reviewer 只读复核，不冒称 reviewer 或独立 tester 在 Root 重新运行。之前独立 tester 对固定候选的 33 个不同顶层测试加专项复跑证据仍单独保留，不混淆两者角色。

安装后读取的 `.git/index` SHA-256 为 `ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`，与初始审查及独立验收前后值相同。收据、日志、三叶实际文件及候选输入摘要固定于 `POST-INSTALL-INPUTS.json`。未核准其他未提交修改、FR002 实现、整个 WP4、完整 Web 端到端、真实外部 Harness 或完整版交付；本补记不作这些范围的验收签字。
