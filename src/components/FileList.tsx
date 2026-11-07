import { useState } from "react";
import { useT } from "../lib/i18n";
import { isBundleSeg, topSeg, primaryAssetPath } from "../lib/files";
import type { Package, PackageKind } from "../types";

function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// Documentation/manual formats — shown with a distinct icon; bundled for
// distribution but not installed into After Effects (see installer.rs).
const DOC_EXTS = new Set([
  "pdf", "doc", "docx", "rtf", "odt", "pages", "ppt", "pptx", "xls", "xlsx", "chm", "epub",
  "md", "txt", "html", "htm",
]);
const extOf = (p: string) => p.split(".").pop()?.toLowerCase() ?? "";
const isDoc = (p: string) => DOC_EXTS.has(extOf(p));
const fileIcon = (p: string) =>
  isDoc(p) ? (extOf(p) === "pdf" ? "bi-file-earmark-pdf" : "bi-file-earmark-text") : "bi-file-earmark";

type FileNode = { path: string; size: number; kind: PackageKind; bundle: boolean };

interface Props {
  pkg: Package;
  editing: boolean;
  onRemoveFile: (id: string, relPath: string) => void;
}

/**
 * Renders a package's stored files: the primary asset pinned and highlighted at
 * the top, `.plugin` bundles as single opaque items, and companion folders as
 * collapsible groups. In edit mode each entry gets a remove button (except the
 * primary asset).
 */
export function FileList({ pkg, editing, onRemoveFile }: Props) {
  const t = useT();
  // Folders start collapsed; a folder appears here once the user expands it.
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const toggleFolder = (folder: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      next.has(folder) ? next.delete(folder) : next.add(folder);
      return next;
    });

  const bundleNames = [
    ...new Set(pkg.files.map((f) => topSeg(f.rel_path)).filter((s): s is string => !!s && isBundleSeg(s))),
  ];
  const bundleNodes: FileNode[] = bundleNames.map((name) => ({
    path: name,
    bundle: true,
    kind: "plugin",
    size: pkg.files.filter((f) => f.rel_path.startsWith(name + "/")).reduce((s, f) => s + f.size, 0),
  }));
  const topFileNodes: FileNode[] = pkg.files
    .filter((f) => !f.rel_path.includes("/"))
    .map((f) => ({ path: f.rel_path, size: f.size, kind: f.kind, bundle: false }));

  // Primary asset (script/plugin body) — pinned to the top and highlighted.
  const flat = [...topFileNodes, ...bundleNodes];
  const primary = primaryAssetPath(pkg);
  let mainNode: FileNode | null = flat.find((n) => n.path === primary) ?? null;
  if (!mainNode && primary) {
    const source = pkg.files.filter((f) => f.rel_path === primary || f.rel_path.startsWith(primary + "/"));
    if (source.length) mainNode = {path: primary, size: source.reduce((n,f) => n + f.size,0), kind: pkg.kind, bundle: isBundleSeg(primary.split("/").pop() ?? "")};
  }
  const otherRows = flat.filter((n) => n !== mainNode);

  // Real folders (excluding bundles and the pinned main file).
  const folders = new Map<string, typeof pkg.files>();
  for (const f of pkg.files) {
    const seg = topSeg(f.rel_path);
    if (!seg || isBundleSeg(seg)) continue;
    if (mainNode && (f.rel_path === mainNode.path || f.rel_path.startsWith(mainNode.path + "/"))) continue;
    const arr = folders.get(seg) ?? [];
    arr.push(f);
    folders.set(seg, arr);
  }

  const delBtn = (relPath: string) =>
    editing && !(primary && (relPath === primary || primary.startsWith(relPath + "/") || relPath.startsWith(primary + "/"))) && (
      <button
        className="file-del"
        title={t("files.removeTip")}
        onClick={() => onRemoveFile(pkg.id, relPath)}
      >
        <i className="bi bi-x-lg" />
      </button>
    );

  return (
    <ul className="file-list">
      {mainNode && (
        <li className="file-item file-main">
          <span className="file-name">
            <i className="bi bi-star-fill" />
            {mainNode.path}
          </span>
          <span className="main-tag">{t("files.mainTag")}</span>
          <span className="file-size">{fmtSize(mainNode.size)}</span>
        </li>
      )}
      {otherRows.map((n) => (
        <li key={n.path} className="file-item">
          <span className="file-name">
            <i className={`bi ${n.bundle ? "bi-box-seam" : fileIcon(n.path)}`} />
            {n.path}
          </span>
          {!n.bundle && isDoc(n.path) && <span className="main-tag doc-tag">{t("files.docTag")}</span>}
          <span className="file-size">{fmtSize(n.size)}</span>
          {delBtn(n.path)}
        </li>
      ))}
      {[...folders.entries()].map(([folder, files]) => {
        const open = expanded.has(folder);
        return (
          <li key={folder} className="file-item file-folder">
            <div className="folder-row">
              <button
                className="folder-head"
                onClick={() => toggleFolder(folder)}
                aria-expanded={open}
              >
                <span className="file-name">
                  <i className={`bi ${open ? "bi-chevron-down" : "bi-chevron-right"} folder-caret`} />
                  <i className="bi bi-folder-fill" />
                  {folder}
                </span>
                <span className="folder-tag">
                  {t("files.folderTag")} · {files.length}
                </span>
              </button>
              {delBtn(folder)}
            </div>
            {open && (
              <ul className="folder-children">
                {files.map((f) => (
                  <li key={f.rel_path}>
                    <span className="file-name">
                      <i className={`bi ${fileIcon(f.rel_path)}`} />
                      {f.rel_path.slice(folder.length + 1)}
                    </span>
                    <span className="file-size">{fmtSize(f.size)}</span>
                    {delBtn(f.rel_path)}
                  </li>
                ))}
              </ul>
            )}
          </li>
        );
      })}
    </ul>
  );
}
