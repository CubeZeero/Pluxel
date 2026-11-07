import { useCallback, useEffect, useState } from "react";
import type { PackageKind } from "../types";

export type Theme = "light" | "dark";
const THEME_KEY = "pluxel.theme";
const KIND_COLOR_KEY = "pluxel.kindColors";
const EFFECTS_FOLDER_KEY = "pluxel.effectsFolder";
const CUSTOM_INSTALL_KEY = "pluxel.customInstallDirs";

/** Subfolder under Plug-ins for effect plugins, or null when disabled. */
export function effectsSubdir(): string | null {
  return localStorage.getItem(EFFECTS_FOLDER_KEY) === "1" ? "Effects" : null;
}

/** Toggle: nest effect plugins into a Plug-ins/Effects/ subfolder. */
export function useEffectsFolder(): [boolean, (on: boolean) => void] {
  const [on, setOn] = useState<boolean>(
    () => localStorage.getItem(EFFECTS_FOLDER_KEY) === "1",
  );
  useEffect(() => {
    localStorage.setItem(EFFECTS_FOLDER_KEY, on ? "1" : "0");
  }, [on]);
  return [on, setOn];
}

/** User-configured custom install folders (set in Settings). */
export function customInstallDirs(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(CUSTOM_INSTALL_KEY) || "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

/** Persisted list of custom install folders. */
export function useCustomInstallDirs(): [string[], (dirs: string[]) => void] {
  const [dirs, setDirs] = useState<string[]>(() => customInstallDirs());
  useEffect(() => {
    localStorage.setItem(CUSTOM_INSTALL_KEY, JSON.stringify(dirs));
  }, [dirs]);
  return [dirs, setDirs];
}

export const KIND_COLOR_PRESETS = [
  "#6b8afd", // blue
  "#8a6bfd", // purple
  "#46c2c9", // teal
  "#3fbf7f", // green
  "#e6a13c", // amber
  "#f0616d", // red
  "#ec6bb0", // pink
];


function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}

const HEX_COLOR = /^#[0-9a-f]{6}$/i;
/** Accept only `#rrggbb`; fall back otherwise (guards CSS vars set from
 * possibly-tampered localStorage against `url(...)`-style values). */
function safeColor(color: unknown, fallback: string): string {
  return typeof color === "string" && HEX_COLOR.test(color.trim()) ? color.trim() : fallback;
}


/* ---------- Per-kind colors ---------- */

/** Distinct, well-separated default colors for each package kind. */
export const KIND_COLOR_DEFAULTS: Record<PackageKind, string> = {
  script: "#5b8cff",
  "script-ui-panel": "#2bc4b4",
  plugin: "#f2a53c",
  zxp: "#a978f5",
  installer: "#ef5d63",
  unknown: "#8a93a6",
};

const KIND_VARS: Record<PackageKind, string> = {
  script: "--kind-script",
  "script-ui-panel": "--kind-script-ui-panel",
  plugin: "--kind-plugin",
  zxp: "--kind-zxp",
  installer: "--kind-installer",
  unknown: "--kind-unknown",
};

export function currentKindColors(): Record<PackageKind, string> {
  try {
    const saved = JSON.parse(localStorage.getItem(KIND_COLOR_KEY) || "{}");
    return Object.fromEntries(Object.entries(KIND_COLOR_DEFAULTS).map(([key, fallback]) => [key, safeColor(saved?.[key], fallback)])) as Record<PackageKind, string>;
  } catch {
    return { ...KIND_COLOR_DEFAULTS };
  }
}

function applyKindColors(colors: Record<PackageKind, string>) {
  const root = document.documentElement;
  for (const k of Object.keys(KIND_VARS) as PackageKind[]) {
    root.style.setProperty(KIND_VARS[k], safeColor(colors[k], KIND_COLOR_DEFAULTS[k]));
  }
}

/** Apply appearance and validated classification colors before React mounts. */
export function initTheme() {
  applyTheme(currentTheme());
  applyKindColors(currentKindColors());
}

/** Per-kind color state with persistence. */
export function useKindColors() {
  const [colors, setColors] = useState<Record<PackageKind, string>>(currentKindColors);

  useEffect(() => {
    applyKindColors(colors);
    localStorage.setItem(KIND_COLOR_KEY, JSON.stringify(colors));
  }, [colors]);

  const setColor = useCallback((kind: PackageKind, color: string) => {
    setColors((prev) => ({ ...prev, [kind]: safeColor(color, KIND_COLOR_DEFAULTS[kind]) }));
  }, []);

  const reset = useCallback(() => setColors({ ...KIND_COLOR_DEFAULTS }), []);

  return { colors, setColor, reset };
}

export function useTheme(): [Theme, (theme: Theme) => void] {
  const [theme, setTheme] = useState<Theme>(currentTheme);
  useEffect(() => {
    applyTheme(theme);
    localStorage.setItem(THEME_KEY, theme);
    localStorage.removeItem("pluxel.mainColor");
    localStorage.removeItem("pluxel.accent");
  }, [theme]);
  return [theme, setTheme];
}

function currentTheme(): Theme {
  // Migrate the short-lived surface-color preference without losing appearance.
  const oldColor = localStorage.getItem("pluxel.mainColor");
  if (oldColor === "#f5f5f5") return "light";
  if (oldColor === "#101010") return "dark";
  return localStorage.getItem(THEME_KEY) === "light" ? "light" : "dark";
}
