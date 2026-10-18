/**
 * 配对码输入格的编辑语义（WP4 review P2）。
 *
 * ## 判别力从哪来
 *
 * 断言的是**格数组本身**，不是「能提交」这类弱信号：
 *
 * - 编辑一个已经填过的格（`selectTextOnFocus` 让输入变成单字符）只改该格，其余格保持原值——
 *   回退成「重建全部格」会让这里的前几位变成空串；
 * - 一次多字符变更（粘贴、或没有选中时的追加）从**被编辑的格**开始写入，起点之前的格不动——
 *   回退成「从格 0 重建」会让这里的起点之前的字符被覆盖；
 * - 只有整码（8 位 / `XXXX-XXXX`）才从头填满 8 格，这是「粘贴整码」的唯一合法形态。
 */

import { describe, expect, it } from "vitest";

import { codeCellsAfterEdit } from "./PairingPanel";

/** 8 个格，按字符顺序填入；`_` 表示空格。 */
function cellsOf(text: string): readonly string[] {
  return Array.from({ length: 8 }, (_, index) => {
    const character = text.charAt(index);
    return character === "_" ? "" : character;
  });
}

describe("配对码输入格的变更", () => {
  it("编辑已有字符的一格：只替换该格，其余格保持不变", () => {
    // 反例：多字符分支从格 0 重建 → 第 0–1 位变成 `D`/`X`，后面六位被清空。
    expect(codeCellsAfterEdit(cellsOf("12345678"), 3, "X")).toEqual(cellsOf("123X5678"));
  });

  it("删除一格只清空该格，其余格保持不变", () => {
    expect(codeCellsAfterEdit(cellsOf("12345678"), 5, "")).toEqual(cellsOf("12345_78"));
  });

  it("多字符变更从被编辑的格开始写入，起点之前的格保持原值", () => {
    // 没有 selectTextOnFocus 时输入第 4 位会得到 `D` + `X`：这段必须只覆盖第 4、5 位。
    expect(codeCellsAfterEdit(cellsOf("12345678"), 3, "AB")).toEqual(cellsOf("123AB678"));
  });

  it("多字符变更写不进的格保持原值（不因为变更短就清空后面的格）", () => {
    expect(codeCellsAfterEdit(cellsOf("ABCDEFGH"), 6, "XY")).toEqual(cellsOf("ABCDEFXY"));
  });

  it("整码粘贴总是填满全部 8 格，起点无关", () => {
    expect(codeCellsAfterEdit(cellsOf("12345678"), 4, "ABCDEFGH")).toEqual(cellsOf("ABCDEFGH"));
    expect(codeCellsAfterEdit(cellsOf("________"), 0, "ABCDEFGH")).toEqual(cellsOf("ABCDEFGH"));
  });

  it("空状态粘贴短码：只写前几格，其余保持空格", () => {
    expect(codeCellsAfterEdit(cellsOf("________"), 0, "AB")).toEqual(cellsOf("AB______"));
  });
});
