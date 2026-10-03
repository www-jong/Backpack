import type { CustomAgent } from "./types";
const key = "backpack.custom-agents.v1";
export function readCustomAgents(): CustomAgent[] {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(key) ?? "[]");
    if (!Array.isArray(saved)) return [];
    return saved.slice(0,32).filter((a): a is CustomAgent => a && typeof a.id === "string" && /^custom-[a-z0-9-]+$/.test(a.id) && typeof a.name === "string" && typeof a.configRoot === "string" && (a.executable === undefined || typeof a.executable === "string") && Array.isArray(a.configFiles) && a.configFiles.every((f: unknown) => typeof f === "string"));
  } catch { return []; }
}
export function saveCustomAgents(agents: CustomAgent[]): boolean {
  try { localStorage.setItem(key,JSON.stringify(agents)); return true; } catch { return false; }
}
