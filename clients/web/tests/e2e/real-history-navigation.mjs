import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { writeFileSync, existsSync } from "node:fs";
import path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

export async function runRealHistory({page,cfg,report,openNewTarget,groups,button,wire,openBound}) {
  const { targetId, nativeSessionId } = await openNewTarget(page,cfg);
  const root = page.locator('section[aria-label="会话历史"]:not([data-stale="true"])').last();
  const latest = () => wire.findLast(event => event.page)?.page;
  const signal = async (name) => {
    writeFileSync(path.join(cfg.workDir,name),"");
    const ack = path.join(cfg.workDir,name + ".done");
    for (let i=0; i<600 && !existsSync(ack); i++) await sleep(100);
    assert.ok(existsSync(ack), "node lifecycle acknowledgement");
  };
  async function release() {
    const reset = button(page,"登记另一目标");
    if (await reset.isVisible()) await reset.click();
    await page.locator("#native-release-target").fill(targetId);
    await button(page,"登记释放").click();
    await button(page,"执行原生释放").click();
    await button(page,"重试释放协调").waitFor();
  }
  report.stage = "twelve real prompts";
  writeFileSync(path.join(cfg.workspaceDir,"marker.txt"),"HN_FILE_913");
  for (let turn=1;turn<=12;turn++) {
    // Leave room in the node's 120-command minute for uploads, queue reads and stream credits.
    const recent = wire.filter(event => event.sentAt > Date.now() - 60000);
    if (recent.length >= 80) await sleep(recent[recent.length - 80].sentAt + 60500 - Date.now());
    report.stage = "real prompt " + turn;
    const marker = turn === 1 ? "HN_FILE_913" : `HN_RESULT_${turn}`;
    const prompt = turn === 1 ? "Run exactly Get-Content -LiteralPath marker.txt in PowerShell and reply only with the file contents." :
      `Concatenate HN, RESULT, and ${turn} with underscores and reply only with the resulting token.`;
    await page.locator("#session-input-body").fill(prompt);
    await button(page,"上传并提交").click();
    if (turn === 1) {
      const read = button(page,"读取此审批");
      const deadline = Date.now() + 120000;
      while (!await root.getByText(marker,{exact:true}).first().isVisible() && Date.now() < deadline) {
        await button(page,"刷新审批列表").click();
        if (await read.isVisible()) {
          await read.click();
          const raw = await page.getByLabel("审批原始载荷").innerText();
          const request = JSON.parse(raw);
          const params = request.params ?? request;
          const input = params.subject?.toolCall?.rawInput;
          const expected = "Get-Content -LiteralPath marker.txt";
          const cli = input?.command ?? "";
          const shell = cli.match(/^(?:"?[^"\r\n]*[\\/])?(?:pwsh|powershell)\.exe"?\s+-Command\s+(.+)$/i);
          assert.ok([expected, `"${expected}"`, `'${expected}'`].includes(shell?.[1] ?? cli)
            && !input.url && !input.additionalPermissions, "expected marker read approval");
          await page.getByRole("button",{name:/allow_once/}).click();
          await button(page,"提交已登记决定").click();
          break;
        }
        await sleep(1500);
      }
    }
    try {
      await root.getByText(marker,{exact:true}).first().waitFor({timeout:120000});
    } catch (error) {
      const body = await root.innerText();
      report.visibleResultTurns = [...body.matchAll(/HN_RESULT_(\d+)\b/g)].map(m => Number(m[1]));
      report.historyReadFailed = body.includes("读取或展示准备失败");
      throw error;
    }
    report.completedTurns = turn;
    if (turn === 1) {
      await root.locator('[data-state="已完成"]').first().waitFor();
      report.completedTool = true;
    }
  }

  report.stage = "latest five after twelve replies";
  if (!await root.getByLabel("每页轮数").isVisible()) {
    await button(page,"打开共享视图").first().click();
  }
  await root.getByLabel("每页轮数").fill("5");
  await button(root,"重新加载首页").click();
  await groups(root,8,12);
  assert.equal(latest().turnStatus,"provable");
  const { firstTurn, lastTurn, totalTurns, turnStatus, source } = latest();
  report.latestPage = { firstTurn, lastTurn, totalTurns, turnStatus, source };
  report.steps.push("12 real turns, tool call, latest 8–12 provable");
  report.stage = "restart and release";
  await release();
  await signal("restart");
  await page.reload();
  await openBound(page,targetId);
  await root.locator('[data-sync-state="synced"]').first().waitFor({state:"attached"});
  await root.getByLabel("每页轮数").fill("5");
  await button(root,"重新加载首页").click();
  await groups(root,8,12);
  await root.locator('[data-sync-state="synced"]').first().waitFor({state:"attached"});
  await release();
  report.steps.push("reopened 8–12 after restart and released after native sync");
  const generation = latest().generationId;
  const inventory = () => JSON.parse(execFileSync("powershell.exe",["-NoProfile","-Command",
    "ConvertTo-Json -Compress -InputObject @(Get-Process -Name codex -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)"],
    {encoding:"utf8", windowsHide:true}));
  const initial = new Set(inventory());
  await signal("before-pages");
  for (const [first,last] of [[3,7],[1,2]]) {
    await button(root,"更早").click();
    await groups(root,first,last);
    assert.ok(inventory().every(pid => initial.has(pid)));
  }
  await signal("after-pages");
  assert.ok(await button(root,"更早").isDisabled());
  report.steps.push("offline 3–7,1–2 with process checks");
  report.stage = "external Codex append";
  const deadline = Date.now() + 30000;
  for (;;) {
    const append = spawnSync(cfg.codexPath, ["exec", "resume", "--skip-git-repo-check", nativeSessionId,
      "Concatenate HN and EXTERNAL with an underscore and reply with the result."], {
      cwd: cfg.workspaceDir, env: {...process.env, CODEX_HOME: cfg.codexHome},
      stdio: ["ignore", "ignore", "pipe"], encoding: "utf8", windowsHide: true, timeout: 150000,
    });
    report.externalExit = append.status;
    if (append.status === 0) break;
    // This rejection precedes input delivery. Retrying other failures could duplicate turn 13.
    const writerBusy = /thread\/resume failed: thread [\da-f-]+ already has an active writer/i.test(append.stderr ?? "");
    report.externalFailure = writerBusy ? "active_writer" : "external_exec_failed";
    if (!writerBusy || Date.now() >= deadline) throw new Error("external Codex resume failed");
    report.writerBusyRetries = (report.writerBusyRetries ?? 0) + 1;
    await sleep(2000);
  }
  delete report.externalFailure;
  report.steps.push("external Codex resume exited successfully");
  await root.getByLabel("每页轮数").fill("5");
  await button(root,"重新加载首页").click();
  await groups(root,9,13);
  await root.getByText("HN_EXTERNAL",{exact:true}).first().waitFor();
  const fresh = latest();
  assert.notEqual(fresh.generationId,generation);
  await button(root,"更早").click();
  await groups(root,4,8);
  assert.equal(latest().generationId,fresh.generationId);
  assert.equal(wire.findLast(e=>e.action==="history.page").mode.cursor,fresh.olderCursor);
  report.steps.push("external turn 13, new generation, continuation with fresh cursor");
}
