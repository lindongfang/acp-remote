import { describe, expect, it } from "vitest";
import { buildAgentMessageContent } from "./media";

describe("消息文本片段还原", () => {
  it("完成后读回的单词片段与流式正文一致，保留原有空格和换行", () => {
    const fragments = ["The", " user", " says", " 你好", ".\n\n", "Short", " reply", " is appropriate."];
    const content = buildAgentMessageContent(fragments.map((text) => ({ type: "text" as const, text })));
    expect(content.text).toBe("The user says 你好.\n\nShort reply is appropriate.");
  });
});
