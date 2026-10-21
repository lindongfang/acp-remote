import { createElement } from "react";
import type { ReactElement } from "react";
import { View, useTheme } from "./theme";
const SHAPES = {
  "DIR": [
    {
      "tag": "path",
      "props": {
        "d": "M1.5 4.5V2.8c0-.4.3-.7.7-.7h3.1l1.3 1.7h6.2c.4 0 .7.3.7.7v8.1c0 .4-.3.7-.7.7H2.2a.7.7 0 0 1-.7-.7V4.5Z",
        "fill": "currentColor"
      }
    }
  ],
  "CODE": [
    {
      "tag": "path",
      "props": {
        "d": "m5.4 4.2-3.6 3.8 3.6 3.8M10.6 4.2l3.6 3.8-3.6 3.8"
      }
    }
  ],
  "CONFIG": [
    {
      "tag": "path",
      "props": {
        "d": "M6.2 3.3H5.6a2 2 0 0 0-2 2v1.1c0 .9-.5 1.5-1.4 1.8.9.3 1.4.9 1.4 1.8v1.1c0 1.1.9 2 2 2h.6M9.8 3.3h.6a2 2 0 0 1 2 2v1.1c0 .9.5 1.5 1.4 1.8-.9.3-1.4.9-1.4 1.8v1.1c0 1.1-.9 2-2 2h-.6"
      }
    }
  ],
  "DOC": [
    {
      "tag": "path",
      "props": {
        "d": "M3 3.5h10M3 6.5h10M3 9.5h10M3 12.5h6"
      }
    }
  ],
  "IMAGE": [
    {
      "tag": "rect",
      "props": {
        "x": "1.8",
        "y": "2.8",
        "width": "12.4",
        "height": "10.4",
        "rx": "2"
      }
    },
    {
      "tag": "circle",
      "props": {
        "cx": "5.2",
        "cy": "6.2",
        "r": "1.1",
        "fill": "currentColor",
        "stroke": "none"
      }
    },
    {
      "tag": "path",
      "props": {
        "d": "m2.8 11.6 3.4-3.2 2.4 2.1 1.9-1.7 2.7 2.6"
      }
    }
  ],
  "FILE": [
    {
      "tag": "path",
      "props": {
        "d": "M4 1.8h5.2l3 3V13.4c0 .4-.3.7-.7.7H4.7a.7.7 0 0 1-.7-.7V2.5c0-.4.3-.7.7-.7Z"
      }
    },
    {
      "tag": "path",
      "props": {
        "d": "M9 1.9v3.1h3.1"
      }
    }
  ]
} as const;
/** Safe text and vector rendering shared by the file preview. */
export function FileTypeIcon({ directory = false, path = "" }: { readonly directory?: boolean; readonly path?: string }): ReactElement {
  const { colors } = useTheme();
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const type = directory ? "DIR" : ["ts","tsx","js","jsx","rs"].includes(ext) ? "CODE" : ["json","toml","yaml","yml"].includes(ext) ? "CONFIG" : ["md","txt"].includes(ext) ? "DOC" : ["png","jpg","jpeg","gif","webp","svg","ico"].includes(ext) ? "IMAGE" : "FILE";
  const color = type === "DIR" ? colors.accentInk : type === "CODE" ? colors.tool : type === "CONFIG" ? colors.warn : type === "IMAGE" ? colors.think : type === "DOC" ? colors.ink2 : colors.ink3;
  const backgroundColor = type === "DIR" ? colors.accentSoft : type === "CODE" ? colors.toolSoft : type === "CONFIG" ? colors.warnSoft : type === "IMAGE" ? colors.thinkSoft : colors.line2;
  return <View aria-hidden={true} dataSet={{ fileIcon: directory ? "folder" : "file", fileIconType: type === "DOC" ? "document" : type.toLowerCase() }} style={{ width:22,height:22,flexShrink:0,borderRadius:6,alignItems:"center",justifyContent:"center",backgroundColor }}>
    {createElement("svg", { width:16,height:16,viewBox:"0 0 16 16",fill:"none",stroke:"currentColor",strokeWidth:1.4,strokeLinecap:"round",strokeLinejoin:"round",style:{color},"aria-hidden":true },
      ...SHAPES[type].map((shape,index) => createElement(shape.tag, { ...shape.props, key:index, ...(type === "DIR" ? {fill:colors.panel2} : {}) })))}
  </View>;
}
