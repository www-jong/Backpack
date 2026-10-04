export type ResourceKind = "skill" | "rule" | "tool" | "hook" | "mcp" | "plugin" | "setting";
export interface Detail { label: string; value: string }
export interface Resource { id: string; agentId: string; kind: ResourceKind; name: string; path: string; scope: string; status: string; source: string; origin: "user" | "bundled" | "unknown"; details: Detail[] }
export interface Agent { id: string; name: string; executable: string | null; configRoots: string[]; resources: Resource[]; warnings: string[]; custom: boolean; mcpEditing:boolean; inspection: "app-server" | "mcp-cli" | "file" }
export interface Snapshot { scannedAt: number; platform: string; home: string; projectPath: string | null; agents: Agent[] }
export interface ScanRequest { projectPath?: string; roots?: Record<string, string>; customAgents?: CustomAgent[] }
export interface QueryState { status: "success" | "partial" | "unsupported" | "error" | "skipped"; message: string }
export interface CodexMcpStatus { name: string; origin: "user" | "bundled" | "unknown"; authStatus: string; runtimeStatus: string | null; toolNames: string[]; toolsError: boolean; pluginId: string | null }
export interface CodexInspection { observedAt: number; version: string | null; executable: string; codexHome: string; cwd: string; context: string; configQuery: QueryState; skillsQuery: QueryState; mcpQuery: QueryState; settings: Detail[]; skills: Resource[]; mcpServers: CodexMcpStatus[] }

export interface CustomAgent { id: string; name: string; executable?: string; configRoot: string; configFiles: string[] }

export interface CliInspection { agentId:string; observedAt:number; version:string|null; executable:string; cwd:string; configRoot:string; query:QueryState; servers:{name:string;status:string;origin:"user"|"bundled"|"unknown"}[] }

export interface McpDraft { agentId:string; path:string; name:string; action:"register"|"enable"|"disable"; command:string; args:string[]; url:string; envNames:string[]; tokenEnv:string; enabled:boolean }
export interface ChangePreview {path:string;name:string;action:string;before:string;after:string;transport:string;envNames:string[];createsFile:boolean}
export interface BackupReceipt {id:string;path:string;name:string;action:string;createdAt:number;restorable:boolean;existed:boolean}
export interface EditorData {targets:string[];backups:BackupReceipt[];servers:Record<string,string[]>;envReference:boolean;tokenReference:boolean;notice:string}
export interface PreviewResult {token:string;change:ChangePreview}

export interface LibraryFile {path:string;hash:string;size:number}
export interface LibraryEntry {version:number;id:string;name:string;agentId:string;kind:ResourceKind;createdAt:number;files:LibraryFile[]}
export interface ImportPreview {name:string;kind:ResourceKind;agentId:string;files:LibraryFile[];skipped:string[];totalBytes:number;note:string}
export interface LibraryPreview {token:string;change:ImportPreview}
export interface LibraryComparison {identical:boolean;files:{path:string;status:"equal"|"changed"|"libraryOnly"|"localOnly"}[]}
