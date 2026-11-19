import type { JsonValue } from "../protocol/jws";
import {
  requireSessionId,
  requireSessionU64,
} from "./session-protocol";

export class CatalogActionError extends Error {
  readonly code: "upload_failed" | "preparation_timeout" | "request_rejected";
  readonly detail: string | null;

  constructor(
    code: "upload_failed" | "preparation_timeout" | "request_rejected",
    detail: string | null = null,
  ) {
    super(detail === null ? code : `${code}: ${detail}`);
    this.name = "CatalogActionError";
    this.code = code;
    this.detail = detail;
  }
}

export interface CatalogLaunchEnvInput {
  readonly name: string;
  readonly value: string;
}

export interface CatalogAdditionalDirectoryInput {
  readonly workspaceId: string;
  readonly path: string;
}

export interface CatalogProfileRecipeInput {
  readonly wire: 1 | 2;
  readonly mcpServers?: readonly JsonValue[];
  readonly additionalDirectories?: readonly CatalogAdditionalDirectoryInput[];
  readonly extra?: Record<string, JsonValue>;
}

export interface InstallHarnessDocumentInput {
  readonly name: string;
  readonly source:
    | { readonly kind: "new"; readonly name: string }
    | { readonly kind: "existing"; readonly sourceId: string };
  readonly executable: string;
  readonly args?: readonly string[];
  readonly environment?: readonly CatalogLaunchEnvInput[];
  readonly recipes?: readonly CatalogProfileRecipeInput[];
}

export interface BindTargetInitialDocumentInput {
  readonly targetId: string;
  readonly sourceId: string;
  readonly harnessId: string;
  readonly profileId: string;
  readonly profileRevision: string;
  readonly workspaceId: string;
  readonly launchPath: string;
}

export function utf16leBase64(value: string): string {
  const bytes = new Uint8Array(value.length * 2);
  for (let i = 0; i < value.length; i += 1) {
    const code = value.charCodeAt(i);
    bytes[i * 2] = code & 0xff;
    bytes[i * 2 + 1] = code >>> 8;
  }
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return globalThis.btoa(binary);
}

export function buildInstallHarnessDocument(
  input: InstallHarnessDocumentInput,
): Record<string, unknown> {
  const recipesInput: readonly CatalogProfileRecipeInput[] =
    input.recipes !== undefined && input.recipes.length > 0
      ? input.recipes
      : [{ wire: 2 }];
  return {
    kind: "install_harness",
    name: input.name,
    source:
      input.source.kind === "existing"
        ? {
            kind: "existing",
            sourceId: requireSessionId(input.source.sourceId),
          }
        : {
            kind: "new",
            name: input.source.name,
          },
    profile: {
      format: 1,
      launch: {
        executable_utf16le_base64: utf16leBase64(input.executable),
        arguments_utf16le_base64: (input.args ?? []).map(utf16leBase64),
        environment: (input.environment ?? []).map((entry) => ({
          name_utf16le_base64: utf16leBase64(entry.name),
          value_utf16le_base64: utf16leBase64(entry.value),
        })),
      },
      recipes: recipesInput.map((recipe) => ({
        wire: recipe.wire,
        mcp_servers: recipe.mcpServers ?? [],
        additional_directories: (recipe.additionalDirectories ?? []).map(
          (dir) => ({
            workspace_id: requireSessionId(dir.workspaceId),
            path: dir.path,
          }),
        ),
        extra: recipe.extra ?? {},
      })),
    },
  };
}

export function buildBindTargetInitialDocument(
  input: BindTargetInitialDocumentInput,
): Record<string, unknown> {
  return {
    kind: "bind_target_initial",
    targetId: requireSessionId(input.targetId),
    sourceId: requireSessionId(input.sourceId),
    harnessId: requireSessionId(input.harnessId),
    profileId: requireSessionId(input.profileId),
    profileRevision: requireSessionU64(input.profileRevision),
    workspaceId: requireSessionId(input.workspaceId),
    launchPath: input.launchPath,
  };
}
