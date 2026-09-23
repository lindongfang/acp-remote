import { execFileSync, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// 提交前的快速门禁。
//
// 为什么需要它：CI（`.github/workflows/ci.yml`）是完整判据，但反馈周期以分钟计，而 agent 与人都
// 倾向于「先提交，等 CI 告诉我」。本脚本把 CI 里最快的三类判定前移到提交时：
//   1. `cargo fmt --check`（改动 Rust 时）；
//   2. `cargo clippy -D warnings`（改动 Rust 时，与 CI 参数一致）；
//   3. `npm run check`（全部合同门禁，离线、~4 秒，见 `AGENTS.md` §10）。
// 刻意**不跑** `cargo test`：它是分钟级判定，属于 `npm run verify` 与 CI，不属于提交前。
//
// 两条必须说清的边界：
//   - 钩子检查的是**工作区**内容，不是暂存的快照。`git add -p`（部分暂存）时被检查的内容可能与
//     提交的内容不同，因此这里对「同一文件既暂存又有未暂存改动」给出警告（不失败，因为这是常见且
//     合法的操作，只是需要作者知道）。
//   - 本地钩子可以被 `git commit --no-verify` 绕过。CI 的三个 job 绕不过去，所以这里不是门禁的唯一
//     实现，只是让失败更早暴露。
//
// 修改判定范围时同步改这里与 `.github/workflows/ci.yml`：两者不一致会让「本地绿、CI 红」重新出现。

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function git(args) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8" });
}

function listFiles(args) {
  return git(args)
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

const staged = listFiles(["diff", "--cached", "--name-only", "--diff-filter=ACMR"]);
if (staged.length === 0) {
  console.log("pre-commit: 没有暂存改动，跳过本地检查。");
  process.exit(0);
}

const unstaged = new Set(listFiles(["diff", "--name-only"]));
const partiallyStaged = staged.filter((file) => unstaged.has(file));
if (partiallyStaged.length > 0) {
  console.warn(
    `pre-commit: 警告——下列文件既有暂存改动又有未暂存改动，检查的是工作区版本而不是提交快照：\n  ${partiallyStaged.join("\n  ")}`,
  );
}

if (!existsSync(join(root, "node_modules", "ajv"))) {
  console.error(
    "pre-commit: 合同门禁的依赖未安装，无法执行 `npm run check`。请先运行 `npm ci`（Node ≥ 22.12）。",
  );
  process.exit(1);
}

const rustTouched = staged.some(
  (file) =>
    file.endsWith(".rs") ||
    file === "Cargo.toml" ||
    file === "Cargo.lock" ||
    file === "rust-toolchain.toml" ||
    file.endsWith("/Cargo.toml"),
);

const npm = process.platform === "win32" ? "npm.cmd" : "npm";

// Windows 上 `.cmd` 不能直接 spawn（Node 对 .cmd/.bat 的保护会抛 EINVAL），必须经 shell；
// 因此那里传整条命令行字符串而不是 (command, args)（传 args + shell 会触发 DEP0190）。
// 参数全部来自本文件的常量，不含空格与元字符，拼接不引入注入面。
function run(command, args) {
  if (process.platform === "win32") {
    return spawnSync([command, ...args].join(" "), { cwd: root, stdio: "inherit", shell: true });
  }
  return spawnSync(command, args, { cwd: root, stdio: "inherit" });
}

const steps = [];
if (rustTouched) {
  steps.push(["cargo", ["fmt", "--all", "--", "--check"]]);
}
steps.push([npm, ["run", "check"]]);
if (rustTouched) {
  // 与 CI 的静态检查参数逐字一致（`AGENTS.md` §8）；工具链版本由 `rust-toolchain.toml` 固定。
  steps.push(["cargo", ["clippy", "--locked", "--workspace", "--all-targets", "--all-features", "--", "-D", "warnings"]]);
}

if (rustTouched) {
  console.log(
    "pre-commit: 本次提交涉及 Rust，将运行 fmt / 合同门禁 / clippy。首次运行（或工具链刚变更时）需要完整编译，可能持续数分钟。",
  );
}

for (const [index, [command, args]] of steps.entries()) {
  const label = `${index + 1}/${steps.length} ${command} ${args.join(" ")}`;
  console.log(`pre-commit: [${label}]`);
  const result = run(command, args);
  if (result.error) {
    console.error(`pre-commit: 无法执行 ${command}：${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) {
    console.error(
      `pre-commit: [${label}] 失败（退出码 ${result.status}）。修正后重新 \`git add\`；` +
        "确认要跳过时用 `git commit --no-verify`，但 CI 会再判一次。",
    );
    process.exit(result.status ?? 1);
  }
}

console.log(`pre-commit: 通过（${steps.length} 步）。`);
