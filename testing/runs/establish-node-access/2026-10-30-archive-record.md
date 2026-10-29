# establish-node-access 归档记录

日期：2026-10-30。用户已明确要求开始归档。

- 原路径：`openspec/changes/establish-node-access/`。
- 归档路径：`openspec/changes/archive/2026-10-30-establish-node-access/`。
- 归档前CLI活动列表确认15/15任务完成；独立验收33/33 PASS、原问题均闭环，最终独立检视见`2026-10-30-ready-metadata-review.md`。
- `node-access`与`client-trust`的增量均仅ADDED，全部Requirement及Scenario正文已在长期规格存在且一致。没有待合并内容，因此实际执行`openspec archive establish-node-access --skip-specs --yes --json`，保留长期规格原字节，不重复添加要求；这不代表省略规格同步核对。
- CLI归档成功。移动前后10/10文件逐项SHA256相同，`.openspec.yaml`保留，原目录消失，活动列表已移除本change。逐文件摘要为`2026-10-30-archive-files.sha256`；不改写归档内plan/tests/review的历史内容。
- 当前README、Windows说明、能力地图、下一项Codex会话变更的依赖链接、长期suite来源和定义检查器已指向归档路径。能力地图记录15/15完成。长期NA001的共用执行说明给出归档后两个独立验证命令，稳定ID及每例原定义不变。
- 实际检查：`node testing/suites/verify-suite.mjs`通过（33例/28场景/2能力）；`openspec validate --specs --strict`通过2份规格；`openspec validate --archived --strict`通过1份归档；`git diff --check`通过。定义及归档检查不替代行为验收；产品实现未变，沿用R20实际完整检查和独立验收，不假称归档时又运行完整产品套件。
- CLI同时传`--specs --archived`仅输出归档检查，因此最终分别执行两条命令，不把归档任务检查冒称规格校验。主方摘要核对脚本首次遇到PowerShell数组解析错误，纠正解析后完成10/10核验；并非归档内容变动或校验失败。

## 历史引用与摘要映射

旧测试报告、旧SHA清单及归档文件内部的路径保持受测时原文。解读其中`openspec/changes/establish-node-access/<file>`时，现定位到`openspec/changes/archive/2026-10-30-establish-node-access/<file>`；文件内容摘要保持不变。历史文档中的相对链接按其原基准目录解释，不把未经修复的历史链接当当前导航。

旧ready/implementation清单仍表示各自历史快照；归档后的README、suite、检查器等有预期文档/路径更新，不要求旧元数据清单继续匹配当前树，也不重新签署旧报告。当前归档与导航资产另列`2026-10-30-archive-metadata.sha256`，其自身及独立`2026-10-30-archive-review.md`不入条目，避免自引用；独立reviewer对移动和最终引用另行核对。

本轮不改产品行为、依赖或长期规格正文，不创建/删除账户、进程或用户资料，不修改Git索引，不提交、推送、创建PR或合并。
