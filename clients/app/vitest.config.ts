import { defineConfig } from "vitest/config";

/**
 * `clients/app` 的测试入口（PV3 / AC3）。
 *
 * 契约测试直接读仓库根 `fixtures/sync/v1/manifest.json` 与 `schemas/sync/v1/**`——见
 * `src/protocol/contract.test.ts` 与 `src/protocol/paths.ts`。测试不复制样例，也不改仓库根资产。
 */
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    // 契约资产在仓库根、被测代码在 clients/app：两者都不在默认的 Vite root 之内，
    // 这里显式固定 root，避免 vitest 因工作目录不同而解析到另一份文件。
    root: __dirname,
  },
  resolve: {
    alias: {
      "@": new URL("./src", import.meta.url).pathname,
    },
  },
});
