# 最终metadata/evidence独立检视

日期：2026-10-30。**4.4内容同步条件PASS，可由主方勾选并同步plan；最终刷新后的清单摘要仍需一次独立读回。** 当前产品源码及reviewer负责复查范围PASS；整体仍因NA012另一真实SID pair/stop等原必要命令的环境/证据缺口BLOCKED，不是ready或归档批准。

本报告是独占写入文件；没有修改373项清单内的review.md、plan、tasks、统一表或其他资产。清单自身和本报告均不纳入清单，避免自引用。

## 本次373项核验记录

收到的 `2026-10-30-final-metadata.sha256` 自身SHA256为 **`4DEFD2E09AEE41E9C8D86CEE244DEC662D88C9A3D33A3DB0203054B75E0977E0`**，373条、373个唯一相对路径，0缺失、0畸形行、0路径越界或.tmp/target/node_modules/dist生成/私人目录条目。按分组为docs1、README1、openspec12、testing359。

逐项实际结果：**372匹配、1项预期metadata更新不匹配**，不是产品源码漂移：

| 文件 | 373清单记录 | 本次实际读回 |
|---|---|---|
| testing/runs/establish-node-access/2026-10-30-current-independent-status.md | A52B599D90B883C9EF08FD9708768537AB067C68CB39E7184941EFCEA84A4B46 | 884EF9BECC87344CDF978C8CFBF4AE94CD2CAB8580C19B49984B94138E5ED8F0 |

原因已由主方确认：清单创建后原tester完成最新统一表。新增 `2026-10-30-component-final-assets-check.md` 也晚于373项，尚未纳入，实际SHA256 **`312FC5EC2102147F7C3272DCAC3D38DFD801A89ADC21BC6CC987DED70495AB09`**。均为明确元数据版本差异，不作为产品FAIL、源码漂移或新安全问题；但不能把初版373项写成373/373全匹配。最终清单应纳入这份新报告、更新统一表和即将变化的plan/tasks摘要，届时另记录新条数/自身hash及逐项结果。

## 4.4内容判据逐项判断

| 判据 | 有限独立结论 |
|---|---|
| 当前结果与问题闭环写入plan | **PASS。** plan最新R20检查点已明确NA007原提出者真实TTYClosed、32PASS/1PARTIAL12/0FAIL、原FAIL历史保留及环境剩余；与当前统一表正文一致。旧R19/R17阶段明确标历史，不要求删除旧统计。 |
| component任务依据 | **PASS。** 原testercomponent-final-assets-check已读，只核读合并而未虚报新跑验收，确认2.3/3.1判据满足；tasks实际11/15，与plan一致。勾选ID为1.1/1.2/1.3/2.2/2.3/2.4/2.5/3.1/3.2/3.3/4.1。reviewer不自行改tasks。 |
| 按能力长期同步 | **PASS（定义/引用）。** 实际执行 `node testing/suites/verify-suite.mjs` exit0：33cases/28scenarios/2capabilities，长期与增量规格正文相同，source/suite三中文字段定义相等，场景/稳定ID/matrix关联有效。该脚本不判断行为充分性或执行通过，未恢复旧12字段。 |
| 文件路径、当前ID、旧新编号映射 | **PASS。** active review所有报告相对链接实际存在；长期matrix与plan明确旧033安全汇总已归002/012/014及review，旧034映射当前033用户理解。当前33例，无把旧033安全结果挪给当前033。 |
| 同一集成版本及可复用范围 | **PASS（现有负责范围）。** R20冻结源83/83、产品561A6024…AAFBE3、144库+4/Web50开发全日志、各真实独立报告/定向复查按字节和适配范围记录。普通CLI变更由原tester用R20真实TTY补验；未变Web/后端正式链不为metadata更新全部重跑。R20非plan/tasks/review的80项此前核对0漂移。 |
| 原NA007问题关闭权限与证据 | **PASS。** 原提出者新R20报告SHA256477D8CD4…1A2AB，30项原始证据本reviewer重算全匹配，三分支/同key恢复全true；旧R17FAIL原文保留。源码/helper与真实TTY角色分开，不由coder或reviewer代签原tester。 |
| 秘密、无关改动和扫描边界 | **PASS（本次资产内容及已记录扫描范围）。** 清单排除私人/构建目录；已读新增公开汇总无运行凭据。官方工具Gitleaks、Rust公告、npm锁/审计和许可证/平台结论按原报告有效范围复用。主方末扫716文件/8039244bytes/exit0，结果实际读回[]，摘要37517E5F…5B570；reviewer之前另有713副本独立扫描0/exit0，不冒称亲自重扫了716。最终增补本报告/状态文字不倒称属于旧扫描副本，后续source扫描按主方所述覆盖最终metadata。 |

因此，**4.4作为“准确同步结果、长期定义与元数据”的内容条件已经满足，可由主方勾选并记录其有限结论**。当前尚未封存的新清单完整性待刷新后核验，是已声明的文档收口步骤，不是要求NA012先通过或新增产品场景。不能因为整体环境BLOCKED就把已经完成的4.4定义同步判成未实施；同样不能因为4.4勾选就把2.1/4.2/4.3所受NA012约束取消。

## 最终刷新读回所需范围

1. 纳入新增component报告，刷新统一表当前实际摘要；主方完成4.4勾选/plan状态更新后刷新这两文件摘要。若还新增其他公开报告，列出真实增项，不以“只变两项”掩盖已知统一表/新报告变化。
2. 提供新清单自身SHA256和条数；本reviewer将只读回最终条目、内容结论与映射，不重跑产品suite，也不再改清单内review.md。
3. 本清单自身、本独占review报告继续排除。所有产品冻结源/ordinary binary及旧证据保持原摘要，不覆写历史清单，不把新metadata说成旧83项覆盖。
4. 当前统一表由原tester维护，本reviewer只核对33行确为32PASS/1PARTIAL且无FAIL；NA012另一真实SID剩余仍保留，不新增另一机器、完整法律审计、硬件断电或无限调度门禁，不要求用户重做已完成技术验收。

## 最终交付边界

**产品范围PASS；整体交付BLOCKED仅NA012环境/必要证据未满足。** 无当前确认产品FAIL，NA007原问题已Closed。4.4有限内容PASS及最终metadata清单核验不构成ready、归档、提交、推送或发布授权。

本轮仅写本报告，所有其他文件只读；未运行节点、访问用户30447、接管日常浏览器、修改源码/规格/plan/tasks/旧报告或清单，未提交、推送或归档。

## 最终374项刷新读回（本节为当前有效结论）

**最终metadata/evidence清单完整性PASS：374/374匹配，0缺失、0摘要异常、0畸形条目、374个唯一路径。**

最终有效清单：`testing/runs/establish-node-access/2026-10-30-final-metadata.sha256`。

**自身SHA256：`2532C35AF3E41850B9EB3CEA1190260197C92B63211CF901EC7F327D4043A8D9`。**

此前373项清单摘要4DEFD2E0…E0977E0及372/373记录保留为首次读回历史，不再作为最终有效清单。当前已纳入component-final-assets-check，统一表/plan/tasks已正确刷新；不是把初版异常删除或冒称当时全部匹配。

| 本次已读回的变更项 | 最终实际摘要 |
|---|---|
| openspec/changes/establish-node-access/plan.md | 591DCD3A67F97B419D2D51367F047CF8BF33420DA857524060BCA43F6AB72259 |
| openspec/changes/establish-node-access/tasks.md | 33C2C2AE54D4EE896C89732F69B5FAB4CF822BF2BC97D4DD9BF95DFCCFF7FAAB |
| testing/runs/establish-node-access/2026-10-30-current-independent-status.md | 884EF9BECC87344CDF978C8CFBF4AE94CD2CAB8580C19B49984B94138E5ED8F0 |
| testing/runs/establish-node-access/2026-10-30-component-final-assets-check.md | 312FC5EC2102147F7C3272DCAC3D38DFD801A89ADC21BC6CC987DED70495AB09 |

实际检查确认：清单自身、本独占final-metadata-review.md均不入条目，无自引用。其余370项同样逐文件核验，不是仅检查四个变更项。

### 4.4与状态一致性最终核验

- tasks4.4已勾选，plan最新明确12/15；仅2.1/4.2/4.3保持待办，原因均准确关联NA012不同真实SID的剩余命令环境/证据。没有借metadata任务完成宣布整个验收通过。
- plan最新已接入NA007原提出者R20真实TTYClosed及旧FAIL保留；统一表为32PASS/1PARTIAL12/0FAIL/0NOT-RUN，与其正文和当前索引的角色范围一致。reviewer核对一致性，不重签一份tester逐例表。
- 再次实际 `node testing/suites/verify-suite.mjs` exit0：33cases/28scenarios/2capabilities，只有033LX；长期定义、source/suite三字段及spec/matrix引用有效。旧033安全汇总已删除、旧034→当前033映射保持，不恢复旧字段或人工签署门禁。
- R20源码/普通产品与独立运行证据按原冻结范围复用；本轮只有metadata读回和定义校验，未把它写成又跑完整144库+4/Web50或真实E2E。

**4.4有限同步及最终清单读回PASS，本轮metadata收口完成。产品源码及负责检视范围PASS；整体交付仍BLOCKED且非ready，唯一剩余NA012跨SID环境/必要证据未满足。**

最后一轮快照秘密扫描的报告按主方安排在本清单外单列；它描述实际扫描时点与结果，不因该清单外报告或本独占review追加而循环重建374项。不得在它实际完成前冒签新扫描通过，也不外推旧扫描到未扫字节。本次未修改任何清单内文件，未改manifest、产品、旧报告或角色结论，未启动节点/浏览器、未提交推送归档。
