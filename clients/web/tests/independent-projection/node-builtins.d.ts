// Narrow test-only declarations for Node's installed built-ins; no package or tsconfig change.
declare module "node:fs" {
  export function readFileSync(path: URL): Uint8Array;
}
declare module "node:crypto" {
  interface Sha256Hash {
    update(bytes: Uint8Array): Sha256Hash;
    digest(encoding: "hex"): string;
  }
  export function createHash(algorithm: "sha256"): Sha256Hash;
}
