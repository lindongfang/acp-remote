/**
 * 测试用的**最小** IndexedDB 替身。
 *
 * 只用在本目录的 Node 侧测试里：Node 22 没有内置 `indexedDB`，而被测端口只用到
 * `open` / 事务 / `get` / `put` / `delete` / `clear` / `openCursor` 这几项。
 *
 * 它不是 IndexedDB 的完整实现，也**不**声称实现对结构化克隆的保真：
 * 值按引用保存。真正的浏览器语义（结构化克隆 `CryptoKey` 后仍 `extractable === false`）
 * 由 `clients/app/scripts/run-browser-check.mjs` 在真实 Chromium 里执行
 * `testing/browser-checks.ts` 核对——这正是「不要把替身当成浏览器证据」的边界。
 */

/** 替身里的一条记录。 */
interface FakeRecord {
  key: string;
  value: unknown;
}

interface FakeStore {
  readonly records: FakeRecord[];
}

interface FakeDatabase {
  readonly stores: Map<string, FakeStore>;
}

/** 当前打开的库（每次 `installFakeIndexedDB()` 重置）。 */
let database: FakeDatabase | null = null;

/** 把 `indexedDB` 装到 `globalThis`，并重置状态。 */
export function installFakeIndexedDB(): void {
  database = { stores: new Map() };

  class FakeObjectStore {
    constructor(private readonly store: FakeStore) {}

    get(key: IDBValidKey): { onsuccess: (() => void) | null; onerror: (() => void) | null; result: unknown } {
      const request = { onsuccess: null as (() => void) | null, onerror: null as (() => void) | null, result: undefined as unknown };
      const found = this.store.records.find((record) => record.key === String(key));
      request.result = found?.value;
      queueMicrotask(() => request.onsuccess?.());
      return request;
    }

    put(value: unknown, key: IDBValidKey): void {
      const text = String(key);
      const existing = this.store.records.find((record) => record.key === text);
      if (existing !== undefined) existing.value = value;
      else this.store.records.push({ key: text, value });
    }

    delete(key: IDBValidKey): void {
      const text = String(key);
      const index = this.store.records.findIndex((record) => record.key === text);
      if (index >= 0) this.store.records.splice(index, 1);
    }

    clear(): void {
      this.store.records.length = 0;
    }

    openCursor(): {
      onsuccess: (() => void) | null;
      onerror: (() => void) | null;
      result: { value: unknown; continue: () => void } | null;
    } {
      const request = {
        onsuccess: null as (() => void) | null,
        onerror: null as (() => void) | null,
        result: null as { value: unknown; continue: () => void } | null,
      };
      const records = [...this.store.records];
      let index = 0;
      const step = () => {
        if (index >= records.length) {
          request.result = null;
          request.onsuccess?.();
          return;
        }
        const record = records[index];
        index += 1;
        request.result = { value: record?.value, continue: step };
        request.onsuccess?.();
      };
      queueMicrotask(step);
      return request;
    }
  }

  class FakeTransaction {
    oncomplete: (() => void) | null = null;
    onerror: (() => void) | null = null;
    onabort: (() => void) | null = null;

    private readonly store: FakeStore;

    constructor(db: FakeDatabase, storeName: string) {
      let store = db.stores.get(storeName);
      if (store === undefined) {
        store = { records: [] };
        db.stores.set(storeName, store);
      }
      this.store = store;
      queueMicrotask(() => queueMicrotask(() => this.oncomplete?.()));
    }

    objectStore(_name: string): FakeObjectStore {
      return new FakeObjectStore(this.store);
    }
  }

  const fake = {
    open(_name: string, _version?: number) {
      const request = {
        onsuccess: null as (() => void) | null,
        onerror: null as (() => void) | null,
        onupgradeneeded: null as (() => void) | null,
        onblocked: null as (() => void) | null,
        result: {
          objectStoreNames: { contains: (name: string) => database?.stores.has(name) ?? false },
          createObjectStore: (name: string) => {
            database?.stores.set(name, { records: [] });
          },
          transaction: (storeName: string, _mode: IDBTransactionMode) =>
            new FakeTransaction(database as FakeDatabase, storeName),
          close: () => {},
        },
      };
      queueMicrotask(() => request.onsuccess?.());
      return request;
    },
  };

  Object.defineProperty(globalThis, "indexedDB", { value: fake, configurable: true });
  Object.defineProperty(globalThis, "__fakeIndexedDBDump", {
    value: () => {
      const output: FakeRecord[] = [];
      for (const store of database?.stores.values() ?? []) output.push(...store.records);
      return output;
    },
    configurable: true,
  });
  Object.defineProperty(globalThis, "__fakeIndexedDBMutate", {
    value: (index: number, mutate: (value: unknown) => unknown) => {
      let cursor = 0;
      for (const store of database?.stores.values() ?? []) {
        for (const record of store.records) {
          if (cursor === index) record.value = mutate(record.value);
          cursor += 1;
        }
      }
    },
    configurable: true,
  });
}

declare global {
  // eslint-disable-next-line no-var
  var __fakeIndexedDBDump: () => readonly { key: string; value: unknown }[];
  // eslint-disable-next-line no-var
  var __fakeIndexedDBMutate: (index: number, mutate: (value: unknown) => unknown) => void;
}
