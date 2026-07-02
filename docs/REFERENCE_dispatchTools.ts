/**
 * REFERENCE IMPLEMENTATION: Substrate Dispatch Tools for OmniRoute MCP
 *
 * Copy this file to OmniRoute/open-sse/mcp-server/tools/dispatchTools.ts
 * for Phase 3 integration.
 *
 * See F3_OMNIROUTE_INTEGRATION.md for full integration instructions.
 */

import { logToolCall } from "../audit.ts";
import type { McpToolExtraLike } from "../scopeEnforcement.ts";

const SUBSTRATE_HTTP_URL = process.env.SUBSTRATE_HTTP_URL || "";

interface DispatchRequest {
  prompt: string;
  tier?: "heavy" | "main" | "worker" | string;
  engine?: "forge" | "codex" | "claude" | "agentapi" | string;
  cwd?: string;
  mode?: "background" | "foreground" | "in_process";
}

interface DispatchResponse {
  text: string;
  artifacts: Array<{ name: string; uri: string }>;
  pr_urls: string[];
  status: "submitted" | "running" | "completed" | "failed";
  error?: string;
}

async function callSubstrateApi(path: string, body: object): Promise<unknown> {
  if (!SUBSTRATE_HTTP_URL) {
    throw new Error(
      "SUBSTRATE_HTTP_URL environment variable not set. Set it to substrate HTTP server URL (e.g., http://localhost:8000)",
    );
  }

  const url = `${SUBSTRATE_HTTP_URL}${path}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(120000), // 2 min timeout for long-running tasks
  });

  if (!response.ok) {
    const text = await response.text().catch(() => "Unknown error");
    throw new Error(`Substrate API [${response.status}]: ${text}`);
  }

  return response.json();
}

/**
 * Dispatch a prompt to substrate with optional tier/engine routing.
 * Requires SUBSTRATE_HTTP_URL env var pointing to substrate HTTP server.
 */
export async function handleSubstrateDispatch(
  prompt: string,
  tier?: string,
  engine?: string,
  cwd?: string,
): Promise<DispatchResponse> {
  if (!prompt || !prompt.trim()) {
    throw new Error("prompt must not be empty");
  }

  const req: DispatchRequest = {
    prompt,
    cwd: cwd || process.cwd(),
  };

  if (tier) req.tier = tier;
  if (engine) req.engine = engine;

  const result = await callSubstrateApi("/v1/dispatch", req);
  return result as DispatchResponse;
}

/**
 * Dry-run: return dispatch plan without spawning an engine.
 * Useful for introspection before committing to a dispatch.
 */
export async function handleSubstratePlan(
  prompt: string,
  engine?: string,
  cwd?: string,
): Promise<object> {
  if (!prompt || !prompt.trim()) {
    throw new Error("prompt must not be empty");
  }

  const req: Omit<DispatchRequest, "tier" | "mode"> = {
    prompt,
    cwd: cwd || process.cwd(),
  };

  if (engine) req.engine = engine;

  return callSubstrateApi("/v1/plan", req);
}

/**
 * Health check: verify substrate HTTP server is reachable.
 */
export async function handleSubstrateHealth(): Promise<object> {
  if (!SUBSTRATE_HTTP_URL) {
    return {
      status: "misconfigured",
      message: "SUBSTRATE_HTTP_URL not set",
    };
  }

  try {
    const response = await fetch(`${SUBSTRATE_HTTP_URL}/healthz`, {
      signal: AbortSignal.timeout(5000),
    });
    return {
      status: response.ok ? "healthy" : "unhealthy",
      http_status: response.status,
    };
  } catch (e) {
    return {
      status: "unreachable",
      error: String(e),
    };
  }
}

export const dispatchTools: McpToolExtraLike[] = [
  {
    name: "substrate_dispatch",
    description:
      "Dispatch a prompt to substrate with optional tier-based model routing (heavy/main/worker). Requires SUBSTRATE_HTTP_URL env var pointing to running substrate HTTP server.",
    inputSchema: {
      type: "object",
      properties: {
        prompt: {
          type: "string",
          description: "The prompt or task to dispatch",
        },
        tier: {
          type: "string",
          enum: ["heavy", "main", "worker"],
          description: "Model tier: heavy (gpt-5.5, reasoning), main (gpt-5.4-mini), worker (gpt-5.3-codex-spark)",
        },
        engine: {
          type: "string",
          enum: ["forge", "codex", "claude", "agentapi"],
          description: "Execution engine (default: forge)",
        },
        cwd: {
          type: "string",
          description: "Working directory for the dispatch (default: current working directory)",
        },
      },
      required: ["prompt"],
    },
    handler: async (
      input: Record<string, unknown>,
      extra?: Record<string, unknown>,
    ): Promise<object> => {
      const { prompt, tier, engine, cwd } = input;
      await logToolCall("substrate_dispatch", input, extra);
      return handleSubstrateDispatch(
        String(prompt),
        tier ? String(tier) : undefined,
        engine ? String(engine) : undefined,
        cwd ? String(cwd) : undefined,
      );
    },
  },
  {
    name: "substrate_plan",
    description:
      "Dry-run: return the dispatch plan without spawning an engine. Useful for introspection before committing to a dispatch.",
    inputSchema: {
      type: "object",
      properties: {
        prompt: {
          type: "string",
          description: "The prompt or task to plan",
        },
        engine: {
          type: "string",
          enum: ["forge", "codex", "claude", "agentapi"],
          description: "Execution engine (default: forge)",
        },
        cwd: {
          type: "string",
          description: "Working directory for the dispatch (default: current working directory)",
        },
      },
      required: ["prompt"],
    },
    handler: async (
      input: Record<string, unknown>,
      extra?: Record<string, unknown>,
    ): Promise<object> => {
      const { prompt, engine, cwd } = input;
      await logToolCall("substrate_plan", input, extra);
      return handleSubstratePlan(
        String(prompt),
        engine ? String(engine) : undefined,
        cwd ? String(cwd) : undefined,
      );
    },
  },
  {
    name: "substrate_health",
    description: "Check substrate HTTP server health. Requires SUBSTRATE_HTTP_URL env var.",
    inputSchema: {
      type: "object",
      properties: {},
    },
    handler: async (
      _input: Record<string, unknown>,
      extra?: Record<string, unknown>,
    ): Promise<object> => {
      await logToolCall("substrate_health", {}, extra);
      return handleSubstrateHealth();
    },
  },
];
