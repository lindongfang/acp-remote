import { useCallback, useEffect, useRef } from "react";
import { View, StyleSheet, webStyle } from "./theme";

/** 栏间拖动柄：指针捕获保证拖出柄后仍能调整，事件监听随组件卸载清理。 */
export function ConversationColumnResizer({ side, width, maxWidth, onChange }: {
  readonly side: "left" | "right";
  readonly width: number;
  readonly maxWidth: number;
  readonly onChange: (width: number) => void;
}) {
  const nodeRef = useRef<HTMLElement | null>(null);
  const current = useRef({ width, maxWidth, onChange });
  current.current = { width, maxWidth, onChange };
  const assignRef = useCallback((node: unknown) => { nodeRef.current = node as HTMLElement | null; }, []);
  useEffect(() => {
    const node = nodeRef.current;
    if (node === null) return;
    let drag: { pointerId: number; startX: number; width: number } | null = null;
    const minimum = side === "left" ? 160 : 200;
    const commit = (value: number) => {
      current.current.onChange(Math.max(minimum, Math.min(value, current.current.maxWidth)));
    };
    const down = (event: PointerEvent) => {
      if (event.button !== 0) return;
      event.preventDefault();
      node.focus();
      node.setPointerCapture(event.pointerId);
      drag = { pointerId: event.pointerId, startX: event.clientX, width: current.current.width };
    };
    const move = (event: PointerEvent) => {
      if (drag === null || drag.pointerId !== event.pointerId) return;
      commit(drag.width + (event.clientX - drag.startX) * (side === "left" ? 1 : -1));
    };
    const end = () => { drag = null; };
    const key = (event: KeyboardEvent) => {
      const direction = side === "left" ? 1 : -1;
      if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
        event.preventDefault();
        commit(current.current.width + (event.key === "ArrowRight" ? 20 : -20) * direction);
      } else if (event.key === "Home" || event.key === "End") {
        event.preventDefault();
        commit(event.key === "Home" ? minimum : current.current.maxWidth);
      }
    };
    node.addEventListener("pointerdown", down);
    node.addEventListener("pointermove", move);
    node.addEventListener("pointerup", end);
    node.addEventListener("lostpointercapture", end);
    node.addEventListener("pointercancel", end);
    node.addEventListener("keydown", key);
    return () => {
      node.removeEventListener("pointerdown", down);
      node.removeEventListener("pointermove", move);
      node.removeEventListener("pointerup", end);
      node.removeEventListener("lostpointercapture", end);
      node.removeEventListener("pointercancel", end);
      node.removeEventListener("keydown", key);
    };
  }, [side]);
  return <View ref={assignRef} role="separator" tabIndex={0}
    accessibilityLabel={side === "left" ? "调整左栏宽度" : "调整右栏宽度"}
    aria-orientation="vertical" aria-valuemin={side === "left" ? 160 : 200}
    aria-valuemax={maxWidth} aria-valuenow={Math.round(width)}
    dataSet={{ columnResizer: side }} style={styles.handle}>
    <View style={styles.hitArea} />
  </View>;
}

const styles = StyleSheet.create({
  // 可见分隔线由侧栏的 1px 边框呈现；透明命中区跨在线两侧，不占栏宽。
  handle: { width: 0, flexShrink: 0, zIndex: 2, overflow: "visible" },
  hitArea: { position: "absolute", top: 0, bottom: 0, left: -4, width: 8,
    ...webStyle({ cursor: "col-resize", touchAction: "none", userSelect: "none" }) },
});
