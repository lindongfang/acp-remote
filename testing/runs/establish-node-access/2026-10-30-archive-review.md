# establish-node-access 归档后独立检视

日期：2026-10-30。当前结论：**移动保真与规格保留 PASS；归档后当前引用收口待主方最终清单核验。** 用户已授权且主方实际完成归档，本reviewer未再次执行archive，也未提交、推送或创建PR。仅写本独占报告。

## 第一阶段：归档移动只读核对

- 归档前入口 `.tmp/archive-establish-node-access-20261030/before.json`，本次读取SHA256 `23E94ECB9F3ECD0B894C3F3A8C53293C507C637224E86BB2362494A0413F7844`。
- 目标 `openspec/changes/archive/2026-10-30-establish-node-access` 实际10个文件（包含隐藏元数据与两份delta）；按before.json逐项重算 **10/10完全匹配，0异常，无额外文件**。`.openspec.yaml`保留。
- 原 `openspec/changes/establish-node-access` 路径不存在。实际 `openspec list --json` 活动变更列表已无 establish-node-access；其他活动变更保留。
- 两份长期规格与归档前 ready-metadata 中条目摘要相同：node-access 为 `6486E022C4C745F3787BA7E108C778204A2882AD8EF52C99BECA2D9F32A1EE73`；client-trust 为 `15BEF4EA763BECCF3FF24108139331B241B08B222A2F1BD24DDD183BB08BA285`。采用实际命令输出为摘要权威，后续最终清单再次核验。
- 独立比较两份delta自首个 `### Requirement:` 起全部正文与main对应正文，忽略外围空白后均完全相同。已存在长期定义，不需重复合并；本轮没有改写main规格或归档内容。

## 下一阶段边界

主方正在修改README、Windows文档、能力映射、长期suite及checker的当前路径。等最终固定清单后，本reviewer再只读核验新摘要、当前引用/定义与受影响strict检查。历史 testing/runs、旧manifest、归档内相对链接作为历史证据不由本轮原地改写；新索引应提供归档后正确入口，不把旧清单历史路径不存在误判为产品字节丢失。

本阶段不重跑无关产品全套，既有R20产品、33例PASS及15/15完成证据继续按固定范围有效。最终归档后引用检视尚未完成，不提前宣布该项通过。
## 最终归档后收口：PASS，归档完成

本节取代此前“当前引用待核”的检查点。独立读取最终 `2026-10-30-archive-metadata.sha256`，SHA256 **`8D0408549999B08F20B5DF52F43D0454E49CCBEBBB8AC7BA6C67EB54932555FD`**；逐文件核验 **22/22匹配、22唯一路径、0异常**。清单自身及本报告不在22项中。

再次按移动前before.json重算归档 **10/10字节相同**。隐藏元数据保留，先前源目录消失、活动列表移除结论保持；两份长期规格与归档前摘要一致，未重复合并ADDED要求。归档内历史plan/review/tests等不改写。

实际核读 archive-record、README、Windows文档、能力地图、后续Codex变更依赖入口、长期suite与checker。对清单内归档目录以外Markdown的 **91个本地链接目标**按文件当前基准目录解析，全部存在；不宣称验证URL服务可达或所有标题锚点。初次自有链接检查脚本因根README父路径为空报错，改用绝对父目录后完整重跑成功，未改任何受检文件。后续Codex依赖链接可定位归档proposal；历史docs/REBUILD_PLANNING_REVIEW与旧报告/manifest按archive-record映射解释，不把旧相对链接作为当前导航，也不重签旧清单为当前路径。

将两份长期suite从首个稳定NA用例标题起的全部内容与归档前ready扫描快照比较，规范化CRLF后均完全相同。新增共用执行说明将NA001活动变更校验迁移为两条独立命令，未改稳定ID或各例步骤/预期。checker实际读取归档tests与delta及长期定义，没有把历史活动路径继续当当前源。

本reviewer独立实际执行：

- `node testing/suites/verify-suite.mjs` exit0：**33用例/28场景/2能力，仅NA033为LX**。
- `openspec validate --specs --strict` exit0：**2份规格通过，0失败**。
- `openspec validate --archived --strict` exit0：**1份归档通过，0失败**。
- `git diff --check` exit0，仅既有LF/CRLF警告。两种strict分别执行，不用同时传flag的归档结果冒充规格校验。

新增归档秘密扫描范围亦独立核验：`.tmp/archive-source-scan-20261030` 中22项与最终清单全部匹配，额外规则文件SHA256 `8302661F460B8BF65B329B6678B73F371AC34773882C46638BCAE360A2C58C18`。原结果JSON读取为`[]`；本reviewer在该snapshot根用已验证官方Gitleaks再次执行`dir . --config .gitleaks.toml --redact --no-banner`，实际 **228257 bytes、exit0、no leaks found**。未修改扫描规则或结果文件，不将规则扫描等同任意秘密可被检测的保证。

**最终结论：归档移动、规格保留、当前导航及定义迁移的有限独立检视全部PASS，establish-node-access归档完成。** 既有R20产品检视、独立33/33及任务15/15证据按已固定版本继续有效，无需因纯归档/路径调整重跑产品全suite。本轮没有新增产品改动或剩余归档阻断；只追加本报告，未修改其他文件，没有commit、push、PR或发布操作。
