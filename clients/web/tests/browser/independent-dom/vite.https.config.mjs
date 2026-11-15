import { readFileSync } from "node:fs";
import { defineConfig } from "vite";

const certificatePath = process.env.ACP_MAIN_FIXTURE_CERT_PATH;
const privateKeyPath = process.env.ACP_MAIN_FIXTURE_KEY_PATH;
if (!certificatePath || !privateKeyPath) {
  throw new Error("Set ACP_MAIN_FIXTURE_CERT_PATH and ACP_MAIN_FIXTURE_KEY_PATH to a trusted local test certificate and key.");
}

export default defineConfig({
  server: {
    host: "127.0.0.1",
    port: 4174,
    strictPort: true,
    https: {
      cert: readFileSync(certificatePath),
      key: readFileSync(privateKeyPath),
    },
  },
});
