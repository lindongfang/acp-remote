// agentic 门禁入口（`npm run check:agentic`）。
//
// 为什么需要这个 wrapper：工作流引擎 `@fission-ai/openspec` 默认**开启**遥测（opt-out 模型）——
// 每次 CLI 调用后由 cli 的 postAction 钩子执行 trackCommand，上报命令名与版本；它还有一次更新检查
// 会请求 npm registry（core/version-check.js）。两者只在「检测到 CI 环境变量」时才自动关闭，开发机与
// Agent 上默认开启。本仓库要求不得引入未授权的遥测或云依赖（`AGENTS.md` §2、§11），因此这里显式设置
// opt-out 环境变量，让门禁在任一台机器上都保持离线语义；扩展转发引擎时会继承本进程环境
// （`@dongfanglin/openspec-agentic/src/project.mjs` 的 runEngine 只传 cwd 与 stdio）。
//
// 两个步骤的判据见 `AGENTS.md` §10：
//   1. `openspec-agentic doctor`：断言所用流程确为扩展（@dongfanglin/openspec-agentic）的 agentic
//      —— 项目本地引擎版本等于扩展 pin 的版本、`openspec/config.yaml` 的 `schema` 为 `agentic`、
//      `x-agentic.configVersion` 为 1、受管文件与清单无漂移、`AGENTS.md` 有验收路由；
//   2. `openspec validate --all --strict`：校验变更与规范资产，无活动变更时以 0 退出。

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const env = {
  ...process.env,
  // 引擎的遥测与更新检查都以这三个变量为硬开关（env 优先级高于全局配置）。
  OPENSPEC_TELEMETRY: "0",
  OPENSPEC_NO_UPDATE_CHECK: "1",
  DO_NOT_TRACK: "1",
};

const steps = [
  {
    label: "openspec-agentic doctor",
    cli: ["node_modules", "@dongfanglin", "openspec-agentic", "bin", "openspec-agentic.mjs"],
    args: ["doctor"],
  },
  {
    label: "openspec validate --all --strict",
    cli: ["node_modules", "@fission-ai", "openspec", "bin", "openspec.js"],
    args: ["validate", "--all", "--strict"],
  },
];

for (const { label, cli, args } of steps) {
  const path = join(root, ...cli);
  if (!existsSync(path)) {
    console.error(`agentic 门禁：缺少 ${cli.join("/")}，请先在仓库根运行 npm ci。`);
    process.exit(1);
  }
  const result = spawnSync(process.execPath, [path, ...args], { cwd: root, stdio: "inherit", env });
  if (result.error) {
    console.error(`agentic 门禁：${label} 启动失败：${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) process.exit(result.status ?? 1);
}
