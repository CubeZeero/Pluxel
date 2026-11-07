import { useEffect, useRef, useState, type ReactNode } from "react";
import { Modal } from "./Modal";
import { api } from "../lib/api";
import { useI18n, useT } from "../lib/i18n";
import { KIND_I18N, type PreparedImport, type ActivityEvent, type IntegrityEntry, type Revision, type Package, type InstallConflict, type PackageFile } from "../types";
import "../Maintenance.css";

/** Adds focus containment without changing the shared modal. */
function MaintenanceModal({ title, onClose, children, busy = false }: {
  title: string; onClose: () => void; children: ReactNode; busy?: boolean;
}) {
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous = document.activeElement;
    const dialog = root.current?.closest<HTMLElement>('[role="dialog"]');
    if (!dialog) return;
    const previousLabel = dialog.getAttribute("aria-label");
    dialog.setAttribute("aria-label", title);
    const controls = () => Array.from(dialog.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), summary, [tabindex="0"]',
    )).filter((element) => element.getClientRects().length > 0);
    // Keep long results at the top. Focusing the footer used to scroll the
    // entire dialog past its title and introductory explanation.
    controls()[0]?.focus({ preventScroll: true });
    const trap = (event: KeyboardEvent) => {
      if (event.key !== "Tab") return;
      const items = controls();
      const index = items.indexOf(document.activeElement as HTMLElement);
      if (!items.length) { event.preventDefault(); return; }
      if (event.shiftKey && index <= 0) { event.preventDefault(); items[items.length - 1].focus(); }
      else if (!event.shiftKey && (index < 0 || index === items.length - 1)) { event.preventDefault(); items[0].focus(); }
    };
    dialog.addEventListener("keydown", trap);
    return () => {
      dialog.removeEventListener("keydown", trap);
      if (previousLabel === null) dialog.removeAttribute("aria-label");
      else dialog.setAttribute("aria-label", previousLabel);
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, [title]);
  return <Modal title={title} onClose={() => { if (!busy) onClose(); }} width={660}>
    <div ref={root} className="maintenance" aria-busy={busy}>{children}</div>
  </Modal>;
}

function Timestamp({ value }: { value: string }) {
  const { lang } = useI18n();
  const date = new Date(value);
  return <time dateTime={Number.isNaN(date.getTime()) ? undefined : value}>
    {Number.isNaN(date.getTime()) ? value : date.toLocaleString(lang === "ja" ? "ja-JP" : "en-US")}
  </time>;
}

function PreviewFiles({ files }: { files: PackageFile[] }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [limit, setLimit] = useState(50);
  return <details onToggle={(event) => setOpen(event.currentTarget.open)}>
    <summary>{t("maintenance.files", { count: files.length })}</summary>
    {open && <><ul className="maintenance-files" tabIndex={0} aria-label={t("maintenance.files", { count: files.length })}>
      {files.slice(0, limit).map((file, i) => <li key={`${file.rel_path}-${i}`}><code>{file.rel_path}</code><span>{file.size.toLocaleString()} B</span></li>)}
    </ul>{limit < files.length && <button className="btn ghost xs" onClick={() => setLimit(limit + 100)}>{t("maintenance.more", {count: files.length - limit})}</button>}</>}
  </details>;
}

export function ImportPreviewModal({ preview, busy, onConfirm, onCancel }: {
  preview: PreparedImport; busy: boolean; onConfirm: () => void; onCancel: () => void;
}) {
  const t = useT();
  const [limit, setLimit] = useState(20);
  return <MaintenanceModal title={t("maintenance.importTitle")} onClose={onCancel} busy={busy}>
    <p>{t("maintenance.importCount", { packages: preview.packages.length, bundles: preview.bundles.length })}</p>
    <p className="maintenance-notice">{t("maintenance.noScripts")}</p>
    {preview.packages.slice(0, limit).map((pkg, index) => <section className="maintenance-card" key={`${pkg.id}-${index}`}>
      <h3>{pkg.manifest.name || t("untitled")}</h3>
      <dl className="maintenance-meta">
        {[["field.kind", t(KIND_I18N[pkg.kind] ?? "kind.unknown")], ["field.author", pkg.manifest.author],
          ["field.version", pkg.manifest.version], ["field.description", pkg.manifest.description],
          ["field.homepage", pkg.manifest.homepage]].map(([key, value]) => <div key={key}><dt>{t(key)}</dt><dd>{value || "—"}</dd></div>)}
      </dl>
      <PreviewFiles files={pkg.files} />
    </section>)}
    {limit < preview.packages.length && <button className="btn ghost" onClick={() => setLimit(limit + 20)}>{t("maintenance.more", {count: preview.packages.length - limit})}</button>}
    {preview.bundles.map((bundle, index) => <section className="maintenance-card" key={`${bundle.id}-${index}`}>
      <h3>{t("maintenance.bundle", { name: bundle.name })}</h3>
      <dl className="maintenance-meta">{[["field.author", bundle.author], ["field.description", bundle.description], ["field.homepage", bundle.homepage]].map(([key, value]) => <div key={key}><dt>{t(key)}</dt><dd>{value || "—"}</dd></div>)}</dl>
      <details><summary>{t("bundle.members", { count: bundle.package_ids.length })}</summary>
        <ul className="maintenance-files">{bundle.package_ids.map((id, i) => <li key={`${id}-${i}`}>{preview.packages.find((pkg) => pkg.id === id)?.manifest.name || id}</li>)}</ul>
      </details>
    </section>)}
    <div className="modal-actions"><button className="btn ghost" data-initial-focus disabled={busy} onClick={onCancel}>{t("common.cancel")}</button>
      <button className="btn" disabled={busy || (!preview.packages.length && !preview.bundles.length)} onClick={onConfirm}>{t(busy ? "maintenance.working" : "maintenance.import")}</button></div>
  </MaintenanceModal>;
}

const ACTION_KEYS: Record<string, string> = {
  import: "import", import_package: "import", install: "install", uninstall: "uninstall", restore: "restore",
  restore_revision: "restore", update: "update", replace: "update", delete: "delete", remove: "delete",
  export: "export", run_installer: "runInstaller", add_files: "addFiles", replace_files: "update", remove_file: "removeFile",
  runInstaller: "runInstaller", addFiles: "addFiles", removeFile: "removeFile",
};

export function HistoryModal({ events, onClose }: { events: ActivityEvent[]; onClose: () => void }) {
  const t = useT();
  const ordered = events.map((event, index) => ({ event, index, at: Date.parse(event.at) }))
    .sort((a, b) => (Number.isNaN(b.at) ? -Infinity : b.at) - (Number.isNaN(a.at) ? -Infinity : a.at) || a.index - b.index);
  return <MaintenanceModal title={t("maintenance.historyTitle")} onClose={onClose}>
    {!events.length && <p>{t("maintenance.noHistory")}</p>}
    <ol className="maintenance-list">{ordered.map(({ event, index }) => <li className="maintenance-card" key={`${event.id}-${index}`}>
      <div className="maintenance-row"><strong>{Object.prototype.hasOwnProperty.call(ACTION_KEYS, event.action) ? t(`maintenance.action.${ACTION_KEYS[event.action]}`) : t("maintenance.unknownAction", { action: event.action })}</strong><Timestamp value={event.at} /></div>
      <p>{event.name} {event.version}</p>{event.detail && <p className="maintenance-detail">{event.detail}</p>}
    </li>)}</ol>
    <div className="modal-actions"><button className="btn ghost" data-initial-focus onClick={onClose}>{t("common.close")}</button></div>
  </MaintenanceModal>;
}

const STATUSES: readonly string[] = ["ok", "missing", "changed", "unverified", "unreadable", "unsupported"];

export function IntegrityModal({ entries, onClose }: { entries: IntegrityEntry[]; onClose: () => void }) {
  const t = useT();
  const [message, setMessage] = useState<{ error: boolean; text: string } | null>(null);
  const [pending, setPending] = useState(false);
  async function operate(path: string, reveal: boolean) {
    setPending(true); setMessage(null);
    try {
      if (reveal) await api.revealPath(path);
      else await navigator.clipboard.writeText(path);
      if (!reveal) setMessage({ error: false, text: t("maintenance.copied") });
    } catch (error) {
      setMessage({ error: true, text: t("maintenance.pathError", { error: error instanceof Error ? error.message : String(error) }) });
    } finally { setPending(false); }
  }
  return <MaintenanceModal title={t("maintenance.integrityTitle")} onClose={onClose}>
    {entries.some((entry) => entry.status === "unverified") && <p className="maintenance-notice">{t("maintenance.legacyBaseline")}</p>}
    {!entries.length && <p>{t("maintenance.noIntegrity")}</p>}
    {message && <p role={message.error ? "alert" : "status"} className="maintenance-notice">{message.text}</p>}
    <ul className="maintenance-list">{entries.map((entry, index) => <li className="maintenance-card" key={`${entry.record_id}-${entry.path}-${index}`}>
      <div className="maintenance-row"><strong>{entry.package_name}</strong><span className="maintenance-status">{t(`maintenance.status.${STATUSES.includes(entry.status) ? entry.status : "unknown"}`)}</span></div>
      <p>{entry.label}</p><code className="maintenance-path">{entry.path}</code>
      {entry.detail && <p className="maintenance-detail">{entry.detail}</p>}
      <div className="maintenance-row"><button className="btn ghost xs" disabled={pending} onClick={() => { void operate(entry.path, false); }}>{t("install.copyPath")}</button>
        <button className="btn ghost xs" disabled={pending} onClick={() => { void operate(entry.path, true); }}>{t("install.reveal")}</button></div>
    </li>)}</ul>
    <div className="modal-actions"><button className="btn ghost" data-initial-focus onClick={onClose}>{t("common.close")}</button></div>
  </MaintenanceModal>;
}

export function RevisionsModal({ pkg, revisions, busy, onRestore, onClose }: {
  pkg: Package; revisions: Revision[]; busy: boolean; onRestore: (revisionId: string, reinstall: boolean) => void; onClose: () => void;
}) {
  const t = useT();
  const canReinstall = pkg.kind !== "installer" && pkg.installs.some((install) => install.paths.length > 0);
  const [choice, setChoice] = useState({ packageId: pkg.id, reinstall: canReinstall });
  const reinstall = canReinstall && (choice.packageId === pkg.id ? choice.reinstall : true);
  return <MaintenanceModal title={t("maintenance.revisionsTitle")} onClose={onClose} busy={busy}>
    <h3>{pkg.manifest.name}</h3><p>{t("maintenance.restoreExplanation")}</p>
    <p className="maintenance-notice">{t("maintenance.restoreFiles")}</p>
    {pkg.kind === "installer" ? <p>{t("maintenance.installerReadOnly")}</p> : <label className="maintenance-check"><input type="checkbox" checked={reinstall} disabled={busy || !canReinstall} onChange={(event) => setChoice({ packageId: pkg.id, reinstall: event.target.checked })} />{t("maintenance.reinstall")}</label>}
    {!canReinstall && pkg.kind !== "installer" && <p>{t("maintenance.noInstallPaths")}</p>}
    {!revisions.length && <p>{t("maintenance.noRevisions")}</p>}
    <ul className="maintenance-list">{revisions.map((revision) => <li className="maintenance-card" key={revision.id}>
      <div className="maintenance-row"><strong>{revision.name} {revision.version}</strong><Timestamp value={revision.created_at} /></div>
      <p className="maintenance-detail">{t(`maintenance.revisionReason.${({replace_files: "files", replace_package: "import", metadata_version: "version", "Before restore (redo)": "redo"} as Record<string,string>)[revision.reason] ?? "backup"}`)}</p>
      <button className="btn ghost" disabled={busy} onClick={() => onRestore(revision.id, reinstall)}>{t(busy ? "maintenance.working" : "maintenance.restore")}</button>
    </li>)}</ul>
    <div className="modal-actions"><button className="btn ghost" data-initial-focus disabled={busy} onClick={onClose}>{t("common.close")}</button></div>
  </MaintenanceModal>;
}

const REASONS: readonly string[] = ["owned", "existing", "modified", "selection"];

export function ConflictModal({ conflicts, onResult }: { conflicts: InstallConflict[]; onResult: (allow: boolean) => void }) {
  const t = useT();
  const selection = conflicts.some((conflict) => conflict.reason === "selection");
  return <MaintenanceModal title={t("maintenance.conflictTitle")} onClose={() => onResult(false)}>
    <p className="maintenance-notice">{t(selection ? "maintenance.selectionConflict" : "maintenance.overwriteWarning")}</p>
    <ul className="maintenance-list maintenance-conflicts" tabIndex={0} aria-label={t("maintenance.conflictTitle")}>{conflicts.map((conflict, index) => <li className="maintenance-card" key={`${conflict.path}-${index}`}>
      <p><strong>{conflict.package_name}</strong>{conflict.other_name && <> / {conflict.other_name}</>}</p>
      <code className="maintenance-path">{conflict.path}</code>
      <p>{t(`maintenance.reason.${REASONS.includes(conflict.reason) ? conflict.reason : "unknown"}`)}</p>
    </li>)}</ul>
    <div className="modal-actions"><button className="btn ghost" data-initial-focus onClick={() => onResult(false)}>{t("common.cancel")}</button>
      {!selection && conflicts.length > 0 && <button className="btn" onClick={() => onResult(true)}>{t("maintenance.overwrite")}</button>}</div>
  </MaintenanceModal>;
}
