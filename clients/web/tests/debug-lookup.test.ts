import { describe, expect, it } from "vitest";

import { parseLookupResponse } from "../src/protocol/pairing";

describe("temporary lookup reproduction", () => {
  it("parses the observed node response", async () => {
    const body = new TextEncoder().encode(
      JSON.stringify({
        v: 1,
        pairingId: "xSlEWx1w7tcEf5Az34i_xg",
        nodeId:
          "a348d753f1d18311ca7875492cb66ed1b70a64d2a80af1e4db0d6f761f3bc858",
        nodePublicKey:
          "BDo4VAx6420TYh0nO2_xyQ-uMxtA9LWrsOjo02uMifpN3qopnp1OE8s-xly1PtgQ0Wt-PZ03yLg5Ja_0zr9slPk",
        origin: "https://localhost:28444",
        requestId: "1VZ_pWt5RsBs1uTOa8nBeg",
        expiresAt: 1793269460,
        challengeId: "Hr2ahPLcvoGtaUEGrA8Ovg",
        nonce: "yrM1sTGDrozGXA114Tv5vsC_uoR0mfxfIXmEDvUxNhQ",
        iat: 1793269251,
        exp: 1793269281,
      }),
    );
    await expect(
      parseLookupResponse(
        body,
        "https://localhost:28444",
        "1VZ_pWt5RsBs1uTOa8nBeg",
      ),
    ).resolves.toMatchObject({ pairingId: "xSlEWx1w7tcEf5Az34i_xg" });
  });
});
