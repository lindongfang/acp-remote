import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { writeFileSync, existsSync } from "node:fs";
import path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

export async function runRealHistory({page,cfg,report,openNewTarget,groups,history,button,wire,openBound}) {
  const { targetId, nativeSessionId } = await openNewTarget(page,cfg);
  const root = history(page);
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
    const marker = turn === 1 ? "HN_FILE_913" : `HN_RESULT_${turn}`;
    const prompt = turn === 1 ? "Use the shell to read marker.txt and reply only with its contents." :
      `Concatenate HN, RESULT, and ${turn} with underscores and reply only with the resulting token.`;
    await page.locator("#session-input-body").fill(prompt);
    await button(page,"上传并提交").click();
    await root.getByText(marker,{exact:true}).first().waitFor({timeout:120000});
  }
  await root.getByText("工具调用",{exact:true}).first().waitFor();
  await root.getByLabel("每页轮数").fill("5");
  await button(root,"重新加载首页").click();
  await groups(root,8,12);
  assert.equal(latest().turnStatus,"provable");
  report.steps.push("12 real turns, tool call, latest 8–12 provable");
  report.stage = "restart and release";
  await release();
  await signal("restart");
  await page.reload();
  await openBound(page,targetId);
  await root.getByLabel("每页轮数").fill("5");
  await button(root,"重新加载首页").click();
  await groups(root,8,12);
  await release();
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
  execFileSync(cfg.codexPath,["exec","resume","--skip-git-repo-check",nativeSessionId,
    "Concatenate HN and EXTERNAL with an underscore and reply with the result."],{
    cwd:cfg.workspaceDir,env:{...process.env,CODEX_HOME:cfg.codexHome},
    stdio:"ignore",windowsHide:true,timeout:150000,
  });
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
