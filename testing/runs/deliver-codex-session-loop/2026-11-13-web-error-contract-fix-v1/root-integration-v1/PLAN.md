# Web 错误合同主区安装与复验

2026-11-13。仅准备，当前未安装产品、未执行主区 test/check/build。独立验收和 WEC-FR-002 仍须实际闭环，本文不替代放行结论。

固定候选沿用父目录 SOURCE-GREEN.json：351 叶，canonical `0448cbe203f87f1b75077f07416e1d8a021633fc8871e874d6a8528b453297a0`。Root 安装前须为 BASELINE.json 的347叶，canonical `4b50dccdcee713c9fc03d19c0f91f60066b4076ee08002149aeedc001d9a5338`。两清单范围均为 Web src/tests 加五个根配置/入口文件，不含既存 qa、依赖和生成物。

`install.mjs preflight` 已实际 exit0，完整核对 Root347叶和候选351叶、精确九叶差异、四个新增目标不存在、Git index不变。`node --check run-check.mjs` 已实际 exit0。这两项只有读取和标准输出，未安装或执行功能检查。

独立 tester 完成 WEC-001–010、原 reviewer 关闭 WEC-FR-002 并明确放行后，依次执行：

```powershell
node testing/runs/deliver-codex-session-loop/2026-11-13-web-error-contract-fix-v1/root-integration-v1/install.mjs preflight
node testing/runs/deliver-codex-session-loop/2026-11-13-web-error-contract-fix-v1/root-integration-v1/install.mjs install
node --experimental-strip-types testing/runs/deliver-codex-session-loop/2026-11-13-web-error-contract-fix-v1/root-integration-v1/run-check.mjs check root-check-v1
node --experimental-strip-types testing/runs/deliver-codex-session-loop/2026-11-13-web-error-contract-fix-v1/root-integration-v1/run-check.mjs test root-test-v1
node --experimental-strip-types testing/runs/deliver-codex-session-loop/2026-11-13-web-error-contract-fix-v1/root-integration-v1/run-check.mjs build root-build-v1
```

安装器仅精确复制已审三产品叶和六开发测试叶，不引入独立 tester 的新测试。先保存五个既有叶原字节，再写九叶，逐项 journal，最后重新核实际完整351叶与 index。若发生部分写入或任何失败，保留现场；已存在 journal 时禁止重试，须先读取实际状态。不执行 Git、依赖安装、网络、npm audit、删除或回滚。

主区复验使用项目真实配置及本地已安装 tsc/Vitest/Vite，运行方式与已审开发运行器相同；原 package.json/config/锁不改。原生 config loader 避免 bundle loader 写共享 .vite-temp；每次运行独占 cache、模块缓存、TEMP/TMP、LOCALAPPDATA 和 build dist。每次前后均全量比较共享依赖路径、字节数和 SHA-256，并重新核主区351叶与 Git index。这是前后检测，不宣称 OS 强只读或瞬间零写入。原始 stdout/stderr 保存到本目录唯一标签，RESULT/Vitest JSON 保存到对应 runs；拒绝覆盖旧证据。

当前脚本身份：

- `install.mjs`：`28c65bf0621c713d65e1deb2e3513bae0f11d0e9070ab2765dcba3fca017ec29`
- `run-check.mjs`：`24cb81528f7f8059aae57b7b15abe1175f315906fef30e085f67be1f5e74e963`

主区复验不替代独立验收，不覆盖 main 真实浏览器、TLS/WSS、Harness、服务端49个 producer 故障注入、Rust 或整个项目检查。完整 WP4 和 tracked 任务仍按原判据保留待办；npm audit 按用户要求未执行。
