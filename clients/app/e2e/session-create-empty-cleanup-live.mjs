#!/usr/bin/env node
/**
 * Real Chromium/Edge acceptance for direct session creation and empty cleanup.
 *
 * This process controls only the browser and exact clock file. The Rust test
 * owns the Daemon, controlled ACP Agent, SQLite inspection, and restart.
 */

import { spawn } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

class ConfigError extends Error {}
class AssertionError extends Error {}

function required(name) {
  const value = process.env[name];
  if (!value || !value.trim()) throw new ConfigError(`missing required environment variable ${name}`);
  return value.trim();
}

let config;
try {
  config = {
    origin: required('SESSION_CREATE_ORIGIN'),
    code: required('SESSION_CREATE_CODE'),
    workspaceRoot: required('SESSION_CREATE_WORKSPACE_ROOT'),
    evidence: required('SESSION_CREATE_EVIDENCE'),
    agentEvidence: required('SESSION_CREATE_AGENT_EVIDENCE'),
    controlDir: required('SESSION_CREATE_CONTROL_DIR'),
    clockFile: required('SESSION_CREATE_CLOCK_FILE'),
    baseClockMs: Number(required('SESSION_CREATE_BASE_CLOCK_MS')),
    restartClockMs: Number(required('SESSION_CREATE_RESTART_CLOCK_MS')),
  };
  if (!Number.isSafeInteger(config.baseClockMs) || !Number.isSafeInteger(config.restartClockMs)) {
    throw new ConfigError('clock environment variables must be safe integers');
  }
} catch (error) {
  process.stderr.write(`session-create-empty-cleanup-live: ${error.message}\n`);
  process.exit(2);
}

const RETENTION_MS = 30 * 60 * 1_000;
const executable = [
  process.env.BROWSER_PATH,
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  '/usr/bin/chromium',
  '/usr/bin/google-chrome',
].find((path) => path && existsSync(path));
if (!executable) {
  process.stderr.write('session-create-empty-cleanup-live: Chromium/Edge not found; set BROWSER_PATH\n');
  process.exit(2);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const debuggingPort = 10000 + Math.floor(Math.random() * 20000);
const userDataDir = mkdtempSync(join(tmpdir(), 'session-create-cleanup-browser-'));
const browserStderr = [];
const browser = spawn(
  executable,
  [
    '--headless=new',
    '--disable-gpu',
    '--ignore-certificate-errors',
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-extensions',
    '--window-size=1400,900',
    '--remote-debugging-port=' + debuggingPort,
    '--user-data-dir=' + userDataDir,
    'about:blank',
  ],
  { stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true },
);
browser.stderr.on('data', (chunk) => {
  browserStderr.push(String(chunk));
  if (browserStderr.length > 200) browserStderr.shift();
});

const report = {
  tool: 'session-create-empty-cleanup-live',
  version: 1,
  startedAt: new Date().toISOString(),
  finishedAt: null,
  origin: config.origin,
  workspaceRoot: config.workspaceRoot,
  browser: executable,
  browserVersion: null,
  platform: process.platform,
  nodeVersion: process.version,
  clock: {
    baseClockMs: config.baseClockMs,
    restartClockMs: config.restartClockMs,
    retentionMs: RETENTION_MS,
  },
  scenarios: [],
  skipped: [],
  pageErrors: [],
  browserDialogs: [],
  exitCode: null,
};

let browserCdp = null;
let page = null;
let controlSequence = 0;
let observationSequence = 0;
let browserTermination = null;

try {
  const browserSocket = await waitForBrowserEndpoint(debuggingPort);
  browserCdp = await connectCdp(browserSocket, (message) => {
    if (message.method === 'Page.javascriptDialogOpening') {
      report.browserDialogs.push({
        at: Date.now(),
        type: message.params?.type ?? null,
        message: message.params?.message ?? null,
      });
    }
  });
  const version = await browserCdp.call('Browser.getVersion');
  report.browserVersion = version.product;
  page = await openPage(
    browserCdp,
    config.origin + '/pair',
    config.code
      .trim()
      .toUpperCase()
      .replace(/^(.{4})-(.{4})$/, '$1$2')
      .replaceAll('I', '1')
      .replaceAll('L', '1')
      .replaceAll('O', '0'),
  );
  await page.evaluate(pairFlow, { code: config.code });

  const sessionA = await runScenario('direct-create-is-persisted-and-active', async () => {
    const created = await page.evaluate(createSessionFlow, {
      workspace: workspacePath('direct'),
    });
    const log = await page.readLog();
    const createCommands = commandFrames(log, 'session.create');
    assert(createCommands.length === 1, `expected exactly one session.create, got ${createCommands.length}`);
    const create = createCommands[0];
    assert(
      JSON.stringify(Object.keys(create.payload).sort()) ===
        JSON.stringify(['agentId', 'workspacePath']),
      `create payload must contain only agentId and workspacePath: ${JSON.stringify(create.payload)}`,
    );
    assert(create.requestId === created.requestId, 'UI navigation result must use the original create requestId');

    const terminal = inboundFrames(log).find(
      (message) =>
        message.type === 'command.result' &&
        message.body?.requestId === create.requestId &&
        message.body?.command === 'session.create' &&
        message.body?.status === 'completed',
    );
    assert(terminal, 'session.create must produce a completed command.result');
    assert(
      terminal.body.result?.sessionId === created.sessionId,
      `completed result and route must share the formal session ID: ${JSON.stringify(terminal.body)}`,
    );

    const snapshotActive = inboundFrames(log).some(
      (message) =>
        message.type === 'sync.snapshot_chunk' &&
        message.body?.resource === 'sessions' &&
        message.body.items?.some(
          (item) => item.sessionId === created.sessionId && item.localActivity === 'active',
        ),
    );
    assert(snapshotActive, 'the post-create snapshot must contain localActivity=active');

    const entries = wireEntries(log);
    const completeAt = entries.findIndex(
      (entry) =>
        entry.direction === 'inbound' &&
        entry.message.type === 'command.result' &&
        entry.message.body?.requestId === create.requestId &&
        entry.message.body?.status === 'completed',
    );
    const viewAt = entries.findIndex(
      (entry) =>
        entry.direction === 'outbound' &&
        entry.message.type === 'session.view' &&
        entry.message.body?.sessionId === created.sessionId,
    );
    const resumeAt = entries.findIndex(
      (entry) =>
        entry.direction === 'outbound' &&
        entry.message.type === 'command' &&
        entry.message.body?.command === 'session.resume' &&
        entry.message.body?.sessionId === created.sessionId,
    );
    assert(completeAt >= 0 && completeAt < viewAt && viewAt < resumeAt,
      `create completion must precede view and resume (${completeAt}, ${viewAt}, ${resumeAt})`);
    assert(
      JSON.stringify(commandFrames(log, 'session.resume')[0]?.payload) === '{}',
      'session.resume payload must be an empty object',
    );

    const state = await page.evaluate(domSnapshot);
    assert(state.routeSessionId === created.sessionId, `route must use the returned ID: ${JSON.stringify(state)}`);
    assert(state.activity === 'active', `created session must render active immediately: ${JSON.stringify(state)}`);
    assert(state.composer === 'editable', `created session composer must be editable: ${JSON.stringify(state)}`);
    const trail = activityTrail(log, created.sessionId);
    assert(!trail.includes('idle'), `created session must not flash idle: ${JSON.stringify(trail)}`);

    const database = await control('inspect_created', { sessionId: created.sessionId });
    assert(database.session.live, 'formal session must exist in authoritative storage before any prompt');
    assert(database.session.directoryAssigned, 'formal session must be assigned to its directory before any prompt');
    assert(database.session.messages === 0, 'direct-create observation must precede any message');
    await page.evaluate(goHomeFlow);
    assert(await page.evaluate(hasSessionRow, { sessionId: created.sessionId }),
      'created session must be visible in the directory and session list before any message');
    return created.sessionId;
  });

  const sessionB = await runScenario('switch-keeps-latest-session', async () => {
    const knownAResumeIds = new Set(
      commandFrames(await page.readLog(), 'session.resume')
        .filter((frame) => frame.sessionId === sessionA)
        .map((frame) => frame.requestId),
    );
    const created = await page.evaluate(createSessionFlow, {
      workspace: workspacePath('switch'),
    });
    await page.evaluate(openSessionFlow, { sessionId: sessionA });
    const delayedARequestId = await waitFor(async () => {
      const log = await page.readLog();
      return commandFrames(log, 'session.resume').find(
        (frame) => frame.sessionId === sessionA && !knownAResumeIds.has(frame.requestId),
      )?.requestId ?? null;
    }, 'A session.resume is dispatched while the first delayed response is still allowed to arrive');
    await page.evaluate(openSessionFlow, { sessionId: created.sessionId });

    await waitFor(async () => {
      const log = await page.readLog();
      const terminal = inboundFrames(log).find(
        (message) =>
          message.type === 'command.result' &&
          message.body?.command === 'session.resume' &&
          message.body?.requestId === delayedARequestId &&
          message.body?.status === 'completed',
      );
      if (terminal !== undefined) {
        assert(terminal.body.result?.sessionId === sessionA,
          `A resume result must still identify A: ${JSON.stringify(terminal.body)}`);
      }
      return terminal !== undefined;
    }, 'delayed A resume result arrives', 20_000);
    await page.evaluate(settleAnimationFramesFlow);

    const log = await page.readLog();
    const views = viewFrames(log);
    const lastB = views.map((view) => view.sessionId).lastIndexOf(created.sessionId);
    assert(lastB >= 0, 'B must have a viewer declaration');
    assert(
      views.slice(lastB).every((view) => view.sessionId === created.sessionId),
      `A must not regain the viewer after B: ${JSON.stringify(views.slice(lastB))}`,
    );
    const state = await page.evaluate(domSnapshot);
    assert(state.routeSessionId === created.sessionId,
      `delayed A response must not move the route: ${JSON.stringify(state)}`);
    return created.sessionId;
  });

  await runScenario('restart-grants-a-fresh-window', async () => {
    const seededBoundary = await page.evaluate(createSessionFlow, {
      workspace: workspacePath('restart-seeded-boundary'),
    });
    const activationSession = await page.evaluate(createSessionFlow, {
      workspace: workspacePath('restart-activation'),
    });
    const seededAgentSessionId = await agentSessionIdForWorkspace('restart-seeded-boundary');
    const activationAgentSessionId = await agentSessionIdForWorkspace('restart-activation');
    writeClock(config.restartClockMs - 1_000);
    await page.evaluate(goHomeFlow);
    const restarted = await control('restart');
    assert(restarted.restartClockMs === config.restartClockMs, 'supervisor must restart at the requested file clock');
    await page.call('Page.navigate', { url: config.origin + '/pair' });
    await page.evaluate(pairFlow, { code: restarted.pairingCode });
    await page.call('Page.navigate', { url: config.origin + '/' });
    await waitFor(async () => {
      const state = await page.evaluate(domSnapshot);
      return state.directoryRoute;
    }, 'new Daemon page reaches the authoritative directory after restart', 30_000);
    await waitFor(async () => {
      const listed = await page.evaluate(listedSessionIds);
      return listed.includes(seededBoundary.sessionId) && listed.includes(activationSession.sessionId);
    }, 'both pre-restart empty sessions remain listed after Daemon restart', 30_000);
    for (const sessionId of [seededBoundary.sessionId, activationSession.sessionId]) {
      const before = await control('inspect_created', { sessionId });
      assert(before.session.live, `restarted Daemon must retain pre-restart empty session ${sessionId}`);
    }

    const freshBoundary = await page.evaluate(createSessionFlow, {
      workspace: workspacePath('restart-fresh-boundary'),
    });
    const freshAgentSessionId = await agentSessionIdForWorkspace('restart-fresh-boundary');
    await page.evaluate(goHomeFlow);
    await releaseViewerAndSettle(freshBoundary.sessionId);
    writeClock(config.restartClockMs + RETENTION_MS - 1);

    const listedBeforeActivation = await page.evaluate(listedSessionIds);
    for (const sessionId of [
      seededBoundary.sessionId,
      activationSession.sessionId,
      freshBoundary.sessionId,
    ]) {
      assert(listedBeforeActivation.includes(sessionId),
        `restart-seeded or restarted session ${sessionId} must remain listed at 29:59.999`);
    }
    const seededBeforeBoundary = await control('inspect_created', {
      sessionId: seededBoundary.sessionId,
    });
    assert(seededBeforeBoundary.session.live,
      'startup-seeded empty session must remain authoritatively live at 29:59.999');
    const resumesBeforeBoundary = (await readAgentEvidence()).filter(
      (record) =>
        record.type === 'session_resume' &&
        record.sessionId === seededAgentSessionId,
    );
    assert(resumesBeforeBoundary.length === 0,
      `startup-seeded boundary session must not be activated before its due time: ` +
        JSON.stringify(resumesBeforeBoundary));

    const activationResumeBaseline = await resumeObservationBaseline(
      activationSession.sessionId,
      activationAgentSessionId,
    );
    const activated = await page.evaluate(openSessionFlow, {
      sessionId: activationSession.sessionId,
    });
    assert(activated.routeSessionId === activationSession.sessionId,
      'the second pre-restart session remains usable one millisecond before the seeded window ends');
    const activationResume = await waitForResumeCompleted(
      activationSession.sessionId,
      activationAgentSessionId,
      activationResumeBaseline,
    );
    await page.evaluate(goHomeFlow);
    const activationRelease = await releaseViewerAndSettle(activationSession.sessionId);
    assert(activationRelease.lifecycle.retentionStart === new Date(config.restartClockMs + RETENTION_MS - 1).toISOString(),
      `activation retention must start before clock advance: ${JSON.stringify(activationRelease)}`);

    writeClock(config.restartClockMs + RETENTION_MS);
    const seededDeletedAt = await waitForDeletedEvent(seededBoundary.sessionId, 30_000);
    const freshDeletedAt = await waitForDeletedEvent(freshBoundary.sessionId, 30_000);
    const seededDeleted = await control('inspect_deleted', { sessionId: seededBoundary.sessionId });
    assert(!seededDeleted.session.live && seededDeleted.session.deletedAt !== null,
      'startup-seeded empty session must leave an authoritative tombstone at its new boundary');
    const freshDeleted = await control('inspect_deleted', { sessionId: freshBoundary.sessionId });
    assert(!freshDeleted.session.live && freshDeleted.session.deletedAt !== null,
      'post-restart comparison session must leave an authoritative tombstone at the same boundary');
    await waitFor(async () => {
      const listed = await page.evaluate(listedSessionIds);
      return !listed.includes(seededBoundary.sessionId) && !listed.includes(freshBoundary.sessionId);
    }, 'deleted startup-seeded and post-restart sessions disappear from product reads', 30_000);
    const activationStillLive = await control('inspect_created', {
      sessionId: activationSession.sessionId,
    });
    assert(activationStillLive.session.live,
      'activation before the seeded boundary must start a fresh complete retention window');

    const agentEvidence = await readAgentEvidence();
    const seededClose = agentEvidence.find(
      (record) => record.type === 'session_close' && record.sessionId === seededAgentSessionId,
    );
    assert(!seededClose,
      'the clean Daemon restart already terminated the managed Agent; startup cleanup must not forge session/close');
    const freshClose = agentEvidence.find(
      (record) => record.type === 'session_close' && record.sessionId === freshAgentSessionId,
    );
    assert(freshClose, 'Agent must observe session/close before restart-boundary deletion');
    assert(freshDeletedAt >= freshClose.at,
      'session.deleted must become visible after session/close');

    const activatedWindowStartedAt = config.restartClockMs + RETENTION_MS - 1;
    const activatedWindowEndsAt = activatedWindowStartedAt + RETENTION_MS;
    writeClock(activatedWindowEndsAt - 1);
    await waitFor(async () => {
      const listed = await page.evaluate(listedSessionIds);
      return listed.includes(activationSession.sessionId);
    }, 'activated empty session remains listed one millisecond before its reset window ends', 30_000);
    const activationBeforeBoundary = await control('inspect_created', {
      sessionId: activationSession.sessionId,
    });
    assert(activationBeforeBoundary.session.live,
      'activated empty session must remain live one millisecond before its reset window ends');

    writeClock(activatedWindowEndsAt);
    const activatedDeletedAt = await waitForDeletedEvent(activationSession.sessionId, 30_000);
    const activatedDeleted = await control('inspect_deleted', {
      sessionId: activationSession.sessionId,
    });
    assert(!activatedDeleted.session.live && activatedDeleted.session.deletedAt !== null,
      'activated empty session must leave a tombstone only after its fresh complete window');
    const activationClose = (await readAgentEvidence()).find(
      (record) =>
        record.type === 'session_close' &&
        record.sessionId === activationAgentSessionId,
    );
    assert(activationClose,
      'the activated empty session must record Agent session/close before deletion');
    assert(activatedDeletedAt >= activationClose.at,
      'activated session/close must precede its session.deleted event');

    return {
      startupSeededSessionId: seededBoundary.sessionId,
      startupSeededWasNotActivated: true,
      startupSeededCloseObserved: false,
      startupSeededDeletedAt: seededDeletedAt,
      freshBoundarySessionId: freshBoundary.sessionId,
      freshBoundaryCloseAt: freshClose.at,
      freshBoundaryDeletedAt: freshDeletedAt,
      activationSessionId: activationSession.sessionId,
      activationResumeRequestIds: activationResume.requestIds,
      activationResumeCompletedAt: activationResume.completedAt,
      activationResumeEvidence: activationResume,
      activationRelease,
      activatedWindowStartedAt,
      activatedWindowEndsAt,
      activatedCloseAt: activationClose.at,
      activatedDeletedAt,
      seededWindowEndsAt: config.restartClockMs + RETENTION_MS,
      activatedSessionStillLiveAtBoundary: true,
    };
  });

  const exact = await runScenario('exact-boundary-cleanup-and-retention', async () => {
    const exactClockMs = config.restartClockMs + 3 * RETENTION_MS;
    await page.evaluate(showViewerFlow);
    writeClock(exactClockMs);
    await page.call('Page.navigate', { url: config.origin + '/' });
    await waitFor(async () => (await page.evaluate(domSnapshot)).directoryRoute, 'directory after exact-clock reset');

    const messaged = await page.evaluate(createSessionFlow, { workspace: workspacePath('messaged') });
    const messagedMarker = await page.evaluate(sendPromptFlow, {
      text: 'Reply with exactly this token and nothing else: exact-boundary-retained',
      sessionId: messaged.sessionId,
    });
    const later = await page.evaluate(createSessionFlow, { workspace: workspacePath('direct') });
    await page.evaluate(sendPromptFlow, {
      text: 'Reply with exactly this token and nothing else: later-message-retained',
      sessionId: later.sessionId,
    });
    const usable = await page.evaluate(createSessionFlow, { workspace: workspacePath('usable') });
    const boundary = await page.evaluate(createSessionFlow, { workspace: workspacePath('boundary') });
    const boundaryAgentSessionId = await agentSessionIdForWorkspace('boundary');

    await page.evaluate(goHomeFlow);
    await releaseViewerAndSettle(boundary.sessionId);
    writeClock(exactClockMs + RETENTION_MS - 1);
    const listed = await page.evaluate(listedSessionIds);
    for (const id of [messaged.sessionId, later.sessionId, usable.sessionId, boundary.sessionId]) {
      assert(listed.includes(id), `session ${id} must remain visible at 29:59.999`);
    }
    // Stay hidden while changing the route. Once the route belongs to the
    // usable session, restore visibility so the visibility reaction activates
    // the new target instead of the previously mounted boundary session.
    await page.evaluate(openSessionFlow, {
      sessionId: usable.sessionId,
      requireActive: false,
    });
    await page.evaluate(showViewerFlow);
    await page.evaluate(settleAnimationFramesFlow);
    await waitFor(async () => (await page.evaluate(domSnapshot)).activity === 'active',
      'usable empty session activates at 29:59.999');
    await page.evaluate(sendPromptFlow, {
      text: 'Reply with exactly this token and nothing else: usable-before-boundary',
      sessionId: usable.sessionId,
    });

    await page.evaluate(goHomeFlow);
    await releaseViewerAndSettle(usable.sessionId);
    const notificationBaseline = await page.readLog();
    const baselineNotifications = notificationCount(notificationBaseline);
    writeClock(exactClockMs + RETENTION_MS);
    const deletedAt = await waitForDeletedEvent(boundary.sessionId, 30_000);
    await waitFor(async () => {
      const state = await page.evaluate(domSnapshot);
      return state.directoryRoute;
    }, 'UI remains silently on the directory during background deletion', 20_000);
    await page.evaluate(settleAnimationFramesFlow);

    const finalLog = await page.readLog();
    assert(notificationCount(finalLog) === baselineNotifications,
      `deletion must not create a toast/dialog: ${JSON.stringify(finalLog.notifications.slice(baselineNotifications))}`);
    assert(report.browserDialogs.length === 0,
      `native browser dialogs are forbidden: ${JSON.stringify(report.browserDialogs)}`);
    const evidence = await readAgentEvidence();
    const close = evidence.find(
      (record) => record.type === 'session_close' && record.sessionId === boundaryAgentSessionId,
    );
    assert(close, 'controlled Agent must observe session/close for the boundary session');
    assert(deletedAt >= close.at, 'session.deleted must be persisted only after session/close is observed');
    const deleted = await control('inspect_deleted', { sessionId: boundary.sessionId });
    assert(!deleted.session.live && deleted.session.deletedAt !== null,
      'boundary cleanup must leave a tombstone invisible to product reads');

    for (const id of [messaged.sessionId, later.sessionId, usable.sessionId]) {
      const retained = await control('inspect_created', { sessionId: id });
      assert(retained.session.live, `nonempty session ${id} must survive retention`);
      assert(retained.session.messages > 0 || retained.session.turns > 0,
        `retained session ${id} must still have content`);
    }
    const shown = await page.evaluate(listedSessionIds);
    const retainedSessionIds = [messaged.sessionId, later.sessionId, usable.sessionId];
    for (const id of retainedSessionIds) {
      assert(shown.includes(id), `nonempty session ${id} must remain in directory/list after release`);
    }
    const retainedActivityClockMs = exactClockMs + 2 * RETENTION_MS;
    writeClock(retainedActivityClockMs);
    await waitFor(async () => {
      const log = await page.readLog();
      return retainedSessionIds.every(
        (id) => localActivityProjectionAt(log, id) === 'idle',
      );
    }, 'nonempty sessions project localActivity=idle on the Sync wire', 30_000);
    const retainedActivityLog = await page.readLog();
    const retainedActivities = Object.fromEntries(
      retainedSessionIds.map((id) => [id, localActivityProjectionAt(retainedActivityLog, id)]),
    );
    await page.evaluate(openSessionFlow, {
      sessionId: messaged.sessionId,
      requireActive: false,
    });
    await page.evaluate(showViewerFlow);
    await page.evaluate(settleAnimationFramesFlow);
    await waitFor(async () => {
      const state = await page.evaluate(domSnapshot);
      return state.routeSessionId === messaged.sessionId && state.activity === 'active';
    }, 'nonempty session reactivates after retention release', 30_000);
    await page.evaluate(goHomeFlow);
    await releaseViewerAndSettle(messaged.sessionId);
    return {
      deletedSessionId: boundary.sessionId,
      retainedSessionIds: [messaged.sessionId, later.sessionId, usable.sessionId],
      messagedMarker,
      closeAt: close.at,
      deletedAt,
      exactClockMs,
      retainedActivityClockMs,
      retainedActivities,
    };
  });

  const untouched = await runScenario('untouched-empty-session-is-cleaned', async () => {
    const untouchedClockMs = config.restartClockMs + 5 * RETENTION_MS;
    writeClock(untouchedClockMs);
    await page.call('Page.navigate', { url: config.origin + '/' });
    await waitFor(async () => (await page.evaluate(domSnapshot)).directoryRoute,
      'directory after untouched-session clock reset');

    const created = await page.evaluate(createSessionFlow, {
      workspace: workspacePath('untouched'),
    });
    const initial = await control('inspect_created', { sessionId: created.sessionId });
    assert(initial.session.live, 'untouched session must be persisted before its first prompt');
    assert(
      initial.session.messages === 0 &&
        initial.session.turns === 0 &&
        initial.session.interactions === 0,
      `untouched session must have no conversation content: ${JSON.stringify(initial)}`,
    );
    await page.evaluate(goHomeFlow);
    const release = await releaseViewerAndSettle(created.sessionId);
    const retentionEndMs = Date.parse(
      release.lifecycle.retentionStart,
    ) + RETENTION_MS;

    writeClock(retentionEndMs - 1);
    const before = await control('inspect_created', { sessionId: created.sessionId });
    assert(before.session.live,
      `untouched empty session must remain live at 29:59.999: ${JSON.stringify(before)}`);
    assert((await page.evaluate(listedSessionIds)).includes(created.sessionId),
      'untouched empty session must remain listed at 29:59.999');

    const notificationLog = await page.readLog();
    const notificationsBefore = notificationCount(notificationLog);
    writeClock(retentionEndMs);
    const deletedAt = await waitForDeletedEvent(created.sessionId, 30_000);
    const after = await control('inspect_deleted', { sessionId: created.sessionId });
    assert(!after.session.live && after.session.deletedAt !== null,
      `untouched empty session must leave an authoritative tombstone: ${JSON.stringify(after)}`);
    assert(
      after.session.messages === 0 &&
        after.session.turns === 0 &&
        after.session.interactions === 0,
      'cleanup must not create conversation content',
    );
    await waitFor(async () => !(await page.evaluate(listedSessionIds)).includes(created.sessionId),
      'untouched empty session disappears from product reads', 30_000);
    const finalLog = await page.readLog();
    assert(notificationCount(finalLog) === notificationsBefore,
      `untouched-session cleanup must be silent: ${JSON.stringify(finalLog.notifications)}`);
    assert(report.browserDialogs.length === 0,
      `untouched-session cleanup must not open a dialog: ${JSON.stringify(report.browserDialogs)}`);
    return {
      sessionId: created.sessionId,
      emptyDatabase: initial.session,
      retentionStart: release.lifecycle.retentionStart,
      retentionEndMs,
      deletedAt,
    };
  });

  await runScenario('deleted-session-does-not-resurrect', async () => {
    const agentEvidenceBefore = agentEvidenceTypeCounts(await readAgentEvidence());
    await page.call('Page.navigate', {
      url: config.origin + '/session/' + encodeURIComponent(exact.deletedSessionId),
    });
    const notFound = await waitFor(async () => {
      const log = await page.readLog();
      const entries = wireEntries(log);
      const commandsForDeletedSession = new Map(
        entries
          .filter(
            (entry) =>
              entry.direction === 'outbound' &&
              entry.message.type === 'command' &&
              entry.message.body?.sessionId === exact.deletedSessionId,
          )
          .map((entry) => [entry.message.body?.requestId, entry.message.body?.command]),
      );
      const result = entries.find(
        (entry) =>
          entry.direction === 'inbound' &&
          entry.message.type === 'command.result' &&
          (entry.message.body?.status === 'failed' ||
            entry.message.body?.status === 'rejected') &&
          String(entry.message.body?.error?.code ?? '').startsWith('session.not_found') &&
          commandsForDeletedSession.has(entry.message.body?.requestId),
      );
      return result === null || result === undefined
        ? null
        : {
            command: commandsForDeletedSession.get(result.message.body.requestId),
            requestId: result.message.body.requestId,
            status: result.message.body.status,
            code: result.message.body.error.code,
          };
    }, `late command for deleted session ${exact.deletedSessionId} returns session.not_found`, 30_000);
    await waitFor(async () => {
      const state = await page.evaluate(domSnapshot);
      return state.directoryRoute;
    }, 'deleted deep link returns to the directory', 30_000);
    const state = await page.evaluate(domSnapshot);
    assert(state.directoryRoute, `deleted route must not reappear: ${JSON.stringify(state)}`);
    assert(!(await page.evaluate(listedSessionIds)).includes(exact.deletedSessionId),
      'deleted ID must not be restored from list or cache');
    const agentEvidenceAfter = agentEvidenceTypeCounts(await readAgentEvidence());
    assert(
      JSON.stringify(agentEvidenceAfter) === JSON.stringify(agentEvidenceBefore),
      `a late command for a deleted session must not add any Agent evidence: ` +
        `${JSON.stringify({ before: agentEvidenceBefore, after: agentEvidenceAfter })}`,
    );
    return {
      sessionId: exact.deletedSessionId,
      notFound,
      agentEvidenceCounts: agentEvidenceAfter,
    };
  });

  report.exitCode = acceptanceExitCode();
} catch (error) {
  report.exitCode = error instanceof ConfigError ? 2 : 1;
  report.error = describe(error);
  if (page !== null) {
    try {
      const log = await page.readLog();
      const entries = wireEntries(log);
      report.pairingLookups = log.pairings ?? [];
      report.navigation = log.navigation ?? [];
      report.wireSummary = entries.map((entry) => ({
        direction: entry.direction,
        at: entry.at,
        type: entry.message.type ?? null,
        command: entry.message.body?.command ?? null,
        status: entry.message.body?.status ?? null,
        eventType: entry.message.body?.eventType ?? null,
        errorCode: entry.message.body?.error?.code ?? null,
      }));
      report.failureWire = {
        views: entries
          .filter(
            (entry) =>
              entry.message.type === 'session.view' ||
              entry.message.type === 'session.view.result',
          )
          .map((entry) => ({
            direction: entry.direction,
            at: entry.at,
            type: entry.message.type,
            sessionId: entry.message.body?.sessionId ?? null,
            viewVersion: entry.message.body?.viewVersion ?? null,
            status: entry.message.body?.status ?? null,
          })),
        commands: entries
          .filter(
            (entry) =>
              entry.message.type === 'command' ||
              entry.message.type === 'command.result',
          )
          .map((entry) => ({
            direction: entry.direction,
            at: entry.at,
            type: entry.message.type,
            command: entry.message.body?.command ?? null,
            requestId: entry.message.body?.requestId ?? null,
            sessionId:
              entry.message.body?.sessionId ??
              entry.message.body?.result?.sessionId ??
              null,
            status: entry.message.body?.status ?? null,
            errorCode: entry.message.body?.error?.code ?? null,
          })),
        events: entries
          .filter(
            (entry) =>
              entry.message.type === 'event' &&
              typeof entry.message.body?.eventType === 'string' &&
              entry.message.body.eventType.startsWith('session.'),
          )
          .map((entry) => ({
            at: entry.at,
            eventType: entry.message.body.eventType,
            sessionId: entry.message.body.payload?.view?.sessionId ?? null,
          })),
        activity: log.activity ?? [],
      };
    } catch {}
  }
} finally {
  if (report.pageErrors.length > 0 && report.exitCode === 0) {
    report.exitCode = 1;
  }
  try {
    page?.close();
    browserCdp?.close();
  } catch {}
  try {
    await terminateBrowser();
  } catch (error) {
    report.cleanupErrors ??= [];
    report.cleanupErrors.push(`browser termination: ${describe(error)}`);
    report.exitCode = 1;
  }
  try {
    rmSync(userDataDir, { recursive: true, force: true });
  } catch (error) {
    report.cleanupErrors ??= [];
    report.cleanupErrors.push(`browser profile removal: ${describe(error)}`);
    report.exitCode = 1;
  }
  report.finishedAt = new Date().toISOString();
  try {
    mkdirSync(dirname(config.evidence), { recursive: true });
    writeFileSync(config.evidence, JSON.stringify(report, null, 2) + '\n', 'utf8');
  } catch (error) {
    process.stderr.write(`evidence write failed: ${describe(error)}\n`);
    report.exitCode = 2;
  }
  process.stderr.write(
    `session-create-empty-cleanup-live: ` +
      `${report.scenarios.filter((item) => item.status === 'passed').length} passed, ` +
      `${report.scenarios.filter((item) => item.status === 'failed').length} failed, ` +
      `${report.skipped.length} skipped -> exit ${report.exitCode}\n`,
  );
  if (report.error) {
    process.stderr.write(`error: ${report.error}\n`);
    if (browserStderr.length > 0) {
      process.stderr.write(`browser stderr:\n${browserStderr.join('')}\n`);
    }
  }
  if (report.pageErrors.length > 0) {
    process.stderr.write(`page errors:\n${JSON.stringify(report.pageErrors, null, 2)}\n`);
  }
  if (report.cleanupErrors?.length > 0) {
    process.stderr.write(`cleanup errors:\n${JSON.stringify(report.cleanupErrors, null, 2)}\n`);
  }
  process.exitCode = report.exitCode;
}

function acceptanceExitCode() {
  if (report.pageErrors.length > 0) return 1;
  if (report.scenarios.some((scenario) => scenario.status === 'failed')) return 1;
  if (report.skipped.length > 0) return 3;
  return 0;
}

function terminateBrowser() {
  if (browserTermination !== null) return browserTermination;
  browserTermination = (async () => {
    if (browser.exitCode === null && browser.signalCode === null) {
      browser.kill();
      await waitForBrowserExit(5_000);
    }
    if (browser.exitCode === null && browser.signalCode === null) {
      browser.kill('SIGKILL');
      await waitForBrowserExit(5_000);
    }
    assert(
      browser.exitCode !== null || browser.signalCode !== null,
      `browser process ${browser.pid ?? '(unknown pid)'} did not terminate`,
    );
  })();
  return browserTermination;
}

function waitForBrowserExit(timeout) {
  if (browser.exitCode !== null || browser.signalCode !== null) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, timeout);
    const done = () => {
      clearTimeout(timer);
      resolve();
    };
    browser.once('exit', done);
    browser.once('close', done);
  });
}

function workspacePath(name) {
  return join(config.workspaceRoot, name).replaceAll('\\', '/');
}

async function runScenario(name, body) {
  const startedAt = Date.now();
  try {
    const details = await body();
    report.scenarios.push({ name, status: 'passed', durationMs: Date.now() - startedAt, details });
    process.stderr.write(`PASS ${name}\n`);
    return details;
  } catch (error) {
    report.scenarios.push({
      name,
      status: 'failed',
      durationMs: Date.now() - startedAt,
      error: describe(error),
    });
    process.stderr.write(`FAIL ${name}: ${describe(error)}\n`);
    throw error;
  }
}

function describe(error) {
  if (error instanceof AssertionError || error instanceof ConfigError) return error.message;
  return error?.stack ?? String(error);
}

function assert(condition, message) {
  if (!condition) throw new AssertionError(message);
}

async function waitFor(predicate, label, timeout = 20_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      const value = await predicate();
      if (value) return value;
    } catch {}
    await sleep(50);
  }
  throw new AssertionError(`timeout waiting for ${label}`);
}

function atomicWrite(path, text) {
  const temporary = `${path}.tmp-${process.pid}-${Date.now()}`;
  writeFileSync(temporary, text, 'utf8');
  const deadline = Date.now() + 2_000;
  while (true) {
    try {
      renameSync(temporary, path);
      return;
    } catch (error) {
      if (
        !['EACCES', 'EBUSY', 'EPERM'].includes(error?.code) ||
        Date.now() >= deadline
      ) {
        rmSync(temporary, { force: true });
        throw error;
      }
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 5);
    }
  }
}

function writeClock(millis) {
  assert(Number.isSafeInteger(millis), `invalid clock value ${millis}`);
  atomicWrite(config.clockFile, String(millis));
}

async function control(op, extra = {}) {
  const id = ++controlSequence;
  const request = join(config.controlDir, `request-${id}.json`);
  const response = join(config.controlDir, `response-${id}.json`);
  atomicWrite(request, JSON.stringify({ op, ...extra }));
  const raw = await waitFor(() => (existsSync(response) ? readFileSync(response, 'utf8') : null),
    `control response ${op}`, 30_000);
  const parsed = JSON.parse(raw);
  assert(parsed.ok === true, `control ${op} failed: ${raw}`);
  return parsed;
}

async function readAgentEvidence() {
  const text = existsSync(config.agentEvidence) ? readFileSync(config.agentEvidence, 'utf8') : '';
  return text
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return null;
      }
    })
    .filter(Boolean);
}

function agentEvidenceTypeCounts(records) {
  return Object.fromEntries(
    Object.entries(
      records.reduce((counts, record) => {
        const type = typeof record.type === 'string' ? record.type : '(missing)';
        counts[type] = (counts[type] ?? 0) + 1;
        return counts;
      }, {}),
    ).sort(([left], [right]) => left.localeCompare(right)),
  );
}

async function waitForLifecycleUnprotected(sessionId) {
  return waitFor(async () => {
    const id = ++observationSequence;
    atomicWrite(config.clockFile + '.observe-request', JSON.stringify({ id, sessionId }));
    const responsePath = config.clockFile + '.observe-response-' + id + '.json';
    const snapshot = await waitFor(async () => {
      try { return JSON.parse(readFileSync(responsePath, 'utf8')); } catch { return null; }
    }, 'Daemon lifecycle observation response', 10_000);
    rmSync(responsePath, { force: true });
    assert(snapshot.id === id && snapshot.sessionId === sessionId, 'lifecycle observation correlation');
    return snapshot.viewerCount === 0 && typeof snapshot.retentionStart === 'string'
      ? snapshot : null;
  }, 'all viewers released and retention start established for ' + sessionId, 15_000);
}

async function releaseViewerAndSettle(sessionId) {
  let observed = null;
  for (let attempt = 0; attempt < 5; attempt += 1) {
    const before = await page.readLog();
    const views = wireEntries(before).filter(
      (entry) => entry.direction === 'outbound' && entry.message.type === 'session.view',
    );
    const latest = views.at(-1)?.message.body ?? null;
    if (latest?.sessionId !== null) {
      observed = await page.evaluate(hideViewerFlow);
    }
    await page.evaluate(settleAnimationFramesFlow);

    const log = await page.readLog();
    const entries = wireEntries(log);
    const viewsAfter = entries.filter(
      (entry) => entry.direction === 'outbound' && entry.message.type === 'session.view',
    );
    const release = viewsAfter.at(-1);
    const body = release?.message.body ?? null;
    const accepted = body?.sessionId === null
      ? entries.find(
          (entry) =>
            entry.direction === 'inbound' &&
            entry.message.type === 'session.view.result' &&
            entry.message.body?.sessionId === null &&
            entry.message.body?.viewVersion === body.viewVersion &&
            entry.message.body?.status === 'accepted',
        )
      : null;
    const state = await page.evaluate(domSnapshot);
    if (accepted && state.directoryRoute && body?.sessionId === null) {
      return {
        sessionId,
        viewVersion: body.viewVersion,
        acceptedAt: accepted.at,
        release: observed,
        lifecycle: await waitForLifecycleUnprotected(sessionId),
      };
    }
    assert(
      body?.sessionId === null || body?.sessionId === sessionId,
      `unexpected viewer while releasing ${sessionId}: ${JSON.stringify(body)}`,
    );
  }
  const log = await page.readLog();
  throw new AssertionError(
    `viewer release for ${sessionId} did not settle: ${JSON.stringify(
      wireEntries(log)
        .filter((entry) => entry.message.type === 'session.view')
        .slice(-8)
        .map((entry) => ({ direction: entry.direction, body: entry.message.body, at: entry.at })),
    )}`,
  );
}

async function resumeObservationBaseline(sessionId, agentSessionId) {
  const entries = wireEntries(await page.readLog());
  const agentEvidence = await readAgentEvidence();
  return {
    completedRequestIds: new Set(
      entries
        .filter(
          (entry) =>
            entry.direction === 'inbound' &&
            entry.message.type === 'command.result' &&
            entry.message.body?.command === 'session.resume' &&
            entry.message.body?.result?.sessionId === sessionId &&
            entry.message.body?.status === 'completed',
        )
        .map((entry) => entry.message.body?.requestId)
        .filter((requestId) => typeof requestId === 'string'),
    ),
    agentResumeCount: agentEvidence.filter(
      (record) =>
        record.type === 'session_resume' &&
        record.sessionId === agentSessionId,
    ).length,
    agentResultCount: agentEvidence.filter(
      (record) =>
        record.type === 'session_resume_result' &&
        record.sessionId === agentSessionId,
    ).length,
  };
}

async function waitForResumeCompleted(sessionId, agentSessionId, baseline) {
  return waitFor(async () => {
    const entries = wireEntries(await page.readLog());
    const completions = entries
      .filter(
        (entry) =>
          entry.direction === 'inbound' &&
          entry.message.type === 'command.result' &&
          entry.message.body?.command === 'session.resume' &&
          entry.message.body?.result?.sessionId === sessionId &&
          entry.message.body?.status === 'completed' &&
          typeof entry.message.body?.requestId === 'string' &&
          !baseline.completedRequestIds.has(entry.message.body.requestId),
      );
    if (completions.length === 0) return null;
    const requestIds = new Set(
      entries
        .filter(
          (entry) =>
            entry.direction === 'outbound' &&
            entry.message.type === 'command' &&
            entry.message.body?.command === 'session.resume' &&
            entry.message.body?.sessionId === sessionId,
        )
        .map((entry) => entry.message.body?.requestId)
        .filter((requestId) => typeof requestId === 'string'),
    );
    const database = await control('inspect_created', { sessionId });
    const mutations = database.session.commands;
    if (mutations.some((record) => record.status === 'accepted')) return null;
    const completedIds = new Set(
      entries
        .filter(
          (entry) =>
            entry.direction === 'inbound' &&
            entry.message.type === 'command.result' &&
            entry.message.body?.command === 'session.resume' &&
            entry.message.body?.result?.sessionId === sessionId &&
            entry.message.body?.status === 'completed',
        )
        .map((entry) => entry.message.body.requestId),
    );
    // Every dispatched resume must settle before advancing the exact clock:
    // a second accepted mutation still protects the session after the first result.
    if (
      completions.some((entry) => !requestIds.has(entry.message.body.requestId)) ||
      [...requestIds].some((requestId) => !completedIds.has(requestId) && !mutations.some((record) =>
        record.requestId === requestId && record.command === 'session.resume' &&
        record.status === 'completed' && typeof record.terminalEventId === 'string'))
    ) {
      return null;
    }
    const agentEvidence = await readAgentEvidence();
    const agentResumes = agentEvidence.filter(
      (record) =>
        record.type === 'session_resume' &&
        record.sessionId === agentSessionId,
    );
    const agentResults = agentEvidence.filter(
      (record) =>
        record.type === 'session_resume_result' &&
        record.sessionId === agentSessionId,
    );
    if (
      agentResumes.length <= baseline.agentResumeCount ||
      agentResults.length <= baseline.agentResultCount
    ) {
      return null;
    }
    return {
      requestIds: [...requestIds],
      durableTerminals: mutations,
      wireCompletedRequestIds: completions.map((entry) => entry.message.body.requestId),
      completedAt: Math.max(...completions.map((entry) => entry.at)),
      agentResumeCount: agentResumes.length,
      agentResultCount: agentResults.length,
    };
  }, `completed session.resume and Agent resume result for ${sessionId}`, 30_000);
}

async function agentSessionIdForWorkspace(name) {
  const expected = workspacePath(name).replaceAll('\\', '/').replace(/\/+$/, '').toLowerCase();
  const record = (await readAgentEvidence())
    .filter(
      (item) =>
        item.type === 'session_new' &&
        typeof item.cwd === 'string' &&
        item.cwd.replaceAll('\\', '/').replace(/\/+$/, '').toLowerCase() === expected,
    )
    .at(-1);
  assert(record, `controlled Agent has no session/new evidence for workspace ${name}`);
  return record.sessionId;
}

async function waitForDeletedEvent(sessionId, timeout) {
  return waitFor(async () => {
    const log = await page.readLog();
    const deleted = inboundFrames(log).find(
      (message) =>
        message.type === 'event' &&
        message.body?.eventType === 'session.deleted' &&
        message.body?.payload?.view?.sessionId === sessionId,
    );
    if (!deleted) return null;
    const entry = wireEntries(log).find(
      (item) =>
        item.direction === 'inbound' &&
        item.message.type === 'event' &&
        item.message.body?.eventType === 'session.deleted' &&
        item.message.body?.payload?.view?.sessionId === sessionId,
    );
    return entry?.at ?? null;
  }, `session.deleted for ${sessionId}`, timeout).catch(async (error) => {
    const database = await control('inspect_created', { sessionId });
    throw new AssertionError(`${describe(error)}; database=${JSON.stringify(database)}`);
  });
}

async function waitForBrowserEndpoint(port) {
  let lastError = null;
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    try {
      const pages = await (
        await fetch(`http://127.0.0.1:${port}/json/list`, {
          signal: AbortSignal.timeout(1_000),
        })
      ).json();
      const candidate = pages.find((entry) => entry.type === 'page');
      if (candidate?.webSocketDebuggerUrl) return candidate.webSocketDebuggerUrl;
    } catch (error) {
      lastError = error;
    }
    await sleep(100);
  }
  throw new ConfigError(`browser debugging endpoint unavailable: ${describe(lastError)}`);
}

async function connectCdp(url, onEvent = () => {}) {
  const socket = new WebSocket(url);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  let nextId = 0;
  const pending = new Map();
  socket.addEventListener('message', ({ data }) => {
    const message = JSON.parse(String(data));
    if (message.id === undefined) {
      onEvent(message);
      return;
    }
    const waiter = pending.get(message.id);
    if (!waiter) return;
    pending.delete(message.id);
    message.error
      ? waiter.reject(new Error(JSON.stringify(message.error)))
      : waiter.resolve(message.result);
  });
  return {
    call(method, params = {}) {
      return new Promise((resolve, reject) => {
        const id = ++nextId;
        pending.set(id, { resolve, reject });
        socket.send(JSON.stringify({ id, method, params }));
      });
    },
    close() {
      try {
        socket.close();
      } catch {}
    },
  };
}

async function openPage(browserConnection, url, expectedPairingCode) {
  const target = await browserConnection.call('Target.createTarget', { url: 'about:blank' });
  const targetId = target.targetId;
  const pageConnection = await waitFor(async () => {
    const pages = await (
      await fetch(`http://127.0.0.1:${debuggingPort}/json/list`, {
        signal: AbortSignal.timeout(1_000),
      })
    ).json();
    const entry = pages.find((candidate) => candidate.id === targetId);
    return entry?.webSocketDebuggerUrl
      ? connectCdp(entry.webSocketDebuggerUrl, (message) => {
          if (message.method === 'Runtime.exceptionThrown') {
            const details = message.params?.exceptionDetails ?? {};
            report.pageErrors.push({
              at: Date.now(),
              text: details.text ?? 'unknown page exception',
              url: details.url ?? null,
              lineNumber: details.lineNumber ?? null,
              columnNumber: details.columnNumber ?? null,
              exception: details.exception === undefined
                ? null
                : {
                    type: details.exception?.type ?? null,
                    subtype: details.exception?.subtype ?? null,
                    className: details.exception?.className ?? null,
                    description: details.exception?.description ?? null,
                  },
              stackTrace: details.stackTrace ?? null,
            });
          }
          if (message.method === 'Page.javascriptDialogOpening') {
            report.browserDialogs.push({
              at: Date.now(),
              type: message.params?.type ?? null,
              message: message.params?.message ?? null,
            });
          }
        })
      : null;
  }, 'new browser page target', 10_000);
  await pageConnection.call('Page.enable');
  await pageConnection.call('Runtime.enable');
  await pageConnection.call('Page.addScriptToEvaluateOnNewDocument', {
    source: instrumentation(expectedPairingCode),
  });
  await pageConnection.call('Page.navigate', { url });

  const evaluateValue = async (expression, awaitPromise = true) => {
    const result = await pageConnection.call('Runtime.evaluate', {
      expression,
      awaitPromise,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      throw new AssertionError(
        result.exceptionDetails.exception?.description ?? result.exceptionDetails.text,
      );
    }
    return result.result.value;
  };

  return {
    targetId,
    call: pageConnection.call,
    close: pageConnection.close,
    evaluateValue,
    evaluate(fn, arg) {
      return evaluateValue(`(${fn.toString()})(${JSON.stringify(arg ?? null)})`);
    },
    async readLog() {
      const raw = await evaluateValue(
        `JSON.stringify(window.__acpCreateCleanupLog ?? {entries:[], inbound:[], outbound:[], notifications:[], activity:[], pairings:[], navigation:[]})`,
      );
      return JSON.parse(raw);
    },
  };
}

function instrumentation(expectedPairingCode) {
  return `(() => {
    const NativeWebSocket = window.WebSocket;
    const NativeFetch = window.fetch;
    const expectedPairingCode = ${JSON.stringify(expectedPairingCode)};
    const log = {
      outbound: [],
      inbound: [],
      entries: [],
      notifications: [],
      activity: [],
      pairings: [],
      navigation: [],
      sockets: [],
    };
    Object.defineProperty(window, '__acpCreateCleanupLog', { value: log, configurable: true });
    window.fetch = async (...args) => {
      const response = await NativeFetch.apply(window, args);
      try {
        const requestUrl = new URL(String(args[0] instanceof Request ? args[0].url : args[0]), location.href);
        if (requestUrl.pathname === '/sync/v1/pairing/lookup') {
          const init = args[1] ?? {};
          const requestBody = typeof init.body === 'string' ? JSON.parse(init.body) : null;
          let responseBody = null;
          try {
            responseBody = await response.clone().json();
          } catch {}
          log.pairings.push({
            status: response.status,
            requestProtocolVersion: requestBody?.protocolVersion ?? null,
            pairingCodeMatchesExpected:
              requestBody?.pairingCode ===
              expectedPairingCode.slice(0, 4) + '-' + expectedPairingCode.slice(4),
            responseProtocolVersion: responseBody?.protocolVersion ?? null,
            canonicalOriginMatchesPage: responseBody?.canonicalOrigin === location.origin,
            pairingIdIsUuid:
              typeof responseBody?.pairingId === 'string' &&
              /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(responseBody.pairingId),
            hostIdIsUuid:
              typeof responseBody?.hostId === 'string' &&
              /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(responseBody.hostId),
            secretBytes: base64UrlBytes(responseBody?.pairingSecret),
            hostPublicKeyBytes: base64UrlBytes(responseBody?.hostPublicKey),
            expiresAtParseable:
              typeof responseBody?.expiresAt === 'string' &&
              !Number.isNaN(Date.parse(responseBody.expiresAt)),
            responseCode: responseBody?.code ?? null,
            responseShape: responseBody === null ? null : Object.keys(responseBody).sort(),
          });
        }
      } catch {}
      return response;
    };
    const base64UrlBytes = (value) => {
      if (typeof value !== 'string' || !/^[A-Za-z0-9_-]*$/.test(value)) return null;
      try {
        return atob(value.replaceAll('-', '+').replaceAll('_', '/') + '='.repeat((4 - value.length % 4) % 4)).length;
      } catch {
        return null;
      }
    };
    const recordActivity = () => {
      const id = document.querySelector('[data-route="session"]')?.getAttribute('data-session-id') ?? null;
      const value = document.querySelector('[data-session-header] [data-session-activity]')
        ?.getAttribute('data-session-activity') ?? null;
      const last = log.activity.at(-1);
      if (id !== null && value !== null && (last?.sessionId !== id || last?.activity !== value)) {
        log.activity.push({ sessionId: id, activity: value, at: Date.now() });
      }
    };
    const scanNotifications = () => {
      for (const node of document.querySelectorAll('[role="alert"], [role="dialog"], [data-toast]')) {
        const rect = node.getBoundingClientRect();
        if (rect.width === 0 || rect.height === 0) continue;
        const text = String(node.textContent ?? '').trim().slice(0, 500);
        const role = node.getAttribute('role');
        const key = role + ':' + text;
        if (text && !log.notifications.some((item) => item.key === key)) {
          log.notifications.push({ key, role, text, at: Date.now() });
        }
      }
    };
    const nativeAlert = window.alert;
    window.alert = (message) => {
      log.notifications.push({ key: 'alert:' + String(message), role: 'alert', text: String(message), at: Date.now() });
      return nativeAlert.call(window, message);
    };
    class TrackedWebSocket extends NativeWebSocket {
      constructor(...args) {
        super(...args);
        log.sockets.push(this);
        this.addEventListener('message', (event) => {
          const text = String(event.data);
          const at = Date.now();
          log.inbound.push(text);
          log.entries.push({ direction: 'inbound', text, at });
        });
      }
      send(data) {
        const text = String(data);
        const at = Date.now();
        log.outbound.push(text);
        log.entries.push({ direction: 'outbound', text, at });
        return super.send(data);
      }
    }
    window.WebSocket = TrackedWebSocket;
    const installObserver = () => {
      if (!document.documentElement) {
        document.addEventListener('DOMContentLoaded', installObserver, { once: true });
        return;
      }
      new MutationObserver(() => { recordActivity(); scanNotifications(); })
        .observe(document.documentElement, { childList: true, subtree: true, attributes: true });
      recordActivity();
      scanNotifications();
    };
    installObserver();
    setInterval(() => { recordActivity(); scanNotifications(); }, 10);
  })();`;
}

function wireEntries(log) {
  return (log.entries ?? [])
    .map((entry) => {
      try {
        return { ...entry, message: JSON.parse(entry.text) };
      } catch {
        return null;
      }
    })
    .filter(Boolean);
}

function outboundFrames(log) {
  return wireEntries(log)
    .filter((entry) => entry.direction === 'outbound')
    .map((entry) => entry.message);
}

function inboundFrames(log) {
  return wireEntries(log)
    .filter((entry) => entry.direction === 'inbound')
    .map((entry) => entry.message);
}

function commandFrames(log, command) {
  return outboundFrames(log)
    .filter((message) => message.type === 'command' && message.body?.command === command)
    .map((message) => message.body);
}

function viewFrames(log) {
  return outboundFrames(log)
    .filter((message) => message.type === 'session.view')
    .map((message) => message.body);
}

function activityTrail(log, sessionId) {
  return (log.activity ?? [])
    .filter((entry) => entry.sessionId === sessionId)
    .map((entry) => entry.activity);
}

function localActivityProjectionAt(log, sessionId) {
  return inboundFrames(log)
    .filter(
      (message) =>
        message.type === 'event' &&
        message.body?.eventType === 'session.activity.changed' &&
        message.body?.payload?.view?.session?.sessionId === sessionId,
    )
    .map((message) => message.body.payload.view.session.localActivity)
    .at(-1) ?? null;
}

function notificationCount(log) {
  return (log.notifications ?? []).length;
}

function domSnapshot() {
  const pathMatch = /^\/session\/([^/]+)$/.exec(location.pathname);
  const pathSessionId = pathMatch ? decodeURIComponent(pathMatch[1]) : null;
  const root =
    pathSessionId === null
      ? null
      : document.querySelector(
          `[data-route="session"][data-session-id="${CSS.escape(pathSessionId)}"]`,
        );
  const header = root?.querySelector('[data-session-header]') ?? null;
  const input = root?.querySelector('[data-composer-input]') ?? null;
  const submit = root?.querySelector('[data-action="submit-prompt"]') ?? null;
  return {
    directoryRoute: location.pathname === '/',
    routeSessionId: root?.getAttribute('data-session-id') ?? null,
    activity: header?.querySelector('[data-session-activity]')?.getAttribute('data-session-activity') ?? null,
    composer: root?.querySelector('[data-composer]')?.getAttribute('data-composer') ?? null,
    composerValue: input?.value ?? null,
    submitEnabled: submit?.getAttribute('data-submit-enabled') ?? null,
    path: location.pathname,
    text: document.body.innerText.slice(-3_000),
  };
}

async function pairFlow({ code }) {
  const q = (selector) => document.querySelector(selector);
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const wait = async (predicate, label, timeout = 30_000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      try {
        const value = predicate();
        if (value) return value;
      } catch {}
      await sleep(30);
    }
    throw new Error(`timeout waiting for ${label}\n${document.body.innerText.slice(-1_500)}`);
  };
  const cells = Array.from({ length: 8 }, () => '');
  const normalizedCode = code
    .trim()
    .toUpperCase()
    .replace(/^(.{4})-(.{4})$/, '$1$2')
    .replaceAll('I', '1')
    .replaceAll('L', '1')
    .replaceAll('O', '0');
  if (!/^[0-9A-HJKMNP-TV-Z]{8}$/.test(normalizedCode)) {
    throw new Error(`pairing code is not canonical: ${code}`);
  }
  const entry = await wait(
    () => q('[data-pairing-code-cell="0"]') ?? q('[data-problem-action="repair"]'),
    'pairing entry',
  );
  if (entry.getAttribute('data-problem-action') === 'repair') {
    entry.click();
    await wait(
      () => q('[data-pairing-code-cell="0"]'),
      'pairing-code entry after identity confirmation',
    );
  }
  for (const [index, character] of [...normalizedCode].entries()) {
    const input = await wait(
      () => q(`[data-pairing-code-cell="${index}"]`),
      `pairing-code input ${index}`,
    );
    input.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, character);
    input.dispatchEvent(new InputEvent('input', {
      bubbles: true,
      data: character,
      inputType: 'insertText',
    }));
    cells[index] = character;
    await wait(
      () => cells.every((expected, position) =>
        q(`[data-pairing-code-cell="${position}"]`)?.value === expected,
      ),
      `pairing-code cells after character ${index + 1}`,
    );
  }
  (await wait(() => q('[data-action="submit-pairing-code"]'), 'pairing submit')).click();
  try {
    await wait(() => q('[data-route="dirs"]'), 'directory page after pairing', 10_000);
  } catch (error) {
    const phase = q('[data-pairing-phase]')?.getAttribute('data-pairing-phase') ?? null;
    const failure = q('[data-pairing-failure]')?.getAttribute('data-pairing-failure') ?? null;
    const diagnostic = q('[data-pairing-error]')?.getAttribute('data-pairing-error') ?? null;
    throw new Error(
      `${error.message}\npairing phase=${phase} failure=${failure} diagnostic=${diagnostic}`,
    );
  }
  return { paired: true };
}

function createSessionFlow({ workspace }) {
  return (async () => {
    const q = (selector) => document.querySelector(selector);
    const qa = (selector) => [...document.querySelectorAll(selector)];
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const norm = (value) => String(value ?? '').replaceAll('\\', '/').replace(/\/+$/, '').toLowerCase();
    const wait = async (predicate, label, timeout = 30_000) => {
      const deadline = Date.now() + timeout;
      while (Date.now() < deadline) {
        try {
          const value = predicate();
          if (value) return value;
        } catch {}
        await sleep(30);
      }
      throw new Error(`timeout waiting for ${label}\n${document.body.innerText.slice(-1_500)}`);
    };
    const click = async (selector, label) =>
      (await wait(() => q(selector), label ?? selector)).click();
    const here = () => q('[data-browse-path]')?.getAttribute('data-browse-path') ?? '';
    const goal = norm(workspace);
    const browseReady = async (expectedPath) => {
      const expected = norm(expectedPath);
      const settled = await wait(() => {
        const state = q('[data-browse-state]')?.getAttribute('data-browse-state');
        return norm(here()) === expected && state !== 'loading';
      }, `browse path ${expectedPath}`);
      window.__acpCreateCleanupLog.navigation.push({
        phase: 'arrived',
        path: here(),
        state: q('[data-browse-state]')?.getAttribute('data-browse-state') ?? null,
        at: Date.now(),
      });
      return settled;
    };
    const enterPath = async () => {
      await wait(() => !q('[data-browse-state="loading"]'), 'directory browser initial readiness');
      for (let guard = 0; guard < 64 && norm(here()) !== goal; guard++) {
        const current = norm(here());
        const crumb = qa('[data-crumb-path]').find((node) => norm(node.getAttribute('data-crumb-path')) === goal);
        if (crumb) {
          crumb.click();
          await browseReady(goal);
          continue;
        }
        const prefix = current === '' ? '' : current === '/' ? '/' : current + '/';
        if (current === '' || current === '/' || goal.startsWith(prefix)) {
          const absolute = current === '' && goal.startsWith('/');
          const segment = goal.slice(absolute ? 1 : prefix.length).split('/')[0];
          const nextPath = absolute ? '/' + segment : prefix + segment;
          const entries = qa('[data-action="enter-directory"]');
          const entry =
            entries.find((node) => norm(node.getAttribute('data-entry-name')) === norm(segment)) ??
            entries.find((node) => norm(node.getAttribute('data-entry-path')) === norm(nextPath)) ??
            entries
              .map((node) => ({ node, path: norm(node.getAttribute('data-entry-path')) }))
              .filter(({ path }) => path !== '' && (goal === path || goal.startsWith(path + '/')))
              .sort((left, right) => right.path.length - left.path.length)[0]?.node;
          if (!entry) throw new Error(`directory entry not found: ${segment} under ${here()}`);
          const expected = entry.getAttribute('data-entry-path') || nextPath;
          window.__acpCreateCleanupLog.navigation.push({
            phase: 'click',
            current,
            segment,
            expected,
            entries: entries.map((node) => ({
              name: node.getAttribute('data-entry-name'),
              path: node.getAttribute('data-entry-path'),
            })),
            at: Date.now(),
          });
          entry.click();
          if (norm(expected) === goal) {
            await browseReady(goal);
            return;
          }
          await browseReady(expected);
          continue;
        }
        const up = q('[data-action="browse-up"]');
        if (!up) throw new Error(`cannot reach ${workspace} from ${here()}`);
        const before = norm(here());
        up.click();
        await wait(() => norm(here()) !== before, 'browse one level up');
      }
      if (norm(here()) !== goal) throw new Error(`directory browser stopped at ${here()}, wanted ${workspace}`);
    };

    if (location.pathname !== '/') {
      await click('[data-action="open-directory-list"]', 'directory-list navigation');
      await wait(
        () => location.pathname === '/' && q('[data-route="dirs"]'),
        'directory page after leaving the current session',
      );
    }
    await click('[data-action="create-session-browse"]', 'top-level create entry');
    await click('[data-action="open-directory-picker"]', 'open directory picker');
    await enterPath();
    await click('[data-action="confirm-directory"]', 'confirm directory');
    await wait(() => q('[data-directory-picker="open"]') === null, 'directory picker close');
    await click('[data-action="select-agent"]', 'select Agent');
    const createButton = await wait(() => {
      const candidate = q('[data-action="submit-create"]');
      return candidate && candidate.getAttribute('aria-disabled') !== 'true' ? candidate : null;
    }, 'create-session button enabled');
    const log = window.__acpCreateCleanupLog;
    const entryFloor = log.entries.length;
    createButton.click();
    const command = await wait(() => {
      const entry = log.entries.slice(entryFloor)
        .map((item) => { try { return { ...item, message: JSON.parse(item.text) }; } catch { return null; } })
        .filter(Boolean)
        .find(
          (item) =>
            item.direction === 'outbound' &&
            item.message.type === 'command' &&
            item.message.body?.command === 'session.create',
        );
      return entry?.message.body ?? null;
    }, 'outbound session.create', 30_000);
    const requestId = command.requestId;
    const sessionId = await wait(() => {
      const result = log.entries.slice(entryFloor)
        .map((item) => { try { return { ...item, message: JSON.parse(item.text) }; } catch { return null; } })
        .filter(Boolean)
        .find(
          (item) =>
            item.direction === 'inbound' &&
            item.message.type === 'command.result' &&
            item.message.body?.requestId === requestId &&
            item.message.body?.command === 'session.create' &&
            item.message.body?.status === 'completed',
        );
      const returned = result?.message.body?.result?.sessionId;
      return typeof returned === 'string' ? returned : null;
    }, `completed session.create ${requestId}`, 30_000);
    await wait(
      () => decodeURIComponent(location.pathname.split('/').filter(Boolean).pop() ?? '') === sessionId,
      `created-session route ${sessionId}`,
      30_000,
    );
    await wait(
      () => {
        const root = q(`[data-route="session"][data-session-id="${sessionId}"]`);
        return root &&
          root.querySelector('[data-session-header] [data-session-activity="active"]') &&
          root.querySelector('[data-composer="editable"]');
      },
      'created session active and editable',
      30_000,
    );
    await wait(() => {
      const entries = log.entries.slice(entryFloor)
        .map((item) => { try { return { ...item, message: JSON.parse(item.text) }; } catch { return null; } })
        .filter(Boolean);
      const resume = entries.find(
        (item) =>
          item.direction === 'outbound' &&
          item.message.type === 'command' &&
          item.message.body?.command === 'session.resume' &&
          item.message.body?.sessionId === sessionId,
      );
      return resume && entries.find(
        (item) =>
          item.direction === 'inbound' &&
          item.message.type === 'command.result' &&
          item.message.body?.requestId === resume.message.body.requestId &&
          item.message.body?.command === 'session.resume' &&
          item.message.body?.status === 'completed',
      );
    }, `active session.resume ${sessionId}`, 30_000);
    await wait(() => {
      const entries = log.entries.slice(entryFloor)
        .map((item) => { try { return { ...item, message: JSON.parse(item.text) }; } catch { return null; } })
        .filter(Boolean);
      const read = entries.find(
        (item) =>
          item.direction === 'outbound' &&
          item.message.type === 'command' &&
          item.message.body?.command === 'session.read' &&
          item.message.body?.sessionId === sessionId,
      );
      return read && entries.find(
        (item) =>
          item.direction === 'inbound' &&
          item.message.type === 'command.result' &&
          item.message.body?.requestId === read.message.body.requestId &&
          item.message.body?.command === 'session.read' &&
          item.message.body?.status === 'completed',
      );
    }, `initial session.read ${sessionId}`, 30_000);
    return { sessionId, requestId };
  })();
}

function goHomeFlow() {
  return (async () => {
    const q = (selector) => document.querySelector(selector);
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    if (location.pathname !== '/') {
      const entry = q('[data-action="open-directory-list"]');
      if (!entry) throw new Error('directory-list navigation is unavailable');
      entry.click();
    }
    const deadline = Date.now() + 20_000;
    while (Date.now() < deadline) {
      if (location.pathname === '/' && q('[data-route="dirs"]')) return true;
      await sleep(30);
    }
    throw new Error(`timeout waiting for directory page\n${document.body.innerText.slice(-1_500)}`);
  })();
}

function hasSessionRow({ sessionId }) {
  return [...document.querySelectorAll('[data-route="dirs"] [data-session-id]')]
    .some((node) => node.getAttribute('data-session-id') === sessionId);
}

function listedSessionIds() {
  return [...document.querySelectorAll('[data-route="dirs"] [data-session-id]')]
    .map((node) => node.getAttribute('data-session-id'))
    .filter(Boolean);
}

function openSessionFlow({ sessionId, requireActive = true }) {
  return (async () => {
    const q = (selector) => document.querySelector(selector);
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const wait = async (predicate, label, timeout = 30_000) => {
      const deadline = Date.now() + timeout;
      while (Date.now() < deadline) {
        try {
          const value = predicate();
          if (value) return value;
        } catch {}
        await sleep(30);
      }
      throw new Error(`timeout waiting for ${label}\n${document.body.innerText.slice(-1_500)}`);
    };
    if (location.pathname !== '/') {
      const entry = await wait(() => q('[data-action="open-directory-list"]'), 'directory-list navigation');
      entry.click();
      await wait(
        () => location.pathname === '/' && q('[data-route="dirs"]'),
        'directory page',
      );
    }
    const row = await wait(
      () => q(`[data-route="dirs"] [data-session-id="${sessionId}"]`),
      `session row ${sessionId}`,
    );
    (row.querySelector('[data-action="open-session"]') ?? row).click();
    await wait(
      () => q(`[data-route="session"][data-session-id="${sessionId}"]`),
      `session route ${sessionId}`,
    );
    if (requireActive) {
      await wait(
        () => q('[data-session-header] [data-session-activity="active"]'),
        `session ${sessionId} active`,
      );
    }
    return { routeSessionId: sessionId };
  })();
}

function sendPromptFlow({ text, sessionId: expectedSessionId }) {
  return (async () => {
    const q = (selector) => document.querySelector(selector);
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const wait = async (predicate, label, timeout = 30_000) => {
      const deadline = Date.now() + timeout;
      while (Date.now() < deadline) {
        const value = predicate();
        if (value) return value;
        await sleep(30);
      }
      throw new Error(`timeout waiting for ${label}\n${document.body.innerText.slice(-1_500)}`);
    };
    const rootForSession = () =>
      q(`[data-route="session"][data-session-id="${expectedSessionId}"]`);
    const input = await wait(
      () => rootForSession()?.querySelector('[data-composer-input]'),
      `composer input for ${expectedSessionId}`,
    );
    const prototype =
      input.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(input, text);
    input.dispatchEvent(new Event('input', { bubbles: true }));
    const submit = await wait(() => {
      const candidate = rootForSession()?.querySelector('[data-action="submit-prompt"]');
      return candidate && candidate.getAttribute('data-submit-enabled') === 'true' ? candidate : null;
    }, `prompt submission enabled for ${expectedSessionId}`);
    const entryFloor = window.__acpCreateCleanupLog.entries.length;
    submit.click();
    const command = await wait(() => {
      const entry = window.__acpCreateCleanupLog.entries
        .slice(entryFloor)
        .map((item) => {
          try {
            return { ...item, message: JSON.parse(item.text) };
          } catch {
            return null;
          }
        })
        .filter(Boolean)
        .find(
          (item) =>
            item.direction === 'outbound' &&
            item.message.type === 'command' &&
            item.message.body?.command === 'session.prompt' &&
            item.message.body?.sessionId === expectedSessionId,
        );
      return entry?.message.body ?? null;
    }, 'outbound session.prompt', 30_000);
    const requestId = command.requestId;
    await wait(() => {
      const result = window.__acpCreateCleanupLog.entries
        .slice(entryFloor)
        .map((item) => {
          try {
            return { ...item, message: JSON.parse(item.text) };
          } catch {
            return null;
          }
        })
        .filter(Boolean)
        .find(
          (item) =>
            item.direction === 'inbound' &&
            item.message.type === 'command.result' &&
            item.message.body?.requestId === requestId &&
            item.message.body?.command === 'session.prompt' &&
            item.message.body?.status === 'completed',
        );
      return result?.message.body ?? null;
    }, `completed result for session.prompt ${requestId}`, 120_000);
    await wait(() => {
      const terminal = window.__acpCreateCleanupLog.entries
        .slice(entryFloor)
        .map((item) => {
          try {
            return { ...item, message: JSON.parse(item.text) };
          } catch {
            return null;
          }
        })
        .filter(Boolean)
        .find(
          (item) =>
            item.direction === 'inbound' &&
            item.message.type === 'event' &&
            item.message.body?.eventType === 'command.completed' &&
            item.message.body?.payload?.view?.requestId === requestId,
        );
      return terminal?.message.body ?? null;
    }, `command.completed event for session.prompt ${requestId}`, 120_000);
    return {
      requestId,
      sessionId: command.sessionId,
      text: text.split(': ').at(-1),
    };
  })();
}

function hideViewerFlow() {
  return (async () => {
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
    const entryFloor = window.__acpCreateCleanupLog.entries.length;
    Object.defineProperty(document, 'visibilityState', {
      configurable: true,
      get: () => 'hidden',
    });
    document.dispatchEvent(new Event('visibilitychange'));
    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
      const log = window.__acpCreateCleanupLog;
      const entries = log?.entries
        ?.slice(entryFloor)
        .map((entry) => {
          try {
            return { ...entry, message: JSON.parse(entry.text) };
          } catch {
            return null;
          }
        })
        .filter(Boolean) ?? [];
      const release = entries.find(
          (entry) =>
            entry.direction === 'outbound' &&
            entry.message.type === 'session.view' &&
            entry.message.body?.sessionId === null,
        );
      if (release) {
        const accepted = entries.find(
          (entry) =>
            entry.direction === 'inbound' &&
            entry.message.type === 'session.view.result' &&
            entry.message.body?.sessionId === null &&
            entry.message.body?.viewVersion === release.message.body?.viewVersion &&
            entry.message.body?.status === 'accepted',
        );
        if (accepted) {
          return {
            viewVersion: release.message.body.viewVersion,
            acceptedAt: accepted.at,
          };
        }
      }
      await sleep(30);
    }
    throw new Error('timeout waiting for accepted hidden viewer release');
  })();
}

function showViewerFlow() {
  delete document.visibilityState;
  document.dispatchEvent(new Event('visibilitychange'));
  return document.visibilityState;
}

function settleAnimationFramesFlow() {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(resolve));
  });
}
