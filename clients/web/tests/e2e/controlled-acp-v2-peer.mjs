import { createInterface } from "node:readline";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import path from "node:path";

function parseArgs(argv) {
  let stateDir = process.cwd();
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--state-dir" && i + 1 < argv.length) {
      stateDir = argv[i + 1];
      i += 1;
    }
  }
  mkdirSync(stateDir, { recursive: true });
  return stateDir;
}

const stateDir = parseArgs(process.argv.slice(2));
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

let replayHash;
function emit(payload) {
  const line = `${JSON.stringify(payload)}\n`;
  if (payload.method === "session/update") replayHash?.update(line);
  process.stdout.write(line);
}

function appendEvent(entry) {
  const file = path.join(stateDir, "peer-events.ndjson");
  const prev = existsSync(file) ? readFileSync(file, "utf8") : "";
  writeFileSync(file, `${prev}${JSON.stringify(entry)}\n`, "utf8");
}

let sessionCounter = 0;
let activeSessionId = "e2e-session-1";
const pendingPrompts = [];
let processing = false;
let permissionCounter = 0;
let promptCounter = 0;
const historyTexts = [];

async function waitWhileFlagged(flagName) {
  const flagPath = path.join(stateDir, flagName);
  const deadline = Date.now() + 120_000;
  while (existsSync(flagPath) && Date.now() < deadline) {
    await sleep(25);
  }
}

async function processPromptQueue() {
  if (processing) return;
  processing = true;
  try {
    while (pendingPrompts.length > 0) {
      const req = pendingPrompts.shift();
      promptCounter += 1;
      const sessionId = req.params?.sessionId ?? activeSessionId;
      const rawPrompt = JSON.stringify(req.params?.prompt ?? []);
      appendEvent({ method: "session/prompt", sessionId, rawPrompt });

      emit({
        jsonrpc: "2.0",
        method: "session/update",
        params: {
          sessionId,
          update: {
            sessionUpdate: "state_update",
            state: "running",
          },
        },
      });
      if (req.id !== undefined) {
        emit({
          jsonrpc: "2.0",
          id: req.id,
          result: { messageId: `e2e-inserted-${promptCounter}` },
        });
      }
      const historyAppend = existsSync(path.join(stateDir,"manual-refresh")) && req.params.prompt?.[0]?.text === "HN live user 13";
      if (historyAppend) emit({jsonrpc:"2.0",method:"session/update",params:{sessionId,
        update:{sessionUpdate:"user_message_chunk",messageId:"13",content:{type:"text",text:"HN live user 13"}}}});
      const replyText = historyAppend ? "HN live reply 13" : `E2E回复:${rawPrompt}`;
      historyTexts.push(replyText);
      emit({
        jsonrpc: "2.0",
        method: "session/update",
        params: {
          sessionId,
          update: {
            sessionUpdate: "agent_message_chunk",
            content: {
              type: "text",
              text: replyText,
            },
          },
        },
      });

      const approvalTrigger = path.join(stateDir, "emit-approval");
      if (existsSync(approvalTrigger)) {
        rmSync(approvalTrigger, { force: true });
        permissionCounter += 1;
        emit({
          jsonrpc: "2.0",
          id: `e2e-perm-${permissionCounter}`,
          method: "session/request_permission",
          params: {
            sessionId,
            title: "E2E Controlled Permission Request",
            subject: {
              type: "tool_call",
              toolCall: { toolCallId: `e2e-tool-${permissionCounter}` },
            },
            options: [
              { optionId: "allow-once", name: "Allow Once", kind: "allow_once" },
              { optionId: "reject-once", name: "Reject Once", kind: "reject_once" },
            ],
          },
        });
      }

      await waitWhileFlagged("hold-prompt");

      emit({
        jsonrpc: "2.0",
        method: "session/update",
        params: {
          sessionId,
          update: {
            sessionUpdate: "state_update",
            state: "idle",
            stopReason: "end_turn",
          },
        },
      });
    }
  } finally {
    processing = false;
  }
}

const rl = createInterface({ input: process.stdin, crlfDelay: Infinity });
rl.on("line", (line) => {
  if (!line.trim()) return;
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    return;
  }
  const method = msg.method;
  const params = msg.params ?? {};

  if (typeof method !== "string") {
    if (String(msg.id ?? "").startsWith("e2e-perm-")) {
      writeFileSync(
        path.join(stateDir, "approval-reply.json"),
        JSON.stringify(msg, null, 2),
        "utf8",
      );
      appendEvent({ method: "permission/response", response: msg });
    }
    return;
  }

  appendEvent({ method, sessionId: params.sessionId ?? null });

  switch (method) {
    case "initialize": {
      emit({
        jsonrpc: "2.0",
        id: msg.id,
        result: {
          protocolVersion: 2,
          info: { name: "sct-016-controlled-acp-v2", version: "1.0.0" },
          capabilities: { session: {} },
        },
      });
      break;
    }
    case "session/new": {
      sessionCounter += 1;
      historyTexts.length = 0;
      activeSessionId = `e2e-acp-session-${process.pid}-${sessionCounter}`;
      emit({
        jsonrpc: "2.0",
        method: "session/update",
        params: {
          sessionId: activeSessionId,
          update: {
            sessionUpdate: "state_update",
            state: "idle",
          },
        },
      });
      emit({
        jsonrpc: "2.0",
        id: msg.id,
        result: { sessionId: activeSessionId },
      });
      break;
    }
    case "session/list": {
      emit({
        jsonrpc: "2.0",
        id: msg.id,
        result: {
          sessions: [{ sessionId: activeSessionId, title: "E2E Session" }],
        },
      });
      break;
    }
    case "session/load":
    case "session/resume": {
      const sessionId = params.sessionId ?? activeSessionId;
      if (existsSync(path.join(stateDir,"manual-refresh")) && method === "session/resume" && params.replayFrom === undefined) {
        emit({jsonrpc:"2.0",method:"session/update",params:{sessionId,update:{sessionUpdate:"state_update",state:"idle"}}});
        emit({jsonrpc:"2.0",id:msg.id,result:{sessionId}});
        break;
      }
      if (existsSync(path.join(stateDir, "history-navigation"))) {
        replayHash = createHash("sha256");
        const version = existsSync(path.join(stateDir, "changed")) ? "new" : "original";
        const update = (sessionUpdate, text, id) => emit({
          jsonrpc: "2.0", method: "session/update",
          params: { sessionId, update: { sessionUpdate, messageId: id, content: { type: "text", text } } },
        });
        update("agent_message_chunk", "HN prelude", "prelude");
        const manualRefresh = existsSync(path.join(stateDir, "manual-refresh"));
        const editFile = path.join(stateDir, "edit-twelfth");
        const edited = existsSync(editFile) && readFileSync(editFile, "utf8") === "1";
        const firstNative = path.join(stateDir,"first-native");
        const otherSession = existsSync(firstNative) && sessionId !== readFileSync(firstNative,"utf8");
        const defaultPrefix = otherSession ? "other target" : version;
        let twelfthPrefix = defaultPrefix;
        if (manualRefresh && edited && !otherSession) twelfthPrefix = "edited";
        if (existsSync(path.join(stateDir,"failed-attempt"))) twelfthPrefix = "failed attempt";
        for (let turn = 1; turn <= (sessionId === "child-history" || manualRefresh ? 12 : 25); turn++) {
          const prefix = turn === 12 ? twelfthPrefix : defaultPrefix;
          update("user_message_chunk", `HN ${prefix} turn ${turn}`, String(turn));
        }
      }
      const entries = historyTexts.length > 0 ? historyTexts : ["E2E重放历史输出"];
      for (const text of entries) {
        emit({
          jsonrpc: "2.0",
          method: "session/update",
          params: {
            sessionId,
            update: {
              sessionUpdate: "agent_message_chunk",
              content: { type: "text", text },
            },
          },
        });
      }
      if (params.replayFrom !== undefined) {
        emit({
          jsonrpc: "2.0",
          method: "session/update",
          params: {
            sessionId,
            update: { sessionUpdate: "replay_complete" },
          },
        });
      }
      emit({
        jsonrpc: "2.0",
        method: "session/update",
        params: {
          sessionId,
          update: { sessionUpdate: "state_update", state: processing ? "running" : "idle" },
        },
      });
      if (replayHash) {
        appendEvent({method:"fixture/replay",sessionId,digest:replayHash.digest("hex")});
        replayHash = undefined;
      }
      emit({
        jsonrpc: "2.0",
        id: msg.id,
        result: { sessionId },
      });
      break;
    }
    case "session/prompt": {
      pendingPrompts.push(msg);
      void processPromptQueue();
      break;
    }
    case "session/cancel": {
      emit({
        jsonrpc: "2.0",
        method: "session/update",
        params: {
          sessionId: params.sessionId ?? activeSessionId,
          update: { sessionUpdate: "state_update", state: "idle", stopReason: "cancelled" },
        },
      });
      if (msg.id !== undefined) {
        emit({ jsonrpc: "2.0", id: msg.id, result: {} });
      }
      break;
    }
    case "session/close": {
      emit({ jsonrpc: "2.0", id: msg.id, result: {} });
      break;
    }
    default: {
      if (msg.id !== undefined) {
        emit({
          jsonrpc: "2.0",
          id: msg.id,
          error: { code: -32601, message: `Unsupported method: ${method}` },
        });
      }
    }
  }
});
