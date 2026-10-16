/**
 * 对话页窄屏次级视图：四个取值的互斥与缺省（design D5）。
 *
 * 取值住在 store（跨路由存活），`hiddenConversationColumns` 是「哪个视图隐藏哪些列」的**唯一**
 * 判据；「四列内容始终在 DOM」正是这条映射的结果（未选中的列只是 `display: none`）。
 *
 * 判别力来源：`all` 一列都不隐藏、三个单列取值各只留一列——把 `all` 实现成「隐藏全部」或把
 * 某个单列取值漏掉一列都会让「恰好一列可见」的那条断言变红。
 */

import { describe, expect, it } from "vitest";

import type { ConversationColumnName, ConversationSecondaryView } from "./conversation-model";
import {
  CONVERSATION_SECONDARY_VIEWS,
  DEFAULT_CONVERSATION_SECONDARY_VIEW,
  hiddenConversationColumns,
} from "./conversation-model";
import { initialClientState } from "./client-store";
import { ONLINE_SESSION_ID, resources, session, storeWith } from "./testing/fixtures";

/** 三列的全集（`data-conversation-column` 的三个取值）。 */
const COLUMNS: readonly ConversationColumnName[] = ["sessions", "timeline", "files"];

/** 某个视图下可见的列 = 全集 − 隐藏集。 */
function visibleColumns(view: ConversationSecondaryView): readonly ConversationColumnName[] {
  const hidden = hiddenConversationColumns(view);
  return COLUMNS.filter((column) => !hidden.includes(column));
}

describe("次级视图的四个取值", () => {
  it("登记表恰好是四个互不相同的取值", () => {
    expect(CONVERSATION_SECONDARY_VIEWS).toEqual(["all", "timeline", "sessions", "files"]);
    expect(new Set(CONVERSATION_SECONDARY_VIEWS).size).toBe(4);
    expect(CONVERSATION_SECONDARY_VIEWS).toContain(DEFAULT_CONVERSATION_SECONDARY_VIEW);
  });

  it("缺省是时间线（窄屏进来先看对话本体）", () => {
    expect(DEFAULT_CONVERSATION_SECONDARY_VIEW).toBe("timeline");
    expect(initialClientState().conversationSecondaryView).toBe("timeline");
    expect(storeWith({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }) }).store.state
      .conversationSecondaryView).toBe("timeline");
  });

  it("`all` 不隐藏任何一列", () => {
    expect(hiddenConversationColumns("all")).toEqual([]);
    expect(visibleColumns("all")).toEqual(COLUMNS);
  });

  it("三个单列取值各只留一列：恰好一列可见，其余两列隐藏", () => {
    for (const view of ["timeline", "sessions", "files"] as const) {
      expect(hiddenConversationColumns(view)).toHaveLength(2);
      expect(hiddenConversationColumns(view)).not.toContain(view);
      expect(visibleColumns(view)).toEqual([view]);
    }
  });

  it("写入的取值是唯一生效的那个（同一个字段，四个取值互斥）", () => {
    const { store } = storeWith({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }) });

    for (const view of CONVERSATION_SECONDARY_VIEWS) {
      store.setConversationSecondaryView(view);
      expect(store.state.conversationSecondaryView).toBe(view);
      // 反例：把「次级视图」拆成三个布尔量时，这里会出现第二个取值为真。
      expect(CONVERSATION_SECONDARY_VIEWS.filter((other) => other === view)).toHaveLength(1);
      expect(visibleColumns(store.state.conversationSecondaryView)).toEqual(
        view === "all" ? COLUMNS : [view],
      );
    }
  });

  it("切换次级视图不改动数据判定（模型逐字不变）", () => {
    const { store, gateway } = storeWith({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID, state: "running" })] }),
    });
    const before = store.conversationPage(ONLINE_SESSION_ID);

    store.setConversationSecondaryView("files");
    const after = store.conversationPage(ONLINE_SESSION_ID);

    expect(after).toEqual(before);
    expect(gateway.dispatches).toEqual([]);
  });
});
