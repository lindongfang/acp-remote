/**
 * 附件帧、类型化媒体、精确表单值与上传闸门的针对性单测。
 *
 * 这些用例不启动网络：附件传输的状态机本身用假 `send` 驱动（见 `attachment-transfer` 的类型），
 * 这里只钉住「解码严格性」与「状态/值语义」这两类最容易悄悄退化的性质。
 */

import { describe, expect, it } from "vitest";

import {
  ATTACHMENT_CHUNK_SIZE,
  decodeAttachmentChunk,
  decodeAttachmentFrame,
  encodeAttachmentChunk,
} from "../protocol/attachment";
import { decodeEffectiveInteraction, decodeMediaReference, decodePartialResult } from "../protocol/media";
import { JsonNumberLiteral } from "../protocol/json-number";
import { stringifyJson } from "../sync-client/json";
import { Sha256 } from "../sync-client/sha256";
import { manifestDigestOf } from "../sync-client/attachment-transfer";
import { buildElicitationForm, validateElicitationForm } from "../domain/form";
import { buildAttachmentBatch } from "./attachment-model";
import type { DraftAttachment } from "./attachment-model";

const UUID = "0f2b1a9c-6c55-4c40-9d55-3f3a4a4f2f10";
const SHA = "A".repeat(43);

function beginFrame(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    protocolVersion: 1,
    type: "attachment.begin",
    messageId: UUID,
    connectionId: UUID,
    connectionSequence: "1",
    body: {
      transferId: UUID,
      sessionId: UUID,
      subject: { kind: "device", id: UUID },
      totalLength: "65537",
      chunkSize: ATTACHMENT_CHUNK_SIZE,
      mediaType: "image/png",
      fileName: "a.png",
      ...overrides,
    },
  };
}

describe("附件帧严格解码", () => {
  it("接受合法 begin/chunk 并原样保留十进制偏移", () => {
    expect(decodeAttachmentFrame(beginFrame()).ok).toBe(true);
    const chunk = decodeAttachmentFrame({
      protocolVersion: 1,
      type: "attachment.chunk",
      messageId: UUID,
      connectionId: UUID,
      connectionSequence: "2",
      body: { transferId: UUID, offset: "0", data: encodeAttachmentChunk(new Uint8Array([1, 2, 3])) },
    });
    expect(chunk.ok).toBe(true);
    if (chunk.ok && chunk.frame.type === "attachment.chunk") {
      expect(chunk.frame.body.offset).toBe("0");
      expect([...decodeAttachmentChunk(chunk.frame.body.data)]).toEqual([1, 2, 3]);
    }
  });

  it("拒绝跨主体、前导零偏移与未知键", () => {
    const node = decodeAttachmentFrame(beginFrame({ subject: { kind: "node", id: UUID } }));
    expect(node.ok).toBe(false);
    if (!node.ok) expect(node.reason).toBe("subject");

    const leading = decodeAttachmentFrame({
      protocolVersion: 1,
      type: "attachment.chunk",
      messageId: UUID,
      connectionId: UUID,
      connectionSequence: "2",
      body: { transferId: UUID, offset: "01", data: "AQ" },
    });
    expect(leading.ok).toBe(false);
    if (!leading.ok) expect(leading.reason).toBe("decimal");

    const unknown = decodeAttachmentFrame(beginFrame({ extra: true }));
    expect(unknown.ok).toBe(false);
  });
});

describe("类型化媒体与有效交互", () => {
  it("媒体引用与部分结果按封闭词表解码", () => {
    expect(
      decodeMediaReference({
        attachmentId: UUID,
        sessionId: UUID,
        kind: "file",
        mediaType: "application/pdf",
        byteLength: "12",
        sha256: SHA,
        displayName: "a.pdf",
        availability: "available",
      }).ok,
    ).toBe(true);
    expect(decodePartialResult({ state: "partial", reason: "agent_stopped" }).ok).toBe(true);
    expect(decodePartialResult({ state: "partial", reason: null }).ok).toBe(false);
    expect(decodePartialResult({ state: "completed", reason: "agent_stopped" }).ok).toBe(false);
  });

  it("有效交互必须带 requestVersion 与完整触发字段", () => {
    const ok = decodeEffectiveInteraction({
      interactionId: UUID,
      sessionId: UUID,
      kind: "elicitation",
      requestVersion: "7",
      expiresAt: null,
      options: [],
      schema: { type: "object", properties: {} },
    });
    expect(ok.ok).toBe(true);
    const missing = decodeEffectiveInteraction({
      interactionId: UUID,
      sessionId: UUID,
      kind: "permission",
      expiresAt: null,
      options: [],
      schema: null,
    });
    expect(missing.ok).toBe(false);
  });
});

describe("表单精确整数与整体不可用", () => {
  it("整数不经 Number：序列化为裸数字原文", () => {
    const form = buildElicitationForm({
      type: "object",
      required: ["count"],
      properties: { count: { type: "integer", minimum: 0 } },
    });
    expect(form.kind).toBe("form");
    const validated = validateElicitationForm(form, { count: "9007199254740993" });
    expect(validated.ok).toBe(true);
    if (!validated.ok) return;
    const literal = validated.values["count"];
    expect(literal).toBeInstanceOf(JsonNumberLiteral);
    expect(stringifyJson({ values: validated.values })).toBe('{"values":{"count":9007199254740993}}');
  });

  it("未知必需类型使整表不可提交；非法值给出字段错误", () => {
    const unsupported = buildElicitationForm({
      type: "object",
      required: ["nested"],
      properties: { nested: { type: "object", properties: {} } },
    });
    expect(unsupported.kind).toBe("unavailable");

    const form = buildElicitationForm({
      type: "object",
      required: ["count"],
      properties: { count: { type: "integer", minimum: 2 } },
    });
    const invalid = validateElicitationForm(form, { count: "1" });
    expect(invalid.ok).toBe(false);
    if (!invalid.ok) expect(invalid.errors["count"]).toContain("不能小于");
  });

  it("整数 `-0` 报「必须是整数」而不是让构造器抛未捕获异常", () => {
    const form = buildElicitationForm({
      type: "object",
      required: ["count"],
      properties: { count: { type: "integer" } },
    });
    expect(() => validateElicitationForm(form, { count: "-0" })).not.toThrow();
    const invalid = validateElicitationForm(form, { count: "-0" });
    expect(invalid.ok).toBe(false);
    if (!invalid.ok) expect(invalid.errors["count"]).toBe("必须是整数。");
    // 正常整数不受影响（含 0 与负数的合法写法）。
    for (const text of ["0", "-1", "42"]) {
      const valid = validateElicitationForm(form, { count: text });
      expect(valid.ok, text).toBe(true);
    }
  });

  it("number 字段回传 JSON 数字（不是字符串），范围校验保持", () => {
    const form = buildElicitationForm({
      type: "object",
      required: ["ratio"],
      properties: { ratio: { type: "number", minimum: 0.5, maximum: 2.5 } },
    });
    const valid = validateElicitationForm(form, { ratio: "1.5" });
    expect(valid.ok).toBe(true);
    if (valid.ok) {
      expect(valid.values["ratio"]).toBe(1.5);
      expect(typeof valid.values["ratio"]).toBe("number");
      // 落 wire 时就是裸 JSON 数字，与整数分支同一口径。
      expect(stringifyJson({ values: valid.values })).toBe('{"values":{"ratio":1.5}}');
    }
    const tooSmall = validateElicitationForm(form, { ratio: "0.25" });
    expect(tooSmall.ok).toBe(false);
    if (!tooSmall.ok) expect(tooSmall.errors["ratio"]).toContain("不能小于 0.5");
    const tooLarge = validateElicitationForm(form, { ratio: "9" });
    expect(tooLarge.ok).toBe(false);
    if (!tooLarge.ok) expect(tooLarge.errors["ratio"]).toContain("不能大于 2.5");
  });

  it("整数边界含小数时不抛错：无小数边界仍精确比较", () => {
    const fractional = buildElicitationForm({
      type: "object",
      required: ["count"],
      properties: { count: { type: "integer", minimum: 1.5, maximum: 4.5 } },
    });
    expect(() => validateElicitationForm(fractional, { count: "1" })).not.toThrow();
    const below = validateElicitationForm(fractional, { count: "1" });
    expect(below.ok).toBe(false);
    if (!below.ok) expect(below.errors["count"]).toContain("不能小于 1.5");
    const above = validateElicitationForm(fractional, { count: "5" });
    expect(above.ok).toBe(false);
    if (!above.ok) expect(above.errors["count"]).toContain("不能大于 4.5");
    const inside = validateElicitationForm(fractional, { count: "2" });
    expect(inside.ok).toBe(true);

    // 无小数边界走精确整数比较：值不经 Number，超过 safe integer 的相邻整数仍能区分
    // （边界用十进制文本给出，避免测试字面量本身在 JS 里先被舍入）。
    const exact = buildElicitationForm({
      type: "object",
      required: ["count"],
      properties: { count: { type: "integer", minimum: "9007199254740993" } },
    });
    const boundary = validateElicitationForm(exact, { count: "9007199254740993" });
    expect(boundary.ok).toBe(true);
    const under = validateElicitationForm(exact, { count: "9007199254740992" });
    expect(under.ok).toBe(false);
    if (!under.ok) expect(under.errors["count"]).toContain("不能小于");
  });

  it("单选与多选只接受原请求里的取值，多选按 minItems/maxItems 校验", () => {
    const form = buildElicitationForm({
      type: "object",
      required: ["mode", "tags"],
      properties: {
        mode: { type: "string", enum: ["fast", "safe"], title: "模式" },
        tags: { type: "array", items: { type: "string", enum: ["a", "b", "c"] }, minItems: 1, maxItems: 2 },
      },
    });
    expect(form.kind).toBe("form");
    if (form.kind !== "form") return;
    expect(form.fields.map((field) => field.kind)).toEqual(["enum", "multi_enum"]);

    // 合法：单选一个、多选两个。
    const valid = validateElicitationForm(form, { mode: "fast", tags: ["a", "c"] });
    expect(valid.ok).toBe(true);
    if (valid.ok) expect(valid.values).toEqual({ mode: "fast", tags: ["a", "c"] });

    // 单选不在枚举里 + 多选超过上限：逐字段报错，不消耗请求。
    const invalid = validateElicitationForm(form, { mode: "turbo", tags: ["a", "b", "c"] });
    expect(invalid.ok).toBe(false);
    if (!invalid.ok) {
      expect(invalid.errors["mode"]).toContain("不是这张表单允许的取值");
      expect(invalid.errors["tags"]).toContain("至多");
    }

    // 必填的多选为空：同样拒绝（不把空数组当成有效答案）。
    const empty = validateElicitationForm(form, { mode: "safe", tags: [] });
    expect(empty.ok).toBe(false);
    if (!empty.ok) expect(empty.errors["tags"]).toContain("必填");
  });
});

describe("附件闸门", () => {
  const attachment = (status: DraftAttachment["status"]): DraftAttachment => ({
    localId: UUID,
    transferId: UUID,
    attachmentId: null,
    kind: "image",
    fileName: "a.png",
    mediaType: "image/png",
    byteLength: "10",
    status,
    uploadedBytes: status === "sealed" ? "10" : "0",
    sha256: status === "sealed" ? SHA : null,
    error: status === "failed" ? "网络中断" : null,
    previewUrl: null,
  });

  it("选择阶段可以点发送，上传中与失败都挡住", () => {
    const capability = () => ({ kind: "supported" as const });
    expect(buildAttachmentBatch({ attachments: [attachment("selected")], capabilityOf: capability }).canSubmit).toBe(true);
    expect(buildAttachmentBatch({ attachments: [attachment("uploading")], capabilityOf: capability }).canSubmit).toBe(false);
    expect(buildAttachmentBatch({ attachments: [attachment("failed")], capabilityOf: capability }).canSubmit).toBe(false);
    expect(buildAttachmentBatch({ attachments: [attachment("sealed")], capabilityOf: capability }).canSubmit).toBe(true);
  });

  it("Agent 未宣告图片能力时整条不可提交", () => {
    const batch = buildAttachmentBatch({
      attachments: [attachment("selected")],
      capabilityOf: () => ({ kind: "unsupported", reason: "不支持图片" }),
    });
    expect(batch.canSubmit).toBe(false);
    expect(batch.blockedReason).toContain("不支持图片");
  });
});

describe("清单摘要", () => {
  it("按 u32be(len(transferId)) || transferId 的顺序复算", () => {
    const ids = [
      "0f2b1a9c-6c55-4c40-9d55-3f3a4a4f2f10",
      "3e1d2c3b-4a59-4687-9a01-2b3c4d5e6f70",
    ];
    // 独立向量（Node 的 SHA-256 对同样的字节序列计算）。
    expect(manifestDigestOf(ids)).toBe("U975O3X5_LmBKTZB-jtITkih7mhvtbnyQLKe2TAY2Qw");
    expect(manifestDigestOf([])).toBe(
      "47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU",
    );
  });
});

describe("流式 SHA-256", () => {
  it("与标准向量一致，且分块大小不影响结果", () => {
    const encode = (value: string): Uint8Array => new TextEncoder().encode(value);
    const hex = (bytes: Uint8Array): string =>
      [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
    expect(hex(new Sha256().update(encode("")).digest())).toBe(
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
    const once = new Sha256().update(encode("abc")).digest();
    const split = new Sha256().update(encode("a")).update(encode("bc")).digest();
    expect(hex(once)).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    expect([...split]).toEqual([...once]);
  });
});
