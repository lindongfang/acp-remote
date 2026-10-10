// agentic 门禁入口（`npm run check:agentic`）。
//
// 为什么需要这个 wrapper：工作流引擎 `@fission-ai/openspec` 默认**开启**遥测（opt-out 模型）——
// 每次 CLI 调用后由 cli 的 postAction 钩子执行 trackCommand，上报命令名与版本；它还有一次更新检查
// 会请求 npm registry（core/version-check.js）。两者只在「检测到 CI 环境变量」时才自动关闭，开发机与
// Agent 上默认开启。本仓库要求不得引入未授权的遥测或云依赖（`AGENTS.md` §2），因此这里显式设置
// opt-out 环境变量，让门禁在任一台机器上都保持离线语义；扩展转发引擎时会继承本进程环境
// （`@dongfanglin/openspec-agentic/src/project.mjs` 的 runEngine 只传 cwd 与 stdio）。
//
// 唯一判据见 `AGENTS.md` §9 与 `openspec/config.yaml` 的 `schema` 设置：
//   `openspec validate --all --strict`：校验变更与规范资产，无活动变更时以 0 退出。
//
// 历史上这里还跑 `openspec-agentic doctor`，它断言三件事：项目本地引擎版本等于扩展 pin、
// `openspec/agentic.yaml` 可解析、扩展受管文件（`.agents/skills/agentic-verify/`、
// `openspec/.agentic-install.json`）在位且哈希与清单一致。后两项的前提已不存在——`openspec/agentic.yaml`
// 与那两份受管文件都不在仓库里（受管文件由扩展的 `init`/`update` 维护，本仓库不保留），doctor 因此
// 恒为 FAIL。它断言的「引擎版本 == 扩展 pin」也由 `package.json` 的精确 pin 与 `npm ci` 的 ERESOLVE
// 保证，不需要第二道断言。故该步骤删除；要恢复它，必须先恢复受管文件与清单。

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

console.log("agentic 门禁 OK：变更与规范资产通过 --strict 校验。");
