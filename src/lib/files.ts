import type { Package } from "../types";

// Extensions that are directory bundles on disk but treated as a single opaque
// item (a `.plugin` is a macOS bundle containing many inner files).
export const BUNDLE_EXTS = [".plugin"];

/** True when a top path segment is itself a directory bundle (e.g. `.plugin`). */
export const isBundleSeg = (seg: string) =>
  BUNDLE_EXTS.some((e) => seg.toLowerCase().endsWith(e));

/** First path segment of a `rel_path`, or null when the path has no separator. */
export const topSeg = (p: string) => {
  const i = p.indexOf("/");
  return i < 0 ? null : p.slice(0, i);
};

/**
 * Count files the way a user perceives them: every inner file of a `.plugin`
 * bundle collapses into a single count, while ordinary files each count once.
 */
export function countLogicalFiles(pkg: Package): number {
  let count = 0;
  const seenBundles = new Set<string>();
  for (const f of pkg.files) {
    const seg = topSeg(f.rel_path);
    if (seg && isBundleSeg(seg)) {
      if (!seenBundles.has(seg)) {
        seenBundles.add(seg);
        count += 1;
      }
    } else {
      count += 1;
    }
  }
  return count;
}
