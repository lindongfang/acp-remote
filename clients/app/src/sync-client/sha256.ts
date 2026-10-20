/**
 * 流式 SHA-256（无依赖、纯 TypeScript）。
 *
 * ## 为什么需要它
 *
 * 附件封存要求「整段原始字节的流式 SHA-256」（`docs/SYNC_PROTOCOL.md` §3.4）：正文可能远大于
 * 单块 32 KiB，客户端 MUST NOT 为了算摘要把它整份读进内存。`DigestPort.sha256` 是**一次性**
 * 接口（`crypto.subtle.digest` 就只接受完整缓冲），因此这里提供增量实现，供附件上传按块喂入。
 *
 * 实现采用 FIPS 180-4 的标准 64 轮压缩：状态 8×u32、512-bit 分块、长度按**字节数**记（不是
 * 位数）最后折成 64-bit 大端长度。JS 的位运算天然是 32-bit，`>>> 0` 保持无符号。
 *
 * 浏览器与 Node 都可以直接运行（只用 `Uint8Array`），单测用 NIST 向量对拍。
 */

const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

const INITIAL = new Uint32Array([
  0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
]);

function rotr(value: number, bits: number): number {
  return ((value >>> bits) | (value << (32 - bits))) >>> 0;
}

/** 512-bit 压缩：`block` 必须是 64 字节。 */
function compress(state: Uint32Array, block: Uint8Array, blockOffset: number): void {
  const schedule = new Uint32Array(64);
  for (let index = 0; index < 16; index += 1) {
    const offset = blockOffset + index * 4;
    schedule[index] =
      (((block[offset] ?? 0) << 24) |
        ((block[offset + 1] ?? 0) << 16) |
        ((block[offset + 2] ?? 0) << 8) |
        (block[offset + 3] ?? 0)) >>>
      0;
  }
  for (let index = 16; index < 64; index += 1) {
    const s0 = (rotr(schedule[index - 15]!, 7) ^ rotr(schedule[index - 15]!, 18) ^ (schedule[index - 15]! >>> 3)) >>> 0;
    const s1 = (rotr(schedule[index - 2]!, 17) ^ rotr(schedule[index - 2]!, 19) ^ (schedule[index - 2]! >>> 10)) >>> 0;
    schedule[index] = (schedule[index - 16]! + s0 + schedule[index - 7]! + s1) >>> 0;
  }

  let a = state[0]!;
  let b = state[1]!;
  let c = state[2]!;
  let d = state[3]!;
  let e = state[4]!;
  let f = state[5]!;
  let g = state[6]!;
  let h = state[7]!;
  for (let index = 0; index < 64; index += 1) {
    const s1 = (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) >>> 0;
    const choice = ((e & f) ^ (~e & g)) >>> 0;
    const temp1 = (h + s1 + choice + K[index]! + schedule[index]!) >>> 0;
    const s0 = (rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) >>> 0;
    const majority = ((a & b) ^ (a & c) ^ (b & c)) >>> 0;
    const temp2 = (s0 + majority) >>> 0;
    h = g;
    g = f;
    f = e;
    e = (d + temp1) >>> 0;
    d = c;
    c = b;
    b = a;
    a = (temp1 + temp2) >>> 0;
  }
  state[0] = (state[0]! + a) >>> 0;
  state[1] = (state[1]! + b) >>> 0;
  state[2] = (state[2]! + c) >>> 0;
  state[3] = (state[3]! + d) >>> 0;
  state[4] = (state[4]! + e) >>> 0;
  state[5] = (state[5]! + f) >>> 0;
  state[6] = (state[6]! + g) >>> 0;
  state[7] = (state[7]! + h) >>> 0;
}

/**
 * 增量 SHA-256。
 *
 * 一次只保留一个不完整块（≤64 字节）与 8 个状态字，因此内存工作集与正文大小无关。
 */
export class Sha256 {
  readonly #state = new Uint32Array(INITIAL);
  readonly #buffer = new Uint8Array(64);
  #bufferLength = 0;
  #totalBytes = 0;
  #finalized = false;

  /** 喂入一段字节；返回值便于链式调用（测试与调用点都是这个形态）。 */
  update(data: Uint8Array): this {
    if (this.#finalized) throw new Error("Sha256 已 finalize，不能继续 update");
    this.#totalBytes += data.length;

    let offset = 0;
    if (this.#bufferLength > 0) {
      const needed = 64 - this.#bufferLength;
      const take = Math.min(needed, data.length - offset);
      this.#buffer.set(data.subarray(offset, offset + take), this.#bufferLength);
      this.#bufferLength += take;
      offset += take;
      if (this.#bufferLength === 64) {
        compress(this.#state, this.#buffer, 0);
        this.#bufferLength = 0;
      }
    }
    while (offset + 64 <= data.length) {
      compress(this.#state, data, offset);
      offset += 64;
    }
    if (offset < data.length) {
      this.#buffer.set(data.subarray(offset), 0);
      this.#bufferLength = data.length - offset;
    }
    return this;
  }

  /** 结算并返回 32 字节摘要；只能调用一次。 */
  digest(): Uint8Array {
    if (this.#finalized) throw new Error("Sha256 只能 finalize 一次");
    this.#finalized = true;

    const totalBits = this.#totalBytes * 8;
    const tail = new Uint8Array(this.#bufferLength < 56 ? 64 : 128);
    tail.set(this.#buffer.subarray(0, this.#bufferLength), 0);
    tail[this.#bufferLength] = 0x80;
    // 长度字段是 64-bit 大端 **位** 数；JS number 在本客户端的单文件范围内安全（< 2^53 位）。
    const lengthOffset = tail.length - 8;
    const high = Math.floor(totalBits / 0x1_0000_0000);
    const low = totalBits >>> 0;
    tail[lengthOffset] = (high >>> 24) & 0xff;
    tail[lengthOffset + 1] = (high >>> 16) & 0xff;
    tail[lengthOffset + 2] = (high >>> 8) & 0xff;
    tail[lengthOffset + 3] = high & 0xff;
    tail[lengthOffset + 4] = (low >>> 24) & 0xff;
    tail[lengthOffset + 5] = (low >>> 16) & 0xff;
    tail[lengthOffset + 6] = (low >>> 8) & 0xff;
    tail[lengthOffset + 7] = low & 0xff;
    for (let offset = 0; offset < tail.length; offset += 64) compress(this.#state, tail, offset);

    const out = new Uint8Array(32);
    for (let index = 0; index < 8; index += 1) {
      const word = this.#state[index]!;
      out[index * 4] = (word >>> 24) & 0xff;
      out[index * 4 + 1] = (word >>> 16) & 0xff;
      out[index * 4 + 2] = (word >>> 8) & 0xff;
      out[index * 4 + 3] = word & 0xff;
    }
    return out;
  }
}

/** 一次性的便捷入口；大文件请用 {@link Sha256} 分块喂入。 */
export function sha256Bytes(data: Uint8Array): Uint8Array {
  return new Sha256().update(data).digest();
}
