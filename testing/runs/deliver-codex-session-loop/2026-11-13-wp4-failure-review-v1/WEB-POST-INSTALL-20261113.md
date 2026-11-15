# Web 错误合同安装后复核

2026-11-13。结论：**获审精确九叶在 Root 安装身份正确，主区 check/test/build 适用回归通过。** 此补件保留先前 WEC-FR-002 关闭和窄合结论，不修改原交付报告，不代表完整 WP4 或真实 WSS/Harness 验收。

本 reviewer 只读检查实际源树、备份、journal、脚本、原日志、Vitest JSON 与依赖清单，没有安装、运行产品测试/构建、修改任务定义、产品或 index。主收据 `root-integration-v1/POST-INSTALL-RESULT.json` SHA-256 为 `7624c8d7270ec617924d831453e7e7ac725302d8d46ee5a79b9de5cb748fa8a7`。本次全部输入及重算结果固定于同目录 `WEB-POST-INSTALL-INPUTS.json`，SHA-256 `53e2ad6d575c89f1d00e367780c8c09b0a5d00c5d9ff6508e7b4a8f07fb178e0`。

## 实际安装身份

再次独立枚举 Root 与候选的 src/tests 加五配置/入口文件，两者均为 351 叶，完整路径、长度和 SHA-256 与冻结 `SOURCE-GREEN.json` 一致，canonical 为 `0448cbe203f87f1b75077f07416e1d8a021633fc8871e874d6a8528b453297a0`。按路径 Ordinal 排序，UTF-8 无 BOM 的 `path<TAB>bytes<TAB>lowercase SHA256<LF>` 并保留末尾 LF 重算。实际差异仍恰为已批准的三产品叶与六测试叶；没有增加独立 tester 的 acceptance 文件。

`INSTALL-JOURNAL.json` 记录的九个写入路径与源清单实际差异相同，verified=true；五份旧文件备份逐字节哈希与 BASELINE 对应旧叶相同。index SHA-256 仍为 `ac76c7ad84c1d4333fc64a710647e60aeb96015eb4407f996a4bf6adc91fbff4`。

已核安装器 `install.mjs` SHA-256 `28c65bf0621c713d65e1deb2e3513bae0f11d0e9070ab2765dcba3fca017ec29` 与复验器 `run-check.mjs` SHA-256 `24cb81528f7f8059aae57b7b15abe1175f315906fef30e085f67be1f5e74e963`，均保持门审原字节；另亲读新增只读汇总脚本 `verify-post-install.mjs`，它没有重新运行测试/构建，只核备份、源树和既有收据并拒绝覆盖。

## 主区实际运行

三组 RESULT 的 project 为真实 Root `clients/web`，native config loader、TEMP/TMP/LOCALAPPDATA 与缓存/输出均使用本次独占运行目录。开始和结束源身份均为上述351 canonical/index；主收据引用的 RESULT/stdout/stderr 哈希全部与实际原文件一致。

| 运行 | exit | 原始结果 |
|---|---:|---|
| root-check-v1 | 0 | TypeScript 无诊断，stdout/stderr 空 |
| root-test-v1 | 0 | 34 文件，917/917，失败/待办/跳过均零，stderr 空 |
| root-build-v1 | 0 | TypeScript 与 Vite 成功，23 模块，HTML/CSS/JS 输出，stderr 空 |

Vitest 原 JSON `b603a642643cba2dd9851ddb160ac3ae940ecac3dfc97adeccd06a5e0361fafc` 内917条断言全 passed，与原 stdout 一致。原 stdout 是带 BOM 的 UTF-16LE；本 reviewer 首次纯只读捕获误按 UTF-8 匹配而在输出收据前停止，检查 BOM 后修正解码并重新执行只读核验，没有改原日志或重跑产品测试。

三次运行的六份 `DEPENDENCIES-BEFORE/AFTER.json` 每叶均与当前实际1439叶依赖树一致，canonical 皆为 `dd097a21189008970617be4dba6aeba4ac78f899a07858fecf1b6ad628eed6ca`。这证明前后持久身份未漂移，不宣称 OS 每一瞬间强制只读。

## 保留边界

此为 Root 安装后的身份及开发回归核查；原独立917+72验收仍由前一交付报告负责，不能将本917再次算成新增独立用例。351叶范围不含原有qa、依赖或生成物。没有 main DOM、真实TLS/WSS、Node/数据库、49 producer动态故障注入、Harness、Rust或完整WP4签字；没有 npm audit、网络安装、Git index 修改或完整任务关闭。
