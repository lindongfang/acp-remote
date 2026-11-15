import { describe, expect, it } from "vitest";

import { SessionResumeCoordinator } from "../src/management/session-resume";

const target = "AAAAAAAAAAAAAAAAAAAAAA";
const otherTarget = "BBBBBBBBBBBBBBBBBBBBBB";
const firstRegistration = "CCCCCCCCCCCCCCCCCCCCCC";
const firstConsume = "DDDDDDDDDDDDDDDDDDDDDD";
const secondRegistration = "EEEEEEEEEEEEEEEEEEEEEE";
const secondConsume = "FFFFFFFFFFFFFFFFFFFFFF";

describe("explicit resume operation identity", () => {
  it("reuses exactly the same registration and consume identity after failure", async () => {
    const coordinator = new SessionResumeCoordinator();
    const registrations: string[] = [];
    const consumes: string[] = [];
    let generated = 0;
    const ids = () => {
      ++generated;
      return {
        registrationId: firstRegistration,
        consumeRequestId: firstConsume,
      };
    };
    let failedOnce = false;

    await expect(
      coordinator.run(
        target,
        ids,
        async (_target, registrationId) => {
          registrations.push(registrationId);
        },
        async (_target, registrationId, consumeRequestId) => {
          consumes.push(`${registrationId}:${consumeRequestId}`);
          if (!failedOnce) {
            failedOnce = true;
            throw new Error("transport");
          }
          return {
            registrationId,
            restored: "3",
            returned: false,
          };
        },
      ),
    ).rejects.toThrow("transport");

    await expect(
      coordinator.run(
        target,
        ids,
        async (_target, registrationId) => {
          registrations.push(registrationId);
        },
        async (_target, registrationId, consumeRequestId) => {
          consumes.push(`${registrationId}:${consumeRequestId}`);
          return {
            registrationId,
            restored: "3",
            returned: false,
          };
        },
      ),
    ).resolves.toEqual({
      registrationId: firstRegistration,
      restored: "3",
      returned: false,
    });

    expect(generated).toBe(1);
    expect(registrations).toEqual([firstRegistration, firstRegistration]);
    expect(consumes).toEqual([
      `${firstRegistration}:${firstConsume}`,
      `${firstRegistration}:${firstConsume}`,
    ]);
  });

  it("creates a new identity after success and when the target changes", async () => {
    const coordinator = new SessionResumeCoordinator();
    const generated = [
      {
        registrationId: firstRegistration,
        consumeRequestId: firstConsume,
      },
      {
        registrationId: secondRegistration,
        consumeRequestId: secondConsume,
      },
    ];
    const run = (targetId: string) =>
      coordinator.run(
        targetId,
        () => {
          const ids = generated.shift();
          if (ids === undefined) throw new Error("id source exhausted");
          return ids;
        },
        async () => undefined,
        async (_target, registrationId) => ({
          registrationId,
          restored: "1",
          returned: false,
        }),
      );

    await expect(run(target)).resolves.toMatchObject({
      registrationId: firstRegistration,
    });
    await expect(run(otherTarget)).resolves.toMatchObject({
      registrationId: secondRegistration,
    });
    expect(generated).toHaveLength(0);
  });
});
