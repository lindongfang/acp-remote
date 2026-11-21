import assert from "node:assert/strict";

export async function pairClient(page, origin, code, displayName) {
  await page.goto(origin, { waitUntil: "load", timeout: 30_000 });
  await page.locator("#pairing-code").waitFor({ state: "visible", timeout: 20_000 });
  await page.locator("#pairing-code").fill(code);
  await page.locator("#display-name").fill(displayName);
  await page.locator('button:has-text("认领这台浏览器")').click();
  await page.locator("#session-target-id").waitFor({ state: "visible", timeout: 30_000 });
}

export async function openNewTarget(pageA, cfg) {
  await pageA.locator("#catalog-workspace-path").fill(cfg.workspaceDir);
  await pageA.locator('button:has-text("登记工作目录")').click();
  await pageA.locator('button:has-text("填入 Target / Source 表单")').waitFor({ state: "visible", timeout: 20_000 });
  await pageA.locator('button:has-text("填入 Target / Source 表单")').click();

  await pageA.locator("#catalog-harness-name").fill(cfg.realCodexMode ? "Real Codex ACP V2" : "Controlled ACP V2");
  await pageA.locator("#catalog-source-name").fill("E2E Source");
  await pageA.locator("#catalog-launch-executable").fill(process.execPath);
  await pageA.locator("#catalog-launch-args").fill(
    cfg.realCodexMode ? cfg.codexAdapter : `${cfg.peerScript}\n--state-dir\n${cfg.peerStateDir}`,
  );
  if (cfg.realCodexMode) {
    await pageA.locator("#catalog-launch-env").fill(
      `CODEX_HOME=${cfg.codexHome}\nCODEX_PATH=${cfg.codexPath}\nNO_BROWSER=1\nINITIAL_AGENT_MODE=read-only`,
    );
  }
  await pageA.locator("#catalog-recipe-wire").selectOption("2");
  await pageA.locator('button:has-text("上传并安装 Harness")').click();
  await pageA.locator('button:has-text("填入 Harness/Profile")').first().waitFor({ state: "visible", timeout: 20_000 });
  await pageA.locator('button:has-text("填入 Harness/Profile")').first().click();

  return createBoundTarget(pageA,cfg);
}

export async function createBoundTarget(pageA,cfg) {
  const operation = pageA.locator('.detail-block:has(.detail-label:text-is("来源操作")) code');
  const previous = await operation.count() ? await operation.textContent() : null;
  await pageA.locator('section[data-panel="source-session"] button:has-text("登记新建")').click();
  if (previous !== null) await operation.filter({hasNotText:previous}).waitFor();
  await pageA.locator('section[data-panel="source-session"] button:has-text("执行操作")').waitFor({ state: "visible", timeout: 20_000 });
  await pageA.locator('section[data-panel="source-session"] button:has-text("执行操作")').click();
  await pageA.locator('section[data-panel="source-session"] button:has-text("读取结果")').waitFor({ state: "visible", timeout: 35_000 });
  await pageA.locator('section[data-panel="source-session"] button:has-text("读取结果")').click();

  const createdNativeEl = pageA.locator('.detail-block:has(.detail-label:text-is("已创建原生会话 ID")) code');
  await createdNativeEl.waitFor({ state: "visible", timeout: 20_000 });
  const nativeSessionId = (await createdNativeEl.textContent())?.trim() ?? "";
  if (cfg.realCodexMode) {
    assert(nativeSessionId.length > 0, `empty nativeSessionId for real codex`);
  } else {
    assert(nativeSessionId.startsWith("e2e-acp-session-"), `unexpected nativeSessionId: ${nativeSessionId}`);
  }

  await pageA.locator("#catalog-target-native-id").fill(nativeSessionId);
  await pageA.locator('button:has-text("登记并绑定 Target")').click();
  const targetCard = pageA.locator(".device-card", { hasText: nativeSessionId });
  await targetCard.waitFor({ state: "visible", timeout: 20_000 });
  const targetId = (await targetCard.locator('.detail-block:has(.detail-label:text-is("Target ID")) code').textContent())?.trim() ?? "";
  assert(targetId.length === 22, `unexpected targetId: ${targetId}`);
  const priorOpen = await pageA.getByRole("button",{name:"打开共享视图",exact:true}).first().elementHandles();
  await targetCard.locator('button:has-text("载入队列")').click();
  if (priorOpen.length) await pageA.waitForFunction(el=>!el.isConnected,priorOpen[0]);
  await pageA.waitForFunction(id=>document.querySelector("#session-target-id").value === id,targetId);
  await pageA.locator('button:has-text("打开共享视图")').first().click();

  return { targetId, nativeSessionId };
}
