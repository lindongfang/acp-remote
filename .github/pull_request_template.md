<!--
PR 模板。判据来源是 AGENTS.md 的完成定义（§11）与文档维护映射（§10），不在这里另立一套要求。
模板的价值在于让「这次改动影响了哪些权威文档与门禁」在 PR 上可见，而不是重复 AGENTS.md 的正文。
不适用的小节删掉即可，但不要留空勾选。
-->

## 变更内容

<!-- 做了什么，以及为什么现在做。若这是 agentic 变更，链接 openspec/changes/<change>/。 -->

## 影响面

- [ ] 未改变产品行为、协议 wire、安全模型、端口合同与模块依赖（若改变，见下一节）
- [ ] 若改变了上述任一项：已按 `AGENTS.md` §10 的映射更新对应权威文档，并在下面列出

受影响的权威文档 / 合同资产：

<!-- 例：docs/SYNC_PROTOCOL.md + schemas/sync/v1/ + fixtures/sync/v1/；docs/SECURITY_DESIGN.md + ADR；deny.toml + README 的门禁说明。 -->

## 门禁

- [ ] `npm run verify` 本地通过（等价于 CI 的 `checks` job：合同门禁 + fmt/clippy/test）
- [ ] 未执行的检查已在此说明原因（平台限制、缺少外部 Agent、网络不可达等），未把未执行写成已通过

新增或修改了下列内容时，逐项确认：

- [ ] 新依赖：已在 `deny.toml` 的 `allow` 里确认许可证，且 `cargo-deny` / `npm audit` 判定通过
- [ ] 新门禁 / 新 CI job：`AGENTS.md` §10 的门禁清单与 `README.md` 的说明已同步
- [ ] 新密钥、token 或密钥材料：没有进入提交内容（`secrets` job 会再扫一次全历史）

## 风险与回滚

<!-- 出问题时的表现、影响范围，以及回滚方式。没有风险的小改动写「无」即可。 -->
