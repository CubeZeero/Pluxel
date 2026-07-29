import { useEffect, useState } from "react";
import { api } from "./api";

// Module-level cache of loaded banner data URLs, keyed by id + version. This
// avoids re-fetching (and the flash of the fallback) when the same banner is
// shown again — e.g. switching the detail panel to a package whose card
// already loaded the banner.
const cache = new Map<string, string | null>();
const keyOf = (id: string, version?: string) => `${id}::${version ?? ""}`;

/**
 * Lazily load a package (or bundle) banner as a data URL. `version` (the
 * `updated_at`) is part of the cache key so a replaced banner re-loads.
 */
export function useBanner(
  id: string,
  banner: string | null | undefined,
  version?: string,
  reader: (id: string) => Promise<string | null> = api.readBanner,
) {
  const k = keyOf(id, version);
  // Seed from cache so a warm banner renders immediately (no flicker).
  const [src, setSrc] = useState<string | null>(() =>
    banner ? cache.get(k) ?? null : null,
  );

  useEffect(() => {
    let alive = true;
    if (!banner) {
      setSrc(null);
      return;
    }
    if (cache.has(k)) {
      setSrc(cache.get(k) ?? null);
      return;
    }
    reader(id)
      .then((s) => {
        cache.set(k, s);
        if (alive) setSrc(s);
      })
      .catch(() => {
        if (alive) setSrc(null);
      });
    return () => {
      alive = false;
    };
  }, [id, banner, k, reader]);

  return src;
}
