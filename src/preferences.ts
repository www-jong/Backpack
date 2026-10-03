export type ThemePreference = "system" | "light" | "dark";
export interface Preferences {
  theme: ThemePreference;
  showBundled: boolean;
  scanOnStartup: boolean;
}
const key = "backpack.preferences.v1";
export const defaultPreferences: Preferences = { theme: "system", showBundled: false, scanOnStartup: true };
export function readPreferences(): Preferences {
  try {
    const saved = JSON.parse(localStorage.getItem(key) ?? "null");
    if (!saved || typeof saved !== "object") return { ...defaultPreferences };
    return {
      theme: ["system", "light", "dark"].includes(saved.theme) ? saved.theme : "system",
      showBundled: typeof saved.showBundled === "boolean" ? saved.showBundled : false,
      scanOnStartup: typeof saved.scanOnStartup === "boolean" ? saved.scanOnStartup : true,
    };
  } catch { return { ...defaultPreferences }; }
}
export function savePreferences(preferences: Preferences): boolean {
  try { localStorage.setItem(key, JSON.stringify(preferences)); return true; }
  catch { return false; }
}
export function applyTheme(theme: ThemePreference) {
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme === "system" ? "light dark" : theme;
}
