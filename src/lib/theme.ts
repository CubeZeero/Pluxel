import { useCallback, useEffect, useState } from "react";
import type { PackageKind } from "../types";

export type Theme = "light" | "dark";

const THEME_KEY = "pluxel.theme";
const ACCENT_KEY = "pluxel.accent";
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

export const DEFAULT_ACCENT = "#6b8afd";
export const ACCENT_PRESETS = [
  "#6b8afd", // blue
  "#8a6bfd", // purple
  "#46c2c9", // teal
  "#3fbf7f", // green
  "#e6a13c", // amber
  "#f0616d", // red
  "#ec6bb0", // pink
];

function systemPref(): Theme {
  return window.matchMedia?.("(prefers-color-scheme: light)").matches
    ? "light"
    : "dark";
}

export function currentTheme(): Theme {
  const saved = localStorage.getItem(THEME_KEY);
  return saved === "light" || saved === "dark" ? saved : systemPref();
}

export function currentAccent(): string {
  return localStorage.getItem(ACCENT_KEY) || DEFAULT_ACCENT;
}

function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
}

/** Relative luminance of a #rrggbb color, 0 (dark) – 1 (light). */
function luminance(hex: string): number {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return 0;
  const n = parseInt(m[1], 16);
  const r = (n >> 16) & 255;
  const g = (n >> 8) & 255;
  const b = n & 255;
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255;
}

const HEX_COLOR = /^#[0-9a-f]{6}$/i;
/** Accept only `#rrggbb`; fall back otherwise (guards CSS vars set from
 * possibly-tampered localStorage against `url(...)`-style values). */
function safeColor(color: string, fallback: string): string {
  return HEX_COLOR.test((color ?? "").trim()) ? color.trim() : fallback;
}

function applyAccent(color: string) {
  const root = document.documentElement;
  const c = safeColor(color, DEFAULT_ACCENT);
  root.style.setProperty("--accent", c);
  // Pick a readable ink color for text/icons placed on the accent.
  root.style.setProperty("--accent-ink", luminance(c) > 0.62 ? "#12151c" : "#ffffff");
}

/* ---------- Per-kind colors ---------- */

/** Distinct, well-separated default colors for each package kind. */
export const KIND_COLOR_DEFAULTS: Record<PackageKind, string> = {
  script: "#5b8cff", // blue
  "script-ui-panel": "#2bc4b4", // teal
  plugin: "#f2a53c", // amber
  zxp: "#a978f5", // purple
  installer: "#ef5d63", // red
  unknown: "#8a93a6", // gray
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
    return { ...KIND_COLOR_DEFAULTS, ...saved };
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

/** Apply the persisted theme, accent and kind colors before React mounts. */
export function initTheme() {
  applyTheme(currentTheme());
  applyAccent(currentAccent());
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
    setColors((prev) => ({ ...prev, [kind]: color }));
  }, []);

  const reset = useCallback(() => setColors({ ...KIND_COLOR_DEFAULTS }), []);

  return { colors, setColor, reset };
}

/** Theme state hook with persistence. Returns `[theme, setTheme]`. */
export function useTheme(): [Theme, (t: Theme) => void] {
  const [theme, setTheme] = useState<Theme>(currentTheme);

  useEffect(() => {
    applyTheme(theme);
    localStorage.setItem(THEME_KEY, theme);
  }, [theme]);

  return [theme, setTheme];
}

/** Accent-color state hook with persistence. */
export function useAccent(): [string, (color: string) => void] {
  const [accent, setAccentState] = useState<string>(currentAccent);

  useEffect(() => {
    applyAccent(accent);
    localStorage.setItem(ACCENT_KEY, accent);
  }, [accent]);

  return [accent, setAccentState];
}
