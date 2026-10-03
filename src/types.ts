export type ResourceKind = "skill" | "rule" | "tool" | "hook" | "mcp" | "plugin" | "setting";
export interface Detail { label: string; value: string }
export interface Resource { id: string; agentId: string; kind: ResourceKind; name: string; path: string; scope: string; status: string; source: string; origin: "user" | "bundled" | "unknown"; details: Detail[] }
export interface Agent { id: string; name: string; executable: string | null; configRoots: string[]; resources: Resource[]; warnings: string[] }
export interface Snapshot { scannedAt: number; platform: string; home: string; projectPath: string | null; agents: Agent[] }
export interface ScanRequest { projectPath?: string; roots?: Record<string, string> }
export interface QueryState { status: "success" | "partial" | "unsupported" | "error" | "skipped"; message: string }
export interface CodexMcpStatus { name: string; origin: "user" | "bundled" | "unknown"; authStatus: string; runtimeStatus: string | null; toolNames: string[]; toolsError: boolean; pluginId: string | null }
export interface CodexInspection { observedAt: number; version: string | null; executable: string; codexHome: string; cwd: string; context: string; configQuery: QueryState; skillsQuery: QueryState; mcpQuery: QueryState; settings: Detail[]; skills: Resource[]; mcpServers: CodexMcpStatus[] }
