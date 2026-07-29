/**
 * Normalize a user/package-supplied homepage into a safe external URL. Only
 * http/https/mailto are allowed through as-is; anything else (a bare host, or a
 * dangerous scheme like `file:`, `smb:`, `javascript:` or an app protocol
 * handler from a malicious package) is coerced to `https://` so the OS opener
 * never receives an unexpected scheme.
 */
export function toUrl(raw: string): string {
  const s = raw.trim();
  const scheme = /^([a-z][a-z0-9+.-]*):/i.exec(s)?.[1]?.toLowerCase();
  if (scheme === "http" || scheme === "https" || scheme === "mailto") return s;
  return `https://${s.replace(/^[a-z][a-z0-9+.-]*:\/*/i, "")}`;
}

/** Strip characters illegal in filenames; fall back if nothing usable remains. */
export function sanitizeFilename(name: string, fallback: string): string {
  return name.replace(/[/\\:*?"<>|]/g, "_") || fallback;
}
