/**
 * PW：从「未配对」出发的**真实配对链路**（浏览器级）。
 *
 * 本目录其余的用例都从「已配对」起步（理由见 `../support.tsx` 的文件头）。这里的用例从
 * **未配对**起步，走的是生产代码：二维码地址的片段挂到 `location.hash` 上打开 `/pair` →
 * `app/pair.tsx` → `PairingPanel` 的固定状态页 → `ClientStore.startPairingFromAddress`
 * 读取并**立即清除**片段 → 组合根的 `HttpPairingTransport`
 * （`/sync/v1/pairing/claim` 与 `/sync/v1/pairing/status`）→ 已批准的主机材料写进真实
 * IndexedDB（`paired-host/<origin>`）→ 装配连接并触发连接 → 真实握手进入 `online` →
 * 1.6 秒后 `router.replace("/")` 自动进入目录页。
 *
 * 页面有**两种**入口（design D1）：移动端二维码把载荷放在 `location.hash` 里打开；桌面网页在
 * `/pair` 无 fragment 时显示 8 位配对码输入（S0）。两条入口之后共用同一套 lookup/claim/轮询/
 * 落盘链路。被替身顶掉的只有**传输**两头：配对 HTTPS 面（`fetch`，e2e 环境里 `https://`
 * 来源不可达）与 socket（假主机）。配对状态机、结构校验、落盘、连接装配、握手验签、
 * 快照管线全部是生产代码。
 *
 * ## 判别力：每条断言在实现被回退时都会红
 *
 * - **PW-1** 正向链路的每一跳都有独立锚点：端点路径、认领正文里的设备材料、片段清除、状态查询
 *   正文里的 `pairingRequestId` 与每次都换的 `requestNonce`、pending 阶段的主机地址与 `m:ss`
 *   倒计时、已批准阶段的设备标识、落盘摘要、自动落点 `/`、出站 URL 路径。认领响应由
 *   **二维码里那把**钥匙签（`signPairingClaimHostProof`），因此「验签用错了钥匙/没验」这条
 *   在此**不**会红——那是 PW-3 的位置。
 * - **PW-2 / PW-2b** 两种入口的无载荷行为：`/pair` 无片段必须显示 S0（8 格、origin、HTTPS、
 *   静态提示、无数字倒计时）且一个请求都不发；`/node-link/pair` 无片段仍是无载荷页、不得出现
 *   配对码输入。把 S0 实现成直接认领或显示倒计时、把 node-link 入口也接上配对码，都会红。
 * - **PW-3** / **PW-4** 用假主机**故意**产出被篡改的响应（真 Daemon 无法诚实地产出这种响应，
 *   这正是它们存在的理由）：认领响应换一把钥匙签、已批准响应换一个公钥。两条都断言
 *   **明确失败**（阶段 + 错误码 + 固定标题）、不落盘、不建连接，并断言**停止轮询**——
 *   「没有成功」不构成证据：把验签去掉、把逐字节比对改成宽松比较，两条都会在这些断言上红。
 * - **PW-5** 倒计时归零：`expiresAt` 只比一个轮询周期多一点，假主机永远回「等待确认」。页面必须
 *   先呈现 pending、再由**绝对截止时间**推进到 `expired`（不是固定 300 秒），并且此后不再发出
 *   状态查询。把到期定时器删掉、或以「页面打开时刻 + 5 分钟」计时，这条都会红。
 * - **PW-6 / PW-7 / PW-8** 配对码入口：合法码必须走同源 lookup → 既有 claim/轮询 → 落盘，
 *   且码不进入 URL/Cookie/两种 Web Storage；格式不符必须在发请求**之前**失败；lookup 失败
 *   必须呈现固定文案、不回显后端 message/码、不重试、不落盘。把 lookup 成功后的流程复制成
 *   第二套、把码写进持久面、或把失败原因直接渲染出来，都会在这些断言上红。
 * - **PW-9** 配对码入口（lookup 之后）的倒计时归零：与 PW-5 同一条绝对截止时间路径，但入口
 *   是配对码，因此必须呈现 S6「配对码不对或已过期」（不是二维码入口的 S7「配对地址已过期」），
 *   并停止轮询、不落盘。把过期呈现写死成二维码口径，这条会红。
 * - **PW-10** 编辑一个**已经填过**的格：输入被 RN-web 报告成「原字符 + 新字符」的多字符变更
 *   时，只能从这一格开始覆盖，其余格（尤其前面已输入的位）必须保留。把多字符分支写成从格 0
 *   重建，这条会红。
 */

import {
  AGENTS,
  DIRS_ROUTE,
  NODE_LINK_PAIR_ROUTE,
  PAIR_ROUTE,
  WORKSPACES_WITH_PATHS,
  sessionsForBothDirectories,
} from "../fixtures";
import { createFakeHostIdentity, signPairingClaimHostProof, tamperPublicKeyLastByte } from "../fake-host";
import type { FakeHostIdentity } from "../fake-host";
import { encodeBase64Url } from "../../../src/sync-client";
import { PAIRING_POLL_INTERVAL_MS } from "../../../src/features/pairing-model";
import {
  THEME_PREFERENCE_STORAGE_KEY,
  themePreferenceStore,
} from "../../../src/components/theme";
import { openPlatform } from "../../../src/platform";
import { currentPath } from "../router-stub";
import {
  CANONICAL_ORIGIN,
  HOST_ID,
  click,
  domText,
  expectAbsent,
  expectEqual,
  expectThat,
  mountScenario,
  query,
  queryAll,
  seedDeviceIdentity,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";
import { typeIntoInput } from "./r15-directory";
import type { FetchLike, PairingHttpResponse } from "../../../src/composition";

/**
 * 配对页的锚点选择器。
 *
 * 每次现查而不是缓存元素：容器里的节点会随阶段整棵替换。选择器只写一处，免得各调用点各写
 * 一份、慢慢走样。配对码入口的 8 个单字符格与提交按钮在这里集中登记。
 */
const PAIRING_SELECTOR = {
  root: '[data-route="pair"]',
  entry: '[data-pairing-entry="true"]',
  phase: "[data-pairing-phase]",
  countdown: '[data-pairing-countdown="true"]',
  host: '[data-pairing-host="true"]',
  hostAddress: "[data-host-address]",
  deviceId: "[data-pairing-device-id]",
  codeEntry: '[data-pairing-code-entry="true"]',
  codeCell: "[data-pairing-code-cell]",
  codeSubmit: '[data-action="submit-pairing-code"]',
  codeOrigin: "[data-pairing-code-origin-value]",
  codeHint: '[data-pairing-code-hint="true"]',
  secure: "[data-pairing-secure]",
} as const;

/** 配对码入口的固定静态提示（lookup 之前不显示数字倒计时）。 */
const CODE_HINT = "配对码 5 分钟内有效。";

/** 一次配对码入口的固定码（服务端把它当作一次性凭据）。 */
const PAIRING_CODE = "ABCD-EFGH";

/** 一次配对会话的固定标识（固定值才能逐字断言它进了请求正文）。 */
const PAIRING_ID = "2bc8b944-2a4f-46a7-8c31-b2c40923f60a";

/** 认领响应给出的配对请求标识（`poll` 的请求正文必须原样带上它，否则状态查询无法索引）。 */
const PAIRING_REQUEST_ID = "9f5b0d51-7c3a-4a2e-8b1d-5e6f708192a3";

/** 一次性配对秘密（32 字节 base64url，与 `src/composition.test.ts` 同口径）。 */
const PAIRING_SECRET = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

/** 认领响应里的服务端随机数（32 字节 base64url，§7.2）：被签进主机证明。 */
const CLAIM_SERVER_NONCE = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

/** 已配对主机记录在本地缓存里的键前缀（`src/composition.ts` 的 `PAIRED_HOST_KEY_PREFIX`）。 */
const PAIRED_HOST_KEY_PREFIX = "paired-host/";

/** 等待状态轮询与握手完成的余量：配对节奏是 2 秒一档，两次查询之后再等重连与握手。 */
const PAIRING_WAIT_MS = 20_000;

/** `m:ss`（分钟没有上限；页面每秒按绝对截止时间重算）。 */
const COUNTDOWN_PATTERN = /^\d+:\d{2}$/;

/** 当前配对阶段（`PairingPanel` 的 `data-pairing-phase`）；没渲染出来时返回 `null`。 */
function phaseOf(container: HTMLElement): string | null {
  return query(container, PAIRING_SELECTOR.phase)?.getAttribute("data-pairing-phase") ?? null;
}

/** 本地缓存里那条已配对主机记录的摘要；没落盘（或不是本地记录）返回 `null`。 */
async function storedPairedHost(): Promise<string | null> {
  const hit = await openPlatform().localCache.get(`${PAIRED_HOST_KEY_PREFIX}${CANONICAL_ORIGIN}`, Date.now());
  return hit !== null && hit.entry.kind === "local" ? hit.entry.summary : null;
}

/**
 * 浏览器里所有可能意外承载配对码的**公开**持久面：地址、Cookie、localStorage、sessionStorage。
 * 配对码只允许活在组件内存与一次 lookup 请求体里。
 */
function browserPersistedText(): string {
  const entries = (storage: Storage): string[] => {
    const pairs: string[] = [];
    for (let index = 0; index < storage.length; index += 1) {
      const key = storage.key(index);
      if (key !== null) pairs.push(`${key}=${storage.getItem(key) ?? ""}`);
    }
    return pairs;
  };
  return [
    globalThis.location.href,
    globalThis.location.hash,
    document.cookie,
    ...entries(globalThis.localStorage),
    ...entries(globalThis.sessionStorage),
  ].join("|");
}

/**
 * 在 8 格里的第一格输入/粘贴整段码，并等到 React 把归一化后的字符分配进各格。
 *
 * 只等第一格变成单个字符：完整码与不完整码都能用同一个等待条件，而不是要求 8 格全满。
 */
async function enterPairingCode(container: HTMLElement, text: string): Promise<void> {
  typeIntoInput(container, '[data-pairing-code-cell="0"]', text, "配对码第一格");
  const firstCharacter = text.trim().toUpperCase().charAt(0).replace("I", "1").replace("L", "1").replace("O", "0");
  await waitFor(() => {
    const first = query(container, '[data-pairing-code-cell="0"]') as HTMLInputElement | null;
    return first !== null && first.value === firstCharacter;
  }, `配对码被分配到输入格（输入 ${JSON.stringify(text)}）`);
}

/**
 * 打开配对地址：真实地址栏里秘密只在**片段**中（`SYNC_PROTOCOL.md` §7.1），
 * 因此与扫码打开同一条路径——把二维码地址的 `#…` 部分挂到 `location.hash` 上，
 * 再挂载页面让组合根读取并立即清除它。
 */
function openPairingAddress(text: string): void {
  globalThis.location.hash = text.slice(text.indexOf("#"));
}

/** 场景缺省：与 `composition-root.ts` 的 `baseScenario` 同口径，起点换成配对页。 */
function baseScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES_WITH_PATHS,
    sessions: sessionsForBothDirectories(),
    agents: AGENTS,
    authenticate: true,
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: [],
    readResult: () => ({ sessionId: "", resources: { messages: [] }, hasEarlier: false }),
    route: PAIR_ROUTE,
    // 环境事实（design D2）：e2e 页面由 http 提供、二维码夹具的 `canonicalOrigin` 是 `https://…`，
    // 因此与注入 `fetch` 同一个理由注入「安全上下文 + 来源等于夹具来源」，让入口的
    // 安全上下文与来源一致校验成立，配对流程才走得到认领与轮询。
    secureContext: () => true,
    origin: () => CANONICAL_ORIGIN,
    ...overrides,
  };
}

/** 一次配对里 `claim` / `status` 的替身及其请求留痕。 */
interface PairingHttpFake {
  readonly fetch: FetchLike;
  /** 按次序记录的请求：端点 URL 与解析后的请求正文（断言「载荷取自哪里」）。 */
  readonly requests: { readonly url: string; readonly body: Record<string, unknown> }[];
}

/**
 * 配对 HTTPS 面的替身：认领回 `pairingRequestId` + `serverNonce` + 一支主机证明；状态**第一次**
 * 回「等待本机确认」、**第二次**回「已批准」并带上主机身份材料。
 *
 * 两次状态而不是一次就批准：「请到电脑上确认」是用户真会看到的阶段（也是页面必须能渲染的
 * 一帧），一次就批准会让它在 e2e 里永远观测不到。
 *
 * ## 可**故意**调错的地方（真 Daemon 调不出来）
 *
 * - `claimSigner`：认领响应的 `hostProof` 由它签。缺省与二维码同一把钥匙；换成另一把，签出来的
 *   仍是形状合法的 64 字节 P1363 签名，只有钥匙不对——客户端只可能因为「验签不过」而拒。
 * - `approvedHostPublicKey`：已批准响应回落的主机公钥。缺省与二维码逐字节相同；换一个即
 *   §7.3 的负向分支（状态响应没有主机签名，比对是唯一依据）。
 * - `alwaysPending`：状态永远回「等待本机确认」。PW-5 用它让「过期」只能由页面的绝对截止时间
 *   推进，而不是由某次状态响应宣布。
 *
 * 认领响应的被签 transcript 覆盖**认领请求本身**的字段，因此它必须现编
 * （`signPairingClaimHostProof`）——写死一条签名只会在客户端换了 nonce 时变成噪声。
 */
function pairingHttpFake(input: {
  /** 二维码里的主机公钥：被签进认领证明，也是「已批准」应当逐字回落的那一份。 */
  readonly qrHostPublicKey: string;
  /** 认领响应 `hostProof` 的签名者（缺省调用点直接给二维码那把钥匙）。 */
  readonly claimSigner: FakeHostIdentity;
  /** 「已批准」响应回落的主机公钥；缺省 = `qrHostPublicKey`（逐字节一致）。 */
  readonly approvedHostPublicKey?: string;
  /** 状态永远停在「等待本机确认」（PW-5 的过期用例）。 */
  readonly alwaysPending?: boolean;
  /** lookup 一律失败（PW-7 的固定失败文案用例）。 */
  readonly lookupFails?: boolean;
  /** lookup 返回的到期时间相对现在（缺省 10 分钟）。 */
  readonly lookupExpiresInMs?: number;
}): PairingHttpFake {
  const requests: { url: string; body: Record<string, unknown> }[] = [];
  let statusPolls = 0;
  const json = (value: unknown, status: number): PairingHttpResponse => ({
    ok: true,
    status,
    json: () => Promise.resolve(value),
  });
  const failed = (status: number): PairingHttpResponse => ({
    ok: false,
    status,
    json: () => Promise.resolve({ code: "pairing.code_invalid", message: "服务端不区分原因", retryable: false, details: {} }),
  });
  const fetchImpl: FetchLike = async (url, init) => {
    // 正文由本替身对侧的**生产**客户端发出（`HttpPairingTransport` 的 `JSON.stringify`），
    // 因此这里按记录读它的键值、用不着再断言形状；读到什么由下面的请求留痕与断言把关。
    const body = JSON.parse(init.body) as Record<string, unknown>;
    requests.push({ url, body });
    if (url.endsWith("/pairing/lookup")) {
      if (input.lookupFails === true) return failed(401);
      // §7.5：成功只返回取回绑定所需的材料，之后与二维码入口共用 claim/status。
      return json(
        {
          protocolVersion: 1,
          pairingId: PAIRING_ID,
          hostId: HOST_ID,
          hostPublicKey: input.qrHostPublicKey,
          canonicalOrigin: CANONICAL_ORIGIN,
          pairingSecret: PAIRING_SECRET,
          expiresAt: new Date(Date.now() + (input.lookupExpiresInMs ?? 600_000)).toISOString(),
        },
        200,
      );
    }
    if (url.endsWith("/pairing/claim")) {
      return json(
        {
          pairingRequestId: PAIRING_REQUEST_ID,
          serverNonce: CLAIM_SERVER_NONCE,
          hostProof: await signPairingClaimHostProof({
            signer: input.claimSigner,
            request: {
              hostId: String(body["hostId"]),
              deviceId: String(body["deviceId"]),
              pairingId: String(body["pairingId"]),
              canonicalOrigin: String(body["canonicalOrigin"]),
              devicePublicKey: String(body["devicePublicKey"]),
              clientNonce: String(body["clientNonce"]),
            },
            hostPublicKey: input.qrHostPublicKey,
            serverNonce: CLAIM_SERVER_NONCE,
            pairingRequestId: PAIRING_REQUEST_ID,
          }),
        },
        201,
      );
    }
    statusPolls += 1;
    if (input.alwaysPending === true || statusPolls === 1) {
      return json({ status: "pending_confirmation" }, 200);
    }
    return json(
      {
        status: "approved",
        // 设备与主机标识都取自这次 **claim 请求**（客户端自己报的那两份）：让「已批准」的
        // 标识一定与二维码/本机身份一致，负向用例才只验它要验的那一项（公钥）。
        device: { deviceId: String(body["deviceId"]) },
        host: {
          hostId: String(body["hostId"]),
          hostPublicKey: input.approvedHostPublicKey ?? input.qrHostPublicKey,
        },
      },
      200,
    );
  };
  return { fetch: fetchImpl, requests };
}

/** 只记账、不应答的配对面：用于断言「一个配对请求都不发」。 */
function recordingFetch(): { readonly fetch: FetchLike; readonly urls: string[] } {
  const urls: string[] = [];
  return {
    urls,
    fetch: async (url) => {
      urls.push(url);
      return { ok: false, status: 500, json: () => Promise.resolve({}) };
    },
  };
}

/**
 * 一段 sync 设备配对二维码地址：形式与协议 §7.1 的二维码一字不差
 * （`https://<origin>/pair#data=<base64url-json>`，秘密在**片段**里）。
 *
 * `expiresInMs` 是相对**现在**的截止时间；PW-5 用它把窗口缩到比一个轮询周期多一点。
 */
function pairingQrText(hostPublicKey: string, expiresInMs = 600_000): string {
  const payload = {
    pairingProtocol: "acp-remote-pairing-v1",
    hostId: HOST_ID,
    hostPublicKey,
    canonicalOrigin: CANONICAL_ORIGIN,
    pairingId: PAIRING_ID,
    pairingSecret: PAIRING_SECRET,
    expiresAt: new Date(Date.now() + expiresInMs).toISOString(),
  };
  return `${CANONICAL_ORIGIN}/pair#data=${encodeBase64Url(new TextEncoder().encode(JSON.stringify(payload)))}`;
}

const checks: readonly Check[] = [
  {
    name: "PW-1 打开配对地址即走真实链路：自动认领 → 等待确认（主机地址与倒计时）→ 已批准 → 约 1.6 秒后自动进目录 → 连接在线",
    async run() {
      const device = await seedDeviceIdentity();
      // **一把钥匙贯穿全程**：二维码里那把 = 认领响应 `hostProof` 的签名者 = 已批准响应回落的
      // 公钥 = 假主机握手的钥匙（§7.2 用载荷里的公钥验认领证明；§7.3 已批准响应的公钥与载荷
      // 逐字节相同）。换一把只会让 claim 或握手当场失败，那是 PW-3 / PW-4 的位置。
      const identity: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      const http = pairingHttpFake({
        qrHostPublicKey: identity.publicKeyBase64Url,
        claimSigner: identity,
      });

      // 起点是真实二维码地址：片段先挂到地址栏上，页面按它读到内容（与扫码打开同一条路径）。
      openPairingAddress(pairingQrText(identity.publicKeyBase64Url));

      const mounted = await mountScenario(
        baseScenario({
          seedPaired: false,
          identity,
          fetch: http.fetch,
          // 自动跳转的落点是目录页根路由 `/`；没注册它，替身会静默忽略这次导航。
          extraRoutes: [DIRS_ROUTE],
        }),
      );
      try {
        // 等**真实页锚点**，不能等 `[data-route="pair"]`：store 未装配时 `RuntimeUnavailable`
        // 占位也带这个属性，拿它当「页面已渲染」会先在占位上通过。
        await waitFor(
          () => query(mounted.container, PAIRING_SELECTOR.entry) !== null,
          "配对页渲染出来（data-pairing-entry 锚点）",
        );

        // 起点真的是「未配对」：没有预置记录（否则后面每一步都可能在它上面走捷径）。
        expectEqual(
          mounted.composition?.store.state.connection.state ?? null,
          "unpaired",
          "未配对起点的连接状态",
        );
        expectEqual(mounted.host?.socketUrls.length ?? -1, 0, "未配对时不得建立任何连接");

        // ① 二维码路径的入口是地址片段：页面挂载后自动读取并认领，没有输入/提交动作。
        await waitFor(() => http.requests.length > 0, "打开配对地址后自动发出认领请求");
        const claim = http.requests[0];
        expectEqual(claim?.url ?? null, `${CANONICAL_ORIGIN}/sync/v1/pairing/claim`, "认领请求的端点");
        expectEqual(claim?.body["pairingId"] ?? null, PAIRING_ID, "认领正文的配对标识（取自二维码）");
        expectEqual(claim?.body["hostId"] ?? null, HOST_ID, "认领正文的主机标识（取自二维码）");
        expectEqual(claim?.body["canonicalOrigin"] ?? null, CANONICAL_ORIGIN, "认领正文的规范化来源");
        // 设备材料必须来自平台身份：换成编造的 deviceId / 公钥，主机侧验签不可能通过。
        expectEqual(claim?.body["deviceId"] ?? null, device.deviceId, "认领正文的 deviceId（来自平台身份）");
        expectEqual(
          claim?.body["devicePublicKey"] ?? null,
          device.publicKeyBase64Url,
          "认领正文的设备公钥（来自平台身份）",
        );
        expectThat(
          typeof claim?.body["proof"] === "string" && (claim.body["proof"] as string).length >= 43,
          "认领正文必须带一段真实的 HMAC 证明",
        );
        // §7.1：片段读完即清——秘密不得留在地址历史里。
        expectEqual(globalThis.location.hash, "", "配对地址的片段读取后被清除");
        // 二维码路径不经过 lookup（本次改动只新增入口，不改既有路径）。
        expectEqual(
          http.requests.filter((request) => request.url.endsWith("/pairing/lookup")).length,
          0,
          "二维码入口不得发出 lookup 请求",
        );

        // ② 状态轮询：先「等待本机确认」，再「已批准」——两个阶段都必须由真实轮询推出来。
        await waitFor(
          () => phaseOf(mounted.container) === "pending_confirmation",
          "状态查询进入「等待本机确认」",
          PAIRING_WAIT_MS,
        );
        // 「等待本机确认」这一刻页面上必须有三件事：二维码解析出来的主机地址（卡片）、
        // 由载荷 `expiresAt` 推出的剩余有效期、以及固定的进行中文案。
        expectThat(query(mounted.container, PAIRING_SELECTOR.host) !== null, "等待确认阶段呈现主机卡片");
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.hostAddress)?.getAttribute("data-host-address") ?? null,
          CANONICAL_ORIGIN,
          "等待确认阶段呈现的主机地址（取自二维码）",
        );
        const countdown = query(mounted.container, PAIRING_SELECTOR.countdown);
        expectThat(countdown !== null, "等待确认阶段必须呈现剩余有效期");
        const countdownText = countdown.textContent ?? "";
        expectThat(
          COUNTDOWN_PATTERN.test(countdownText),
          `倒计时的文本必须是 m:ss（实际 ${JSON.stringify(countdownText)}）`,
        );

        await waitFor(
          () => phaseOf(mounted.container) === "approved",
          "状态查询进入「已批准」",
          PAIRING_WAIT_MS,
        );
        // 自动跳转的计时起点：从页面进入 `approved` 那一刻算（下面还要做落盘与请求断言，
        // 不能把那段耗时算进 1.6 秒里）。
        const approvedAt = Date.now();

        const polls = http.requests.filter((request) => request.url.endsWith("/pairing/status"));
        expectEqual(polls.length, 2, "状态查询次数（一次待确认、一次已批准）");
        expectEqual(polls[0]?.url ?? null, `${CANONICAL_ORIGIN}/sync/v1/pairing/status`, "状态查询的端点");
        // 配对请求标识只在认领响应里出现过：正文里没有它，状态查询无从索引这次配对。
        expectEqual(
          polls[0]?.body["pairingRequestId"] ?? null,
          PAIRING_REQUEST_ID,
          "状态查询正文的配对请求标识（取自认领响应）",
        );
        // §7.3：每次轮询换新 nonce。写死或复用同一个 → 红。
        expectThat(
          typeof polls[0]?.body["requestNonce"] === "string" &&
            typeof polls[1]?.body["requestNonce"] === "string" &&
            polls[0].body["requestNonce"] !== polls[1].body["requestNonce"],
          "每次状态查询必须带一个新的 requestNonce",
        );
        // 已批准阶段：页面显示的就是状态响应给出的设备标识（不是本地编的）。
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.deviceId)?.getAttribute("data-pairing-device-id") ?? null,
          device.deviceId,
          "已批准阶段呈现的设备标识",
        );

        // ③ 落盘的主机材料：键、主机标识与公钥都逐字来自「已批准」响应 + 规范化来源。
        const summary = await storedPairedHost();
        expectThat(summary !== null, "已批准后必须写入 paired-host/<规范化来源> 记录");
        const stored = JSON.parse(summary) as Record<string, unknown>;
        expectEqual(stored["hostId"] ?? null, HOST_ID, "落盘记录的主机标识");
        expectEqual(
          stored["publicKeyBase64Url"] ?? null,
          identity.publicKeyBase64Url,
          "落盘记录的主机公钥（与二维码里那把逐字节相同）",
        );

        // ④ 已批准后自动进目录（design D4）：不留用户点击，也不等连接先 online。
        // 落点是目录页根路由 `/`，不是 `/dirs`。给 1.6 秒留出真实时钟的余量：跳转必须**不是**
        // 立即发生的（那会让用户看不到过渡），也不能拖到数秒之后。
        await waitFor(
          () => currentPath() === "/",
          "已批准约 1.6 秒后自动进入目录页（根路由）",
          PAIRING_WAIT_MS,
        );
        const redirectElapsed = Date.now() - approvedAt;
        expectThat(
          redirectElapsed >= 1_200 && redirectElapsed <= 4_000,
          `自动跳转发生在约 1.6 秒之后（实测 ${redirectElapsed}ms）`,
        );

        // ⑤ 配对完成即触发连接：在线意味着握手用**落盘的那把公钥**验签通过。
        await waitFor(
          () => (mounted.host?.socketUrls.length ?? 0) >= 1,
          "配对完成后建立连接",
          PAIRING_WAIT_MS,
        );
        await waitFor(
          () => mounted.composition?.store.state.connection.state === "online",
          "配对后进入在线（握手用落盘的公钥验签通过）",
          PAIRING_WAIT_MS,
        );
        const socketUrls = mounted.host?.socketUrls ?? [];
        expectEqual(socketUrls.length, 1, "配对完成后恰好一条连接（无并行装配、无反复重连）");
        // 出站 URL 的路径：权威端点是**字面量** `/sync/v1`（不引用被测方的常量，
        // 否则常量被改回 `/sync` 时两边一起变、断言不会红）。
        const first = socketUrls[0];
        expectEqual(first === undefined ? null : new URL(first).pathname, "/sync/v1", "出站连接 URL 的路径");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-2 /pair 无片段显示配对码输入（S0）：8 格、origin、HTTPS、静态提示、无数字倒计时、不发请求",
    async run() {
      const http = recordingFetch();
      // 直接访问 `/pair`（或刷新后片段已清）：地址里没有任何配对内容。
      globalThis.location.hash = "";
      const mounted = await mountScenario(baseScenario({ seedPaired: false, fetch: http.fetch }));
      try {
        await waitFor(
          () => query(mounted.container, PAIRING_SELECTOR.entry) !== null,
          "配对页渲染出来（data-pairing-entry 锚点）",
        );
        await waitFor(
          () => phaseOf(mounted.container) === "awaiting_code",
          "没有片段时进入配对码输入（S0）",
        );

        // 8 个单字符格 + 固定的公开事实（canonical origin / HTTPS）。
        expectEqual(
          queryAll(mounted.container, PAIRING_SELECTOR.codeCell).length,
          8,
          "配对码输入必须有 8 个单字符格",
        );
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.codeOrigin)?.getAttribute(
            "data-pairing-code-origin-value",
          ) ?? null,
          CANONICAL_ORIGIN,
          "配对码输入显示的 canonical origin",
        );
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.secure)?.getAttribute("data-pairing-secure") ?? null,
          "true",
          "安全上下文必须显示 HTTPS 标识",
        );
        expectThat(
          domText(query(mounted.container, PAIRING_SELECTOR.secure) ?? mounted.container).includes(
            "HTTPS",
          ),
          "配对码输入必须显示 HTTPS 标识文本",
        );
        expectThat(
          query(mounted.container, PAIRING_SELECTOR.codeSubmit) !== null,
          "配对码输入必须提供提交按钮",
        );

        // S0 只有静态提示，没有数字倒计时（lookup 之前没有服务端 expiresAt）。
        expectEqual(
          domText(query(mounted.container, PAIRING_SELECTOR.codeHint) ?? mounted.container),
          CODE_HINT,
          "配对码输入必须显示静态提示「配对码 5 分钟内有效。」",
        );
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.countdown),
          null,
          "配对码输入态不得显示数字倒计时",
        );

        // 没有提交就不发任何请求，也没有已配对记录/连接。
        expectEqual(http.urls.length, 0, "配对码输入态一个配对请求都不发");
        expectEqual(await storedPairedHost(), null, "未提交时不得写入已配对主机记录");
        expectEqual(mounted.host?.socketUrls.length ?? -1, 0, "未提交时不得建立连接");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-11 配对页明暗切换：按钮存在、点击翻转标签与共享偏好、不发请求",
    async run() {
      const http = recordingFetch();
      globalThis.location.hash = "";
      themePreferenceStore.set("light");
      const mounted = await mountScenario(baseScenario({ seedPaired: false, fetch: http.fetch }));
      try {
        await waitFor(
          () => query(mounted.container, '[data-pairing-theme-toggle="true"]') !== null,
          "配对页渲染主题按钮",
        );
        const button = query(mounted.container, '[data-pairing-theme-toggle="true"]');
        expectEqual(
          button?.getAttribute("aria-label") ?? null,
          "当前：白天，点击切换",
          "浅色下按钮的动作标签",
        );
        click(button, "配对页主题按钮");
        await waitFor(
          () =>
            query(mounted.container, '[data-pairing-theme-toggle="true"]')?.getAttribute(
              "aria-label",
            ) === "当前：黑夜，点击切换",
          "点击后主题标签翻转为黑夜",
        );
        expectEqual(
          globalThis.localStorage.getItem(THEME_PREFERENCE_STORAGE_KEY),
          "dark",
          "主题切换写入共享偏好",
        );
        expectEqual(http.urls.length, 0, "主题切换不发任何配对请求");
      } finally {
        await mounted.unmount();
        themePreferenceStore.set("system");
      }
    },
  },
  {
    name: "PW-2b /node-link/pair 无片段：仍是无载荷页、不提供配对码输入、不发请求",
    async run() {
      const http = recordingFetch();
      globalThis.location.hash = "";
      const mounted = await mountScenario(
        baseScenario({ seedPaired: false, fetch: http.fetch, route: NODE_LINK_PAIR_ROUTE }),
      );
      try {
        await waitFor(
          () => phaseOf(mounted.container) === "idle",
          "node-link 入口没有片段时进入 idle",
        );
        expectThat(
          query(mounted.container, '[data-pairing-failure="no_payload"]') !== null,
          "node-link 无载荷必须呈现 no_payload",
        );
        expectThat(
          domText(query(mounted.container, PAIRING_SELECTOR.root) ?? mounted.container).includes(
            "这个地址里没有节点配对信息",
          ),
          "node-link 无载荷页必须给出节点专用文案",
        );
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.codeEntry),
          null,
          "node-link 入口不得显示配对码输入",
        );
        expectEqual(http.urls.length, 0, "node-link 无载荷时一个请求都不发");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-3 认领响应由别的钥匙签：明确失败（auth.proof_invalid / 固定标题）、停止轮询、不落盘、不建连接",
    async run() {
      // 二维码里那把（也是握手要用的那把）与**签认领证明的那把**是两把钥匙：证明的长度、字母表
      // 都合法，只有钥匙不对。客户端只可能因为「验签不过」而拒——这正是本用例要抓的那一步。
      const identity: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      const rogue: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      expectThat(
        rogue.publicKeyBase64Url !== identity.publicKeyBase64Url,
        "两把测试钥匙的公钥必须不同，否则「签名不匹配」不成立",
      );
      // 「已批准」这一侧故意配成**会成功**的样子（公钥与二维码一致）：认领一旦失败，
      // 实现若还去轮询，就会走到已批准、落盘、连上去——于是下面的断言全红。
      const http = pairingHttpFake({
        qrHostPublicKey: identity.publicKeyBase64Url,
        claimSigner: rogue,
      });
      openPairingAddress(pairingQrText(identity.publicKeyBase64Url));
      const mounted = await mountScenario(
        baseScenario({ seedPaired: false, identity, fetch: http.fetch }),
      );
      try {
        // 二维码路径的入口是片段：不需要任何输入/点击，页面自己发出认领。
        await waitFor(() => http.requests.length > 0, "打开配对地址后自动发出认领请求");

        // ① 明确失败：阶段进 `failed`，且错误码是**验签失败**那一支。形状不符（`protocol.schema_invalid`）
        // 与验签不过（`auth.proof_invalid`）是两个分支，断言具体的码才分得清「拒绝」与「没读懂」。
        await waitFor(
          () => phaseOf(mounted.container) === "failed",
          "验签不过时配对进入明确失败",
          PAIRING_WAIT_MS,
        );
        expectThat(
          query(mounted.container, '[data-pairing-error="auth.proof_invalid"]') !== null,
          "失败态必须带验签失败的错误码（auth.proof_invalid）",
        );
        // 固定文案在标题/说明里：`data-pairing-error` 只是一个空的诊断码节点，不再承载可读文本。
        expectThat(
          query(mounted.container, '[data-pairing-failure="request_failed"]') !== null,
          "验签失败落到「请求未能完成」这一固定呈现",
        );
        const root = query(mounted.container, PAIRING_SELECTOR.root);
        expectThat(root !== null, "失败页的根节点必须存在");
        expectThat(
          domText(root).includes("配对请求未能完成"),
          "失败页必须呈现固定标题「配对请求未能完成」",
        );

        // ② 不落盘：一条已配对主机记录都不许写（写下去就等于信任了一把不是二维码带来的钥匙）。
        expectEqual(await storedPairedHost(), null, "验签失败时不得写入任何已配对主机记录");
        // ③ 不建连接。
        expectEqual(mounted.host?.socketUrls.length ?? -1, 0, "验签失败时不得建立连接");
        // ④ 停止轮询：等过**两个**轮询周期再断言，否则「还没来得及发」与「真的不发」分不开。
        const { promise: pollWindow, resolve: windowElapsed } = Promise.withResolvers<void>();
        setTimeout(windowElapsed, PAIRING_POLL_INTERVAL_MS * 2);
        await pollWindow;
        expectEqual(
          http.requests.filter((request) => request.url.endsWith("/pairing/status")).length,
          0,
          "认领验签失败后不得发出任何状态查询",
        );
        expectEqual(phaseOf(mounted.container), "failed", "等过轮询窗口后阶段仍是明确失败");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-4 已批准响应的主机公钥与二维码差一个字节：明确失败（protocol.schema_invalid / unrecognized）且不落盘、不建连接",
    async run() {
      const identity: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      // 只翻**末字节**：长度、字母表、前 64 字节都不变，因此「形状检查」一定放行——它抓的是
      // 逐字节比对本身（只比主机标识、或做前缀/trim/大小写宽松比较的实现会在这条上放行）。
      const mismatched = tamperPublicKeyLastByte(identity.publicKeyBase64Url);
      expectThat(
        mismatched.length === identity.publicKeyBase64Url.length &&
          mismatched !== identity.publicKeyBase64Url,
        "被篡改的公钥必须与二维码里那把长度相同、内容不同",
      );
      const http = pairingHttpFake({
        qrHostPublicKey: identity.publicKeyBase64Url,
        claimSigner: identity,
        approvedHostPublicKey: mismatched,
      });
      openPairingAddress(pairingQrText(identity.publicKeyBase64Url));
      const mounted = await mountScenario(
        baseScenario({ seedPaired: false, identity, fetch: http.fetch }),
      );
      try {
        // 先走到「等待本机确认」：认领这一跳是**通过**的（同一把钥匙签、验签过），否则这一条验的
        // 就不是「已批准」那一跳了。
        await waitFor(
          () => phaseOf(mounted.container) === "pending_confirmation",
          "状态查询进入「等待本机确认」",
          PAIRING_WAIT_MS,
        );
        // 已批准响应携带的公钥与二维码不一致 → 明确失败，且错误码是「已批准响应缺少可用的主机身份
        // 材料」那一支。
        await waitFor(
          () => phaseOf(mounted.container) === "failed",
          "公钥不一致时配对进入明确失败",
          PAIRING_WAIT_MS,
        );
        expectThat(
          query(mounted.container, '[data-pairing-error="protocol.schema_invalid"]') !== null,
          "失败态必须带「已批准响应材料不可用」的错误码",
        );
        // 结构/材料不符归到「无法识别这个地址」这一固定呈现（design D6）。
        expectThat(
          query(mounted.container, '[data-pairing-failure="unrecognized"]') !== null,
          "公钥不一致落到「无法识别」这一固定呈现",
        );
        const root = query(mounted.container, PAIRING_SELECTOR.root);
        expectThat(root !== null, "失败页的根节点必须存在");
        expectThat(
          domText(root).includes("无法识别这个地址"),
          "失败页必须呈现固定标题「无法识别这个地址」",
        );

        // 不落盘、不建连接。
        expectEqual(await storedPairedHost(), null, "公钥不一致时不得写入任何已配对主机记录");
        expectEqual(mounted.host?.socketUrls.length ?? -1, 0, "公钥不一致时不得建立连接");
        // 失败即停止轮询：两次查询（一次待确认、一次已批准）之后不再查询。与 PW-3 同款，先等过
        // **两个**轮询周期再断言——否则「还没来得及发」与「真的不发」分不开。
        const { promise: pollWindow, resolve: windowElapsed } = Promise.withResolvers<void>();
        setTimeout(windowElapsed, PAIRING_POLL_INTERVAL_MS * 2);
        await pollWindow;
        const polls = http.requests.filter((request) => request.url.endsWith("/pairing/status"));
        expectEqual(polls.length, 2, "公钥不一致失败后不得再发出状态查询");
        expectEqual(phaseOf(mounted.container), "failed", "等过轮询窗口后阶段仍是明确失败");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-5 倒计时归零：先到等待确认，再按绝对截止时间进入「地址已过期」并停止轮询",
    async run() {
      // 截止时间必须**晚于一次状态轮询**（2 秒）才观测得到 `pending_confirmation`，
      // 又要在本用例的窗口内归零；留出挂载与认领的余量，取 3.5 秒。
      const expiryWindowMs = 3_500;
      const identity: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      // 假主机**永远**回「等待本机确认」：过期只能由页面的绝对截止时间推进，
      // 不能靠某次状态响应「宣布」（那正是本用例要排除的实现）。
      const http = pairingHttpFake({
        qrHostPublicKey: identity.publicKeyBase64Url,
        claimSigner: identity,
        alwaysPending: true,
      });
      openPairingAddress(pairingQrText(identity.publicKeyBase64Url, expiryWindowMs));
      const mounted = await mountScenario(
        baseScenario({ seedPaired: false, identity, fetch: http.fetch }),
      );
      try {
        await waitFor(
          () => phaseOf(mounted.container) === "pending_confirmation",
          "第一次状态查询进入等待确认",
          PAIRING_WAIT_MS,
        );

        // 归零 → 终态 `expired`：不是 `failed`，因此没有 `data-pairing-failure` 锚点。
        await waitFor(
          () => phaseOf(mounted.container) === "expired",
          "倒计时归零后进入「地址已过期」",
          10_000,
        );
        expectEqual(
          query(mounted.container, "[data-pairing-failure]"),
          null,
          "过期是终态，不得被呈现成「本侧失败」",
        );
        const root = query(mounted.container, PAIRING_SELECTOR.root);
        expectThat(root !== null, "过期页的根节点必须存在");
        expectThat(
          domText(root).includes("配对地址已过期"),
          "过期页必须呈现固定标题「配对地址已过期」",
        );

        // 此后至少等过一个轮询周期：到期回调取消了轮询，status 请求数不再增加。
        const pollsAtExpiry = http.requests.filter((request) => request.url.endsWith("/pairing/status")).length;
        const { promise: pollWindow, resolve: windowElapsed } = Promise.withResolvers<void>();
        setTimeout(windowElapsed, PAIRING_POLL_INTERVAL_MS + 500);
        await pollWindow;
        expectEqual(
          http.requests.filter((request) => request.url.endsWith("/pairing/status")).length,
          pollsAtExpiry,
          "过期后不得再发出状态查询",
        );
        expectEqual(await storedPairedHost(), null, "过期时不得写入任何已配对主机记录");
        expectEqual(mounted.host?.socketUrls.length ?? -1, 0, "过期时不得建立连接");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-6 配对码入口：输入合法码 → lookup → 同一套 claim/轮询 → 落盘进目录；码不进 URL/存储",
    async run() {
      const device = await seedDeviceIdentity();
      const identity: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      const http = pairingHttpFake({
        qrHostPublicKey: identity.publicKeyBase64Url,
        claimSigner: identity,
        // 90 秒而不是固定的 5:00：倒计时必须来自 lookup 响应的 `expiresAt`。
        lookupExpiresInMs: 90_000,
      });
      globalThis.location.hash = "";
      const mounted = await mountScenario(
        baseScenario({
          seedPaired: false,
          identity,
          fetch: http.fetch,
          extraRoutes: [DIRS_ROUTE],
        }),
      );
      try {
        await waitFor(
          () => phaseOf(mounted.container) === "awaiting_code",
          "配对码输入态渲染出来",
        );
        // S0：没有数字倒计时，只有静态提示。
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.countdown),
          null,
          "配对码输入态不得显示数字倒计时",
        );

        await enterPairingCode(mounted.container, "abcd-efgh");
        click(query(mounted.container, PAIRING_SELECTOR.codeSubmit), "配对码提交按钮");

        // ① lookup：码只走 body，且大小写/分隔符已归一化。
        await waitFor(() => http.requests.length > 0, "提交后发出 lookup 请求");
        const lookup = http.requests[0];
        expectEqual(lookup?.url ?? null, `${CANONICAL_ORIGIN}/sync/v1/pairing/lookup`, "lookup 的端点");
        expectEqual(lookup?.body["pairingCode"] ?? null, PAIRING_CODE, "lookup 正文里的归一化配对码");
        expectEqual(lookup?.body["protocolVersion"] ?? null, 1, "lookup 正文的协议版本");
        expectThat(
          (lookup?.url ?? "").includes(PAIRING_CODE) === false,
          "配对码不得进入 URL",
        );

        // ② 复用既有 claim 链路：第二条请求是 claim，而不是第二套流程。
        await waitFor(
          () => http.requests.some((request) => request.url.endsWith("/pairing/claim")),
          "lookup 成功后进入既有 claim",
          PAIRING_WAIT_MS,
        );
        const claim = http.requests.find((request) => request.url.endsWith("/pairing/claim"));
        expectEqual(claim?.body["pairingId"] ?? null, PAIRING_ID, "claim 正文的配对标识（来自 lookup）");
        expectEqual(claim?.body["hostId"] ?? null, HOST_ID, "claim 正文的主机标识（来自 lookup）");

        // ③ 等待确认：倒计时从 lookup 返回的 `expiresAt` 启动（这是第一次出现数字倒计时）。
        await waitFor(
          () => phaseOf(mounted.container) === "pending_confirmation",
          "状态查询进入等待确认",
          PAIRING_WAIT_MS,
        );
        const countdown = query(mounted.container, PAIRING_SELECTOR.countdown);
        expectThat(countdown !== null, "lookup 之后必须按服务端 expiresAt 显示倒计时");
        const countdownText = countdown.textContent ?? "";
        expectThat(
          COUNTDOWN_PATTERN.test(countdownText),
          `倒计时文本必须是 m:ss（实际 ${JSON.stringify(countdownText)}）`,
        );
        // 反例：按「页面打开起 5:00」计时 → 这里会是 5:00。
        expectThat(
          countdownText !== "5:00" && (countdownText.startsWith("1:") || countdownText === "2:00"),
          `倒计时必须来自 lookup 响应的 expiresAt（90 秒），实际 ${JSON.stringify(countdownText)}`,
        );

        // ④ 码不进入任何公开持久面。
        expectAbsent(browserPersistedText(), PAIRING_CODE, "配对码不得进入 URL/Cookie/localStorage/sessionStorage");

        // ⑤ 已批准 → 落盘 → 自动进目录 → 连接在线。
        await waitFor(
          () => phaseOf(mounted.container) === "approved",
          "状态查询进入已批准",
          PAIRING_WAIT_MS,
        );
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.deviceId)?.getAttribute("data-pairing-device-id") ?? null,
          device.deviceId,
          "已批准阶段呈现的设备标识",
        );
        const summary = await storedPairedHost();
        expectThat(summary !== null, "已批准后必须写入 paired-host 记录");
        expectEqual(
          (JSON.parse(summary) as Record<string, unknown>)["publicKeyBase64Url"] ?? null,
          identity.publicKeyBase64Url,
          "落盘记录的主机公钥（来自 lookup 与已批准响应）",
        );
        expectAbsent(summary, PAIRING_CODE, "配对码不得写入已配对记录");
        await waitFor(
          () => currentPath() === "/",
          "已批准后自动进入目录页",
          PAIRING_WAIT_MS,
        );
        await waitFor(
          () => mounted.composition?.store.state.connection.state === "online",
          "配对后进入在线",
          PAIRING_WAIT_MS,
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-7 配对码格式不符：在发请求前明确失败，固定文案、无页内重试、不落盘",
    async run() {
      const http = recordingFetch();
      globalThis.location.hash = "";
      const mounted = await mountScenario(baseScenario({ seedPaired: false, fetch: http.fetch }));
      try {
        await waitFor(
          () => phaseOf(mounted.container) === "awaiting_code",
          "配对码输入态渲染出来",
        );
        // 7 位连续码：格式不合法，但能被输入框接住（带 `-` 的不完整码不是合法粘贴形态）。
        await enterPairingCode(mounted.container, "ABCDEFG");
        click(query(mounted.container, PAIRING_SELECTOR.codeSubmit), "配对码提交按钮");

        await waitFor(
          () => phaseOf(mounted.container) === "failed",
          "格式不符时明确失败",
        );
        expectEqual(
          query(mounted.container, '[data-pairing-failure="code_invalid"]') !== null,
          true,
          "格式不符必须落在 code_invalid 固定呈现",
        );
        const root = query(mounted.container, PAIRING_SELECTOR.root);
        expectThat(root !== null, "失败页根节点必须存在");
        expectThat(
          domText(root).includes("配对码不对或已过期，请重新生成配对码"),
          "必须呈现固定文案「配对码不对或已过期，请重新生成配对码」",
        );
        // 格式不符在**发请求前**失败；失败页没有页内重试，也不保留输入态。
        expectEqual(http.urls.length, 0, "格式不符不得发出任何请求");
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.codeSubmit),
          null,
          "失败页不得提供页内重试按钮",
        );
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.codeEntry),
          null,
          "失败页不得保留配对码输入",
        );
        expectEqual(await storedPairedHost(), null, "失败时不得写入已配对记录");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-8 配对码 lookup 失败：固定文案「配对码不对或已过期，请重新生成配对码」，不推断、不重试、不落盘",
    async run() {
      const identity: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      const http = pairingHttpFake({
        qrHostPublicKey: identity.publicKeyBase64Url,
        claimSigner: identity,
        lookupFails: true,
      });
      globalThis.location.hash = "";
      const mounted = await mountScenario(
        baseScenario({ seedPaired: false, identity, fetch: http.fetch }),
      );
      try {
        await waitFor(
          () => phaseOf(mounted.container) === "awaiting_code",
          "配对码输入态渲染出来",
        );
        await enterPairingCode(mounted.container, PAIRING_CODE);
        click(query(mounted.container, PAIRING_SELECTOR.codeSubmit), "配对码提交按钮");

        await waitFor(
          () => phaseOf(mounted.container) === "failed",
          "lookup 失败后进入明确失败",
          PAIRING_WAIT_MS,
        );
        expectThat(
          query(mounted.container, '[data-pairing-failure="code_invalid"]') !== null,
          "lookup 失败必须落在 code_invalid 固定呈现",
        );
        expectThat(
          query(mounted.container, '[data-pairing-error="pairing.code_invalid"]') !== null,
          "失败态必须带 pairing.code_invalid",
        );
        const root = query(mounted.container, PAIRING_SELECTOR.root);
        expectThat(root !== null, "失败页根节点必须存在");
        const panelText = domText(root);
        expectThat(
          panelText.includes("配对码不对或已过期，请重新生成配对码"),
          "必须呈现固定文案「配对码不对或已过期，请重新生成配对码」",
        );
        // 不回显后端 message、不显示配对是否存在，也不回显用户输入的码。
        expectAbsent(panelText, "服务端不区分原因", "失败页不得回显后端 message");
        expectAbsent(panelText, PAIRING_CODE, "失败页不得回显配对码");
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.codeSubmit),
          null,
          "失败页不得提供页内重试按钮",
        );
        // 只有 lookup 一条请求：不 claim、不轮询。
        expectEqual(http.requests.length, 1, "lookup 失败后不得再发其它配对请求");
        expectEqual(
          http.requests[0]?.url ?? null,
          `${CANONICAL_ORIGIN}/sync/v1/pairing/lookup`,
          "唯一请求是 lookup",
        );
        expectEqual(await storedPairedHost(), null, "lookup 失败不得写入已配对记录");
        expectEqual(mounted.host?.socketUrls.length ?? -1, 0, "lookup 失败不得建立连接");
        expectAbsent(browserPersistedText(), PAIRING_CODE, "失败后配对码仍不得进入公开持久面");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-8b lookup 响应已过期（服务端 expiresAt 优先）：不 claim、不显示本地估算、固定失败呈现",
    async run() {
      const identity: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      const http = pairingHttpFake({
        qrHostPublicKey: identity.publicKeyBase64Url,
        claimSigner: identity,
        // lookup 成功但 `expiresAt` 已经过去：页面不得按「现在 + 5 分钟」继续 claim。
        lookupExpiresInMs: -1_000,
      });
      globalThis.location.hash = "";
      const mounted = await mountScenario(
        baseScenario({ seedPaired: false, identity, fetch: http.fetch }),
      );
      try {
        await waitFor(
          () => phaseOf(mounted.container) === "awaiting_code",
          "配对码输入态渲染出来",
        );
        await enterPairingCode(mounted.container, PAIRING_CODE);
        click(query(mounted.container, PAIRING_SELECTOR.codeSubmit), "配对码提交按钮");

        await waitFor(
          () => phaseOf(mounted.container) === "failed",
          "lookup 返回已过期后进入固定失败呈现",
          PAIRING_WAIT_MS,
        );
        expectThat(
          query(mounted.container, '[data-pairing-failure="code_invalid"]') !== null,
          "已过期必须落在 code_invalid 固定呈现",
        );
        expectThat(
          domText(query(mounted.container, PAIRING_SELECTOR.root) ?? mounted.container).includes(
            "配对码不对或已过期，请重新生成配对码",
          ),
          "必须呈现固定文案「配对码不对或已过期，请重新生成配对码」",
        );
        // 服务端 expiresAt 优先：不显示倒计时、不发 claim/status。
        expectEqual(
          query(mounted.container, PAIRING_SELECTOR.countdown),
          null,
          "已过期时不得显示数字倒计时",
        );
        expectEqual(http.requests.length, 1, "lookup 已过期后不得再发其它配对请求");
        expectEqual(await storedPairedHost(), null, "lookup 已过期不得写入已配对记录");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-9 配对码入口倒计时归零：呈现 S6「配对码不对或已过期」，不是 S7「配对地址已过期」",
    async run() {
      // 与 PW-5 同一条绝对截止时间路径，但入口是配对码：lookup 返回的 `expiresAt` 只比一个
      // 轮询周期多一点，假主机永远回「等待确认」，因此归零只能由页面的到期定时器推进。
      const expiryWindowMs = 3_500;
      const identity: FakeHostIdentity = await createFakeHostIdentity(HOST_ID);
      const http = pairingHttpFake({
        qrHostPublicKey: identity.publicKeyBase64Url,
        claimSigner: identity,
        alwaysPending: true,
        lookupExpiresInMs: expiryWindowMs,
      });
      globalThis.location.hash = "";
      const mounted = await mountScenario(
        baseScenario({ seedPaired: false, identity, fetch: http.fetch }),
      );
      try {
        await waitFor(
          () => phaseOf(mounted.container) === "awaiting_code",
          "配对码输入态渲染出来",
        );
        await enterPairingCode(mounted.container, PAIRING_CODE);
        click(query(mounted.container, PAIRING_SELECTOR.codeSubmit), "配对码提交按钮");

        // 先到等待确认：倒计时从 lookup 响应的 `expiresAt` 启动（不是页面打开时刻）。
        await waitFor(
          () => phaseOf(mounted.container) === "pending_confirmation",
          "lookup 之后进入等待确认",
          PAIRING_WAIT_MS,
        );

        // 归零 → 配对码入口的固定失败呈现（S6）。反例：仍按二维码口径收口 → 阶段是 `expired`，
        // 标题是「配对地址已过期」，下面三条断言都会红。
        await waitFor(
          () => phaseOf(mounted.container) === "failed",
          "倒计时归零后进入配对码固定失败呈现",
          10_000,
        );
        expectThat(
          query(mounted.container, '[data-pairing-failure="code_invalid"]') !== null,
          "归零必须落在 code_invalid 固定呈现",
        );
        const root = query(mounted.container, PAIRING_SELECTOR.root);
        expectThat(root !== null, "失败页根节点必须存在");
        const text = domText(root);
        expectThat(
          text.includes("配对码不对或已过期，请重新生成配对码"),
          "必须呈现 S6 固定文案「配对码不对或已过期，请重新生成配对码」",
        );
        expectThat(
          text.includes("配对地址已过期") === false,
          "配对码入口不得呈现二维码入口的「配对地址已过期」",
        );
        // 归零即停止轮询：等过一个轮询周期，status 请求数不再增加。
        const pollsAtExpiry = http.requests.filter((request) =>
          request.url.endsWith("/pairing/status"),
        ).length;
        const { promise: pollWindow, resolve: windowElapsed } = Promise.withResolvers<void>();
        setTimeout(windowElapsed, PAIRING_POLL_INTERVAL_MS + 500);
        await pollWindow;
        expectEqual(
          http.requests.filter((request) => request.url.endsWith("/pairing/status")).length,
          pollsAtExpiry,
          "归零后不得再发出状态查询",
        );
        expectEqual(await storedPairedHost(), null, "归零时不得写入任何已配对主机记录");
        expectEqual(mounted.host?.socketUrls.length ?? -1, 0, "归零时不得建立连接");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "PW-10 编辑已填的格：多字符变更从被编辑的格开始覆盖，前面已输入的位保留",
    async run() {
      // 不需要任何配对请求：这里只验输入格的变更语义，进入 S0 即可。
      const http = recordingFetch();
      globalThis.location.hash = "";
      const mounted = await mountScenario(baseScenario({ seedPaired: false, fetch: http.fetch }));
      try {
        await waitFor(
          () => phaseOf(mounted.container) === "awaiting_code",
          "配对码输入态渲染出来",
        );
        // 先填满 8 格：A B C D E F G H。
        await enterPairingCode(mounted.container, PAIRING_CODE);

        // 模拟「没有选中就直接输入第 4 格」时 RN-web 交给 `onChangeText` 的值：
        // 原字符 `D` + 新字符 `X`。正确的实现从第 4 格开始写成 `D`、`X`，前面三位不动；
        // 从格 0 重建的实现会把整码变成 `DX` 并清空其余格。
        typeIntoInput(
          mounted.container,
          '[data-pairing-code-cell="3"]',
          "DX",
          "配对码第 4 格（多字符变更）",
        );
        await waitFor(
          () =>
            queryAll(mounted.container, PAIRING_SELECTOR.codeCell)
              .map((node) => (node as HTMLInputElement).value)
              .join("") === "ABCDXFGH",
          "多字符变更只覆盖被编辑的格及其后的一格，其余位保留",
        );
        // 一个请求都不发：这一步只验输入格本身。
        expectEqual(http.urls.length, 0, "编辑输入格不得发出配对请求");
      } finally {
        await mounted.unmount();
      }
    },
  },
];

export const PAIRING_FIRST_RUN_CHECKS = checks;
