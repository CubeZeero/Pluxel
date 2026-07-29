import type { Package } from "../types";

/** True when running on macOS (WKWebView user agent contains "Mac"). */
export const isMacOS = /Mac/.test(navigator.userAgent);
/** True when running on Windows (WebView2 user agent contains "Windows"). */
export const isWindows = /Win/.test(navigator.userAgent);

/**
 * Effect plugins are platform-specific: `.aex` is a Windows binary, `.plugin`
 * is a macOS bundle. A package built for the other OS can be stored but not
 * installed here. Returns true when this package's effect can't run on the
 * current platform.
 */
export function isInstallBlocked(pkg: Package): boolean {
  if (pkg.kind !== "plugin") return false;
  const paths = pkg.files.map((f) => f.rel_path.toLowerCase());
  const hasAex = paths.some((p) => p.endsWith(".aex"));
  const hasMacPlugin = paths.some((p) => p.endsWith(".plugin") || p.includes(".plugin/"));
  if (isMacOS) return hasAex && !hasMacPlugin; // .aex is Windows-only
  if (isWindows) return hasMacPlugin && !hasAex; // .plugin is macOS-only
  return false;
}
