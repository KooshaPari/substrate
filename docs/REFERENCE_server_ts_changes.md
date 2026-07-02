# server.ts Integration Changes for F3

## Phase 3: Apply these 4 changes to OmniRoute/open-sse/mcp-server/server.ts

### Change 1: Add Import (after line 83)

```typescript
import { pluginTools } from "./tools/pluginTools.ts";
import { dispatchTools } from "./tools/dispatchTools.ts";  // ← ADD THIS LINE
import { compressionTools } from "./tools/compressionTools.ts";
```

### Change 2: Update Tool Count (around line 114-123)

**Before:**
```typescript
const TOTAL_MCP_TOOL_COUNT =
  MCP_TOOLS.length +
  Object.keys(memoryTools).length +
  Object.keys(skillTools).length +
  Object.keys(agentSkillTools).length +
  Object.keys(poolTools).length +
  gamificationTools.length +
  pluginTools.length +
  notionTools.length +
  obsidianTools.length;
```

**After:**
```typescript
const TOTAL_MCP_TOOL_COUNT =
  MCP_TOOLS.length +
  Object.keys(memoryTools).length +
  Object.keys(skillTools).length +
  Object.keys(agentSkillTools).length +
  Object.keys(poolTools).length +
  gamificationTools.length +
  pluginTools.length +
  dispatchTools.length +           // ← ADD THIS LINE
  notionTools.length +
  obsidianTools.length;
```

### Change 3: Update RESERVED_MCP_NAMES Set (around line 891-901)

**Before:**
```typescript
const RESERVED_MCP_NAMES = new Set([
  ...MCP_TOOLS.map((t) => t.name),
  ...Object.keys(memoryTools),
  ...Object.keys(skillTools),
  ...Object.keys(compressionTools),
  ...Object.keys(poolTools),
  ...pluginTools.map((t) => t.name),
  ...gamificationTools.map((t) => t.name),
  ...obsidianTools.map((t) => t.name),
  ...notionTools.map((t) => t.name),
]);
```

**After:**
```typescript
const RESERVED_MCP_NAMES = new Set([
  ...MCP_TOOLS.map((t) => t.name),
  ...Object.keys(memoryTools),
  ...Object.keys(skillTools),
  ...Object.keys(compressionTools),
  ...Object.keys(poolTools),
  ...pluginTools.map((t) => t.name),
  ...dispatchTools.map((t) => t.name),     // ← ADD THIS LINE
  ...gamificationTools.map((t) => t.name),
  ...obsidianTools.map((t) => t.name),
  ...notionTools.map((t) => t.name),
]);
```

### Change 4: Register Tools (after pluginTools registration, around line 1316)

Find the section:
```typescript
  });

  // ── Compression Tools ─────────────────────────
```

Insert **before** the Compression Tools comment:

```typescript
  });

  // ── Dispatch Tools (Substrate) ────────────────
  dispatchTools.forEach((toolDef) => {
    server.registerTool(
      toolDef.name,
      {
        description: toolDef.description,
        // @ts-ignore: dynamic zod access
        inputSchema: toolDef.inputSchema,
      },
      withScopeEnforcement(
        toolDef.name,
        async (args) => {
          try {
            const parsedArgs = toolDef.inputSchema.parse(args ?? {});
            // @ts-ignore: handler type lost through dynamic array access
            const result = await toolDef.handler(parsedArgs, { callerContext: "omniroute_mcp" });
            return { content: [{ type: "text" as const, text: JSON.stringify(result, null, 2) }] };
          } catch (err) {
            const msg = err instanceof Error ? err.message : String(err);
            return { content: [{ type: "text" as const, text: `Error: ${msg}` }], isError: true };
          }
        }
      )
    );
  });

  // ── Compression Tools ─────────────────────────
```

---

## Validation Checklist

After making changes:

1. **Copy dispatchTools.ts to OmniRoute:**
   ```bash
   cp docs/REFERENCE_dispatchTools.ts OmniRoute/open-sse/mcp-server/tools/dispatchTools.ts
   ```

2. **Type check:**
   ```bash
   cd OmniRoute
   npm run typecheck:core
   ```

3. **Lint:**
   ```bash
   npm run lint
   ```

4. **Run live test:**
   ```bash
   # Terminal 1: Start substrate HTTP server
   cd substrate
   cargo run --bin substrate-http --release

   # Terminal 2: Start OmniRoute
   cd OmniRoute
   export SUBSTRATE_HTTP_URL=http://localhost:8000
   npm run dev

   # Terminal 3: Test via MCP client
   # Tool: substrate_dispatch
   # Input: { prompt: "Print: F3-OK", tier: "main" }
   # Expected: { text: "F3-OK\n", status: "completed", ... }
   ```

5. **Commit & PR:**
   ```bash
   git add open-sse/mcp-server/tools/dispatchTools.ts open-sse/mcp-server/server.ts
   git commit -m "feat(mcp): add substrate dispatch tools (F3 integration)"
   git push -u origin feat/dispatch-f3-integration
   ```
