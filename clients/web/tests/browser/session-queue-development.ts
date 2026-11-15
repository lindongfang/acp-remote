import "../../src/styles.css";

import { sessionQueueSnapshotView } from "../../src/management/session-queue-view";
import { SessionResumeCoordinator } from "../../src/management/session-resume";
import type {
  SessionPriorityFailure,
  SessionQueueEntry,
  SessionQueueSnapshot,
  SessionStopReason,
} from "../../src/management/session-protocol";

const TARGET = "AAAAAAAAAAAAAAAAAAAAAA";
const X = "BBBBBBBBBBBBBBBBBBBBBB";
const Z = "CCCCCCCCCCCCCCCCCCCCCC";
const REGISTRATION = "DDDDDDDDDDDDDDDDDDDDDD";
const CONSUME = "EEEEEEEEEEEEEEEEEEEEEE";
const root = document.querySelector<HTMLDivElement>("#queue-root");

if (root === null) {
  throw new Error("queue root is missing");
}

const suspendedEntry = (itemId: string): SessionQueueEntry =>
  Object.freeze({
    itemId,
    revision: "7",
    submitterId: X,
    editorId: Z,
    suspended: true,
  });

function snapshot(
  overrides: Partial<SessionQueueSnapshot> = {},
): SessionQueueSnapshot {
  return Object.freeze({
    targetId: TARGET,
    failureSuspended: true,
    stopReason: null,
    priorityFailure: null,
    waiting: Object.freeze([suspendedEntry(X), suspendedEntry(Z)]),
    active: null,
    priority: null,
    processing: null,
    ...overrides,
  });
}

let operationStatus: string | null = null;

function status(text: string, tone: "notice" | "error"): HTMLElement {
  const line = document.createElement("p");
  line.className = `status-line status-${tone}`;
  line.setAttribute("role", tone === "error" ? "alert" : "status");
  const marker = document.createElement("span");
  marker.className = "status-marker";
  marker.setAttribute("aria-hidden", "true");
  line.append(marker, document.createTextNode(text));
  return line;
}

function show(value: SessionQueueSnapshot): void {
  root!.replaceChildren();
  if (operationStatus !== null) {
    root!.append(
      status(
        operationStatus,
        operationStatus.startsWith("failed") ? "error" : "notice",
      ),
    );
  }
  root!.append(sessionQueueSnapshotView(value));
}

function waitingText(): readonly string[] {
  return [X, Z].map(
    (itemId) =>
      document.querySelector<HTMLElement>(
        `[data-queue-entry="${itemId}"]`,
      )?.textContent ?? "",
  );
}

const coordinator = new SessionResumeCoordinator();
let registrationCalls: readonly (readonly [string, string])[] = [];
let consumeCalls: readonly (readonly [string, string, string])[] = [];
let generatedIds = 0;
let consumeAttempts = 0;

async function renderResumeRetry(): Promise<void> {
  coordinator.clear();
  registrationCalls = [];
  consumeCalls = [];
  generatedIds = 0;
  consumeAttempts = 0;
  operationStatus = null;
  const value = snapshot({ priorityFailure: "selection_changed" });

  const render = (): void => {
    root!.replaceChildren();
    if (operationStatus !== null) {
      root!.append(
        status(
          operationStatus,
          operationStatus.startsWith("failed") ? "error" : "notice",
        ),
      );
    }
    root!.append(
      sessionQueueSnapshotView(value, {
        onResume: async (button) => {
          try {
            const result = await coordinator.run(
              value.targetId,
              () => {
                ++generatedIds;
                return {
                  registrationId: REGISTRATION,
                  consumeRequestId: CONSUME,
                };
              },
              async (targetId, registrationId) => {
                registrationCalls = Object.freeze([
                  ...registrationCalls,
                  Object.freeze([targetId, registrationId] as const),
                ]);
              },
              async (targetId, registrationId, consumeRequestId) => {
                consumeCalls = Object.freeze([
                  ...consumeCalls,
                  Object.freeze([
                    targetId,
                    registrationId,
                    consumeRequestId,
                  ] as const),
                ]);
                ++consumeAttempts;
                if (consumeAttempts === 1) {
                  throw new Error("simulated transport failure");
                }
                return Object.freeze({
                  registrationId,
                  restored: "2",
                  returned: false,
                });
              },
            );
            operationStatus = `succeeded:${result.restored}`;
          } catch {
            operationStatus = "failed:transport";
            button.disabled = false;
          }
          render();
        },
      }),
    );
  };
  render();
  await Promise.resolve();
}

const development = {
  async showStopReason(reason: SessionStopReason): Promise<{
    readonly text: string;
    readonly waiting: readonly string[];
  }> {
    coordinator.clear();
    operationStatus = null;
    show(snapshot({ stopReason: reason }));
    await Promise.resolve();
    return {
      text: root!.textContent ?? "",
      waiting: waitingText(),
    };
  },
  async showPriorityFailure(reason: SessionPriorityFailure): Promise<{
    readonly text: string;
    readonly waiting: readonly string[];
  }> {
    coordinator.clear();
    operationStatus = null;
    show(snapshot({ priorityFailure: reason }));
    await Promise.resolve();
    return {
      text: root!.textContent ?? "",
      waiting: waitingText(),
    };
  },
  async showResumeRetry(): Promise<void> {
    await renderResumeRetry();
  },
  resumeState(): {
    readonly generatedIds: number;
    readonly registrationCalls: typeof registrationCalls;
    readonly consumeCalls: typeof consumeCalls;
    readonly consumeAttempts: number;
    readonly operationStatus: string | null;
  } {
    return {
      generatedIds,
      registrationCalls,
      consumeCalls,
      consumeAttempts,
      operationStatus,
    };
  },
};

declare global {
  interface Window {
    sessionQueueDevelopment: typeof development;
  }
}

window.sessionQueueDevelopment = development;
