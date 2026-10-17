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
 * 页面**没有**输入框、粘贴框、提交按钮：地址片段是唯一入口（design D1），因此起点一律是
 * 「先把片段挂到 `location.hash`，再挂载页面」。被替身顶掉的只有**传输**两头：配对 HTTPS 面
 * （`fetch`，e2e 环境里 `https://` 来源不可达）与 socket（假主机）。配对状态机、结构校验、
 * 落盘、连接装配、握手验签、快照管线全部是生产代码。
 *
 * ## 判别力：每条断言在实现被回退时都会红
 *
 * - **PW-1** 正向链路的每一跳都有独立锚点：端点路径、认领正文里的设备材料、片段清除、状态查询
 *   正文里的 `pairingRequestId` 与每次都换的 `requestNonce`、pending 阶段的主机地址与 `m:ss`
 *   倒计时、已批准阶段的设备标识、落盘摘要、自动落点 `/`、出站 URL 路径。认领响应由
 *   **二维码里那把**钥匙签（`signPairingClaimHostProof`），因此「验签用错了钥匙/没验」这条
 *   在此**不**会红——那是 PW-3 的位置。
 * - **PW-2** 没有载荷时**不发请求**：把「读不到片段」实现成继续认领 → `recordingFetch` 出现请求；
 *   把无载荷页实现成输入框 → `[data-pairing-input]` 存在；把无载荷当成载荷错误 → 锚点不是
 *   `no_payload`。
 * - **PW-3** / **PW-4** 用假主机**故意**产出被篡改的响应（真 Daemon 无法诚实地产出这种响应，
 *   这正是它们存在的理由）：认领响应换一把钥匙签、已批准响应换一个公钥。两条都断言
 *   **明确失败**（阶段 + 错误码 + 固定标题）、不落盘、不建连接，并断言**停止轮询**——
 *   「没有成功」不构成证据：把验签去掉、把逐字节比对改成宽松比较，两条都会在这些断言上红。
 * - **PW-5** 倒计时归零：`expiresAt` 只比一个轮询周期多一点，假主机永远回「等待确认」。页面必须
 *   先呈现 pending、再由**绝对截止时间**推进到 `expired`（不是固定 300 秒），并且此后不再发出
 *   状态查询。把到期定时器删掉、或以「页面打开时刻 + 5 分钟」计时，这条都会红。
 */

import {
  AGENTS,
  DIRS_ROUTE,
  PAIR_ROUTE,
  WORKSPACES_WITH_PATHS,
  sessionsForBothDirectories,
} from "../fixtures";
import { createFakeHostIdentity, signPairingClaimHostProof, tamperPublicKeyLastByte } from "../fake-host";
import type { FakeHostIdentity } from "../fake-host";
import { encodeBase64Url } from "../../../src/sync-client";
import { PAIRING_POLL_INTERVAL_MS } from "../../../src/features/pairing-model";
import { openPlatform } from "../../../src/platform";
import { currentPath } from "../router-stub";
import {
  CANONICAL_ORIGIN,
  HOST_ID,
  domText,
  expectEqual,
  expectThat,
  mountScenario,
  query,
  seedDeviceIdentity,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";
import type { FetchLike, PairingHttpResponse } from "../../../src/composition";

/**
 * 配对页的锚点选择器。
 *
 * 每次现查而不是缓存元素：容器里的节点会随阶段整棵替换。选择器只写一处，免得各调用点各写
 * 一份、慢慢走样。输入框时代的 `[data-pairing-input]` / 提交按钮已删除，这里也不再引用。
 */
const PAIRING_SELECTOR = {
  root: '[data-route="pair"]',
  entry: '[data-pairing-entry="true"]',
  phase: "[data-pairing-phase]",
  countdown: '[data-pairing-countdown="true"]',
  host: '[data-pairing-host="true"]',
  hostAddress: "[data-host-address]",
  deviceId: "[data-pairing-device-id]",
} as const;

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
}): PairingHttpFake {
  const requests: { url: string; body: Record<string, unknown> }[] = [];
  let statusPolls = 0;
  const json = (value: unknown, status: number): PairingHttpResponse => ({
    ok: true,
    status,
    json: () => Promise.resolve(value),
  });
  const fetchImpl: FetchLike = async (url, init) => {
    // 正文由本替身对侧的**生产**客户端发出（`HttpPairingTransport` 的 `JSON.stringify`），
    // 因此这里按记录读它的键值、用不着再断言形状；读到什么由下面的请求留痕与断言把关。
    const body = JSON.parse(init.body) as Record<string, unknown>;
    requests.push({ url, body });
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

        // ① 地址片段是唯一入口：页面挂载后自动读取并认领，没有任何输入/提交动作。
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
    name: "PW-2 没有配对内容时不发请求：idle、固定无载荷页、没有输入框、没有已配对记录",
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
        await waitFor(() => phaseOf(mounted.container) === "idle", "没有片段时进入 idle");
        expectThat(
          query(mounted.container, '[data-pairing-failure="no_payload"]') !== null,
          "无载荷必须呈现专门的失败类别（no_payload）",
        );
        const root = query(mounted.container, PAIRING_SELECTOR.root);
        expectThat(root !== null, "配对页根节点必须存在");
        expectThat(
          domText(root).includes("这个地址里没有配对信息"),
          "无载荷页必须给出固定标题「这个地址里没有配对信息」",
        );

        // 页面不再是入口：输入框与提交按钮都已删除（有它们就说明第二套入口回来了）。
        expectEqual(
          query(mounted.container, "[data-pairing-input]"),
          null,
          "配对页不得再提供输入框",
        );
        expectEqual(
          query(mounted.container, '[data-action="submit-pairing-qr"]'),
          null,
          "配对页不得再提供提交按钮",
        );

        // 没有可读内容时不发起任何配对请求，也不写入任何已配对记录。
        expectEqual(http.urls.length, 0, "没有载荷时一个配对请求都不发");
        expectEqual(await storedPairedHost(), null, "没有载荷时不得写入已配对主机记录");
        expectEqual(mounted.host?.socketUrls.length ?? -1, 0, "没有载荷时不得建立连接");
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
        // 片段是唯一入口：不需要任何输入/点击，页面自己发出认领。
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
];

export const PAIRING_FIRST_RUN_CHECKS = checks;
