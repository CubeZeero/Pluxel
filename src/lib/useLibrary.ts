import { useCallback, useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api, type InstallParams } from "./api";
import { ASSET_FILTER, IMAGE_FILTER, PPF_EXT, PPF_FILTER, PPF_IMPORT_FILTER } from "./constants";
import { sanitizeFilename } from "./text";
import { useT } from "./i18n";
import { effectsSubdir } from "./theme";
import type { AeInstallation, Bundle, Manifest, Package, PreparedImport, InstallConflict, IntegrityEntry, ActivityEvent, Revision } from "../types";
import type { ToastMsg } from "../components/Toast";

export interface ConfirmOptions {
  title: string;
  message: string;
  okLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}

/** Pick one or many paths from a file/folder dialog, always as an array. */
async function pickPaths(opts: Parameters<typeof open>[0]): Promise<string[]> {
  const picked = await open(opts);
  if (!picked) return [];
  return Array.isArray(picked) ? picked : [picked];
}

/**
 * Owns the package library: data, the detected AE installations, the current
 * selection, and every side-effecting action (import/install/export/…). The
 * UI (search, filters, view mode, modals) lives in the component tree.
 */
export function useLibrary() {
  const [packages, setPackages] = useState<Package[]>([]);
  const [bundles, setBundles] = useState<Bundle[]>([]);
  const [aeInstalls, setAeInstalls] = useState<AeInstallation[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedBundleId, setSelectedBundleId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [toast, setToast] = useState<ToastMsg | null>(null);
  const [confirmState, setConfirmState] = useState<ConfirmOptions | null>(null);
  const confirmResolver = useRef<((ok: boolean) => void) | null>(null);
  const [adminModal, setAdminModal] = useState(false);
  const adminResolver = useRef<((ok: boolean) => void) | null>(null);
  const t = useT();
  const [importPreview, setImportPreview] = useState<PreparedImport | null>(null);
  const previewResolver = useRef<((ok: boolean) => void) | null>(null);
  const [installConflicts, setInstallConflicts] = useState<InstallConflict[] | null>(null);
  const conflictResolver = useRef<((ok: boolean) => void) | null>(null);
  const [integrity, setIntegrity] = useState<IntegrityEntry[] | null>(null);
  const [history, setHistory] = useState<ActivityEvent[] | null>(null);
  const [revisionView, setRevisionView] = useState<{pkg: Package; revisions: Revision[]} | null>(null);

  const resolvePreview = useCallback((ok: boolean) => {
    setImportPreview(null);
    previewResolver.current?.(ok);
    previewResolver.current = null;
  }, []);
  const resolveConflicts = useCallback((ok: boolean) => {
    setInstallConflicts(null);
    conflictResolver.current?.(ok);
    conflictResolver.current = null;
  }, []);

  const withConflicts = useCallback(async <T,>(run: (allow: boolean) => Promise<T>): Promise<T | null> => {
    try { return await run(false); }
    catch (e) {
      const raw = String(e);
      if (!raw.startsWith("PLUXEL_CONFLICT:")) throw e;
      const conflicts = JSON.parse(raw.slice("PLUXEL_CONFLICT:".length)) as InstallConflict[];
      setInstallConflicts(conflicts);
      const allow = await new Promise<boolean>((resolve) => { conflictResolver.current = resolve; });
      if (!allow) return null;
      return run(true);
    }
  }, []);

  const notify = useCallback((message: string, kind: ToastMsg["kind"] = "info") => {
    setToast({ message, kind, id: Math.random() });
  }, []);

  /** Show the in-app confirmation dialog and resolve to the user's choice. */
  const confirm = useCallback(
    (opts: ConfirmOptions) =>
      new Promise<boolean>((resolve) => {
        confirmResolver.current = resolve;
        setConfirmState(opts);
      }),
    [],
  );

  const resolveConfirm = useCallback((ok: boolean) => {
    setConfirmState(null);
    confirmResolver.current?.(ok);
    confirmResolver.current = null;
  }, []);

  /** Open the admin-password modal and resolve once the user unlocks/cancels. */
  const promptAdminUnlock = useCallback(
    () =>
      new Promise<boolean>((resolve) => {
        adminResolver.current = resolve;
        setAdminModal(true);
      }),
    [],
  );

  /** Validate the entered password; it's cached for the rest of the session.
   * Throws on a wrong password so the modal can show an error and stay open. */
  const submitAdminUnlock = useCallback(async (password: string) => {
    await api.adminUnlock(password);
    setAdminModal(false);
    adminResolver.current?.(true);
    adminResolver.current = null;
  }, []);

  const cancelAdminUnlock = useCallback(() => {
    setAdminModal(false);
    adminResolver.current?.(false);
    adminResolver.current = null;
  }, []);

  /** Make sure elevated installs can proceed: silent when already unlocked this
   * session; a password prompt on macOS (cached for the session); a plain
   * confirm on Windows (its UAC prompt follows on the elevated call). */
  const ensureAdminUnlock = useCallback(
    async (opts: { message: string; okLabel: string }): Promise<boolean> => {
      // The backend authenticates once per elevated transaction in helper mode.
      const helper = await api.helperStatus().catch(() => null);
      if (helper?.enabled) return true;
      if (helper?.available && helper.status !== "notRegistered") {
        notify(t("settings.helperApproval"), "info");
        return false;
      }
      const status = await api.adminStatus();
      if (status.unlocked) return true;
      if (!status.supported) {
        return confirm({
          title: t("elevate.title"),
          message: opts.message,
          okLabel: opts.okLabel,
          danger: true,
        });
      }
      return promptAdminUnlock();
    },
    [confirm, promptAdminUnlock, notify, t],
  );

  const refresh = useCallback(async () => {
    try {
      const lib = await api.getLibrary();
      setPackages(lib.packages);
      setBundles(lib.bundles);
    } catch (e) {
      notify(String(e), "error");
    }
  }, [notify]);

  useEffect(() => {
    refresh();
    api.detectAe().then(setAeInstalls).catch(() => void 0);
  }, [refresh]);

  /** Run an async action with the busy flag set and errors surfaced as toasts. */
  const withBusy = useCallback(
    async (fn: () => Promise<void | boolean>) => {
      if (busyRef.current) return false;
      busyRef.current = true;
      setBusy(true);
      try {
        const result = await fn();
        return result !== false;
      } catch (e) {
        await refresh();
        notify(String(e), "error");
        return false;
      } finally {
        try {
          const warnings = await api.takeOperationWarnings();
          if (warnings.length) notify(warnings.map((key) => t(key)).join("\n"), "info");
        } catch { /* Keep the result of the operation if diagnostics are unavailable. */ }
        busyRef.current = false;
        setBusy(false);
      }
    },
    [notify, refresh, t],
  );

  /**
   * Run an action that may fail on a permission error; if so, confirm and retry
   * with elevation. Returns the action's result, or null if cancelled.
   */
  const withElevation = useCallback(
    async (
      run: (elevated: boolean) => Promise<unknown>,
      opts: { message: string; okLabel: string },
    ): Promise<unknown> => {
      try {
        return await run(false);
      } catch (e) {
        const msg = String(e);
        // Permission-denied differs per OS: Unix → EACCES (os error 13) /
        // "Permission denied"; Windows → ERROR_ACCESS_DENIED (os error 5) /
        // "Access is denied". Any of them means "retry elevated".
        const isPermission =
          msg.includes("os error 13") ||
          msg.includes("os error 5") ||
          /permission denied/i.test(msg) ||
          /access is denied/i.test(msg);
        if (!isPermission) throw e;
        const proceed = await ensureAdminUnlock(opts);
        if (!proceed) return null;
        return await run(true);
      }
    },
    [ensureAdminUnlock],
  );

  /**
   * After import, reconcile each newly-imported package against existing ones
   * of the same name/kind: prompt the user to update-and-replace the existing
   * entry (keeping its id/tags/installs) or discard the new import. Returns the
   * id to select afterwards.
   */
  const reconcile = useCallback(
    async (added: Package[]): Promise<{ selectId: string | null; done: number }> => {
      const all = (await api.getLibrary()).packages;
      const existing = all.filter((p) => !added.some((a) => a.id === p.id));
      let selectId: string | null = null;
      let done = 0;
      for (const nw of added) {
        const dup = existing.find(
          (p) =>
            p.kind === nw.kind &&
            p.manifest.name.trim().toLowerCase() === nw.manifest.name.trim().toLowerCase(),
        );
        if (!dup) {
          existing.push(nw);
          selectId = nw.id;
          done++;
          continue;
        }
        const replace = await confirm({
          title: t("dup.title"),
          message: t("dup.msg", { name: nw.manifest.name }),
          okLabel: t("dup.replace"),
          cancelLabel: t("dup.cancel"),
        });
        if (replace) {
          await api.replacePackage(dup.id, nw.id);
          selectId = dup.id;
          done++;
        } else {
          await api.discardImport(nw.id, dup.id);
        }
      }
      return { selectId, done };
    },
    [confirm, t],
  );

  /** Shared import tail: reconcile duplicates, refresh, select, and notify. */
  const finishImport = useCallback(
    async (added: Package[], successKey: string) => {
      const { selectId, done } = await reconcile(added);
      await refresh();
      if (selectId) setSelectedId(selectId);
      notify(
        done ? t(successKey, { count: done }) : t("toast.importCancelled"),
        done ? "success" : "info",
      );
    },
    [reconcile, refresh, notify, t],
  );

  const previewImport = useCallback(async (paths: string[]) => {
    const prepared = await api.prepareImport(paths);
    let committed = false;
    try {
      setImportPreview(prepared);
      const accepted = await new Promise<boolean>((resolve) => { previewResolver.current = resolve; });
      if (!accepted) return;
      const added = await api.commitImport(prepared.token);
      committed = true;
      await finishImport(added, "toast.added");
    } finally {
      setImportPreview(null);
      if (!committed) {
        try { await api.cancelImport(prepared.token); }
        catch { notify(t("maintenance.warningCleanup"), "info"); }
      }
    }
  }, [finishImport, notify, t]);

  const importFiles = useCallback(
    () =>
      withBusy(async () => {
        const paths = await pickPaths({ multiple: true, directory: false, filters: [ASSET_FILTER] });
        if (paths.length) await previewImport(paths);
      }),
    [withBusy, previewImport],
  );

  /** Handle dropped paths: `.ppf`/`.zip` → archive import, everything else → assets. */
  const importDropped = useCallback(
    (paths: string[]) =>
      withBusy(async () => {
        if (paths.length) await previewImport(paths);
      }),
    [withBusy, previewImport],
  );

  const importPackageZip = useCallback(
    () =>
      withBusy(async () => {
        const [arc] = await pickPaths({
          multiple: false,
          directory: false,
          filters: [PPF_IMPORT_FILTER],
        });
        if (!arc) return;
        await previewImport([arc]);
      }),
    [withBusy, previewImport],
  );

  const exportLibraryZip = useCallback(
    () =>
      withBusy(async () => {
        if (!packages.length) {
          notify(t("toast.nothingToExport"), "error");
          return;
        }
        const dest = await save({ defaultPath: `pluxel-export.${PPF_EXT}`, filters: [PPF_FILTER] });
        if (!dest) return;
        const count = await api.exportLibrary([], dest);
        notify(t("toast.exported", { count }), "success");
      }),
    [withBusy, packages.length, notify, t],
  );

  const exportPackage = useCallback(
    (pkg: Package) =>
      withBusy(async () => {
        const safe = sanitizeFilename(pkg.manifest.name, "package");
        const dest = await save({ defaultPath: `${safe}.${PPF_EXT}`, filters: [PPF_FILTER] });
        if (!dest) return;
        await api.exportPackage(pkg.id, dest);
        notify(t("toast.exportedPackage", { name: pkg.manifest.name }), "success");
      }),
    [withBusy, notify, t],
  );

  const detectAe = useCallback(
    () =>
      withBusy(async () => {
        const found = await api.detectAe();
        setAeInstalls(found);
        notify(
          found.length
            ? t("toast.aeDetected", { count: found.length })
            : t("toast.aeNotFound"),
          found.length ? "success" : "error",
        );
      }),
    [withBusy, notify, t],
  );

  const checkIntegrity = useCallback((id?: string) => withBusy(async () => {
    setIntegrity(await api.checkIntegrity(id));
  }), [withBusy]);
  const showHistory = useCallback(() => withBusy(async () => {
    setHistory(await api.getHistory());
  }), [withBusy]);
  const showRevisions = useCallback((pkg: Package) => withBusy(async () => {
    setRevisionView({pkg, revisions: await api.getRevisions(pkg.id)});
  }), [withBusy]);

  const restoreRevision = useCallback((revisionId: string, reinstall: boolean) => withBusy(async () => {
    if (!revisionView) return;
    const pkg = revisionView.pkg;
    await api.restoreRevision(pkg.id, revisionId);
    setRevisionView(null);
    await refresh();
    if (reinstall) {
      try {
        const result = await withConflicts((allow_overwrite) => withElevation(
          (elevated) => api.reinstallPackage(pkg.id, {elevated, allow_overwrite, effect_subdir: effectsSubdir()}),
          {message: t("elevate.msg"), okLabel: t("elevate.ok")},
        ));
        await refresh();
        notify(t(result ? "maintenance.restoredInstalled" : "maintenance.restoredVault"), result ? "success" : "info");
      } catch(e) { throw new Error(t("maintenance.reinstallFailed", {error: String(e)})); }
    } else notify(t("maintenance.restoredVault"), "success");
  }), [revisionView, withBusy, refresh, withConflicts, withElevation, notify, t]);

  const install = useCallback(
    (pkg: Package, params: InstallParams) =>
      withBusy(async () => {
        const updated = (await withConflicts((allow_overwrite) => withElevation(
          (elevated) =>
            api.installPackage(pkg.id, { ...params, elevated, allow_overwrite, effect_subdir: effectsSubdir() }),
          { message: t("elevate.msg"), okLabel: t("elevate.ok") },
        ))) as Package | null;
        if (!updated) return false;
        await refresh();
        const rec = updated.installs[updated.installs.length - 1];
        notify(t("toast.installed", { name: pkg.manifest.name, label: rec?.label ?? "" }), "success");
      }),
    [withBusy, withConflicts, withElevation, refresh, notify, t],
  );

  /** Install one package into every detected AE version at once (one elevation). */
  const installAll = useCallback(
    (pkg: Package) =>
      withBusy(async () => {
        const updated = (await withConflicts((allow_overwrite) => withElevation(
          (elevated) => api.installAllAe(pkg.id, { elevated, allow_overwrite, effect_subdir: effectsSubdir() }),
          { message: t("elevate.msg"), okLabel: t("elevate.ok") },
        ))) as Package | null;
        if (!updated) return false;
        await refresh();
        notify(t("toast.installedAllAe", { name: pkg.manifest.name, count: aeInstalls.length }), "success");
      }),
    [withBusy, withConflicts, withElevation, refresh, notify, t, aeInstalls.length],
  );

  /** Install several selected packages to one AE target in a single batch (one
   * elevation prompt). Already-installed packages at that target are updated. */
  const installSelected = useCallback(
    (ids: string[], installation: AeInstallation) =>
      withBusy(async () => {
        if (!ids.length) return false;
        // Count how many are already installed at this target (→ "updated").
        const updated = ids.filter((id) =>
          packages
            .find((p) => p.id === id)
            ?.installs.some((r) => r.ae?.root === installation.root),
        ).length;
        const res = (await withConflicts((allow_overwrite) => withElevation(
          (elevated) =>
            api.installPackages(ids, { installation, elevated, allow_overwrite, effect_subdir: effectsSubdir() }),
          { message: t("elevate.msg"), okLabel: t("elevate.ok") },
        ))) as Package[] | null;
        if (!res) return false;
        await refresh();
        notify(
          updated > 0
            ? t("toast.installedManyUpd", { count: res.length, updated })
            : t("toast.installedMany", { count: res.length }),
          "success",
        );
      }),
    [withBusy, withConflicts, withElevation, packages, refresh, notify, t],
  );

  const runInstaller = useCallback(
    (pkg: Package) =>
      withBusy(async () => {
        const ok = await confirm({
          title: t("confirm.runTitle"),
          message: t("confirm.runMsg", { name: pkg.manifest.name }),
          okLabel: t("confirm.runOk"),
        });
        if (!ok) return;
        await api.runInstaller(pkg.id);
        await refresh();
        notify(t("toast.installerLaunched"), "success");
      }),
    [withBusy, refresh, notify, confirm, t],
  );

  const uninstall = useCallback(
    (pkg: Package, recordId: string) =>
      withBusy(async () => {
        const res = await withElevation(
          (elevated) => api.uninstallPackage(pkg.id, recordId, elevated),
          { message: t("elevate.uninstallMsg"), okLabel: t("elevate.uninstallOk") },
        );
        if (!res) return;
        await refresh();
        notify(t("toast.uninstalled", { name: pkg.manifest.name }), "success");
      }),
    [withBusy, withElevation, refresh, notify, t],
  );

  const saveManifest = useCallback(
    (id: string, manifest: Manifest) =>
      withBusy(async () => {
        await api.updateManifest(id, manifest);
        await refresh();
        notify(t("toast.saved"), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  const setBanner = useCallback(
    (id: string) =>
      withBusy(async () => {
        const [img] = await pickPaths({ multiple: false, directory: false, filters: [IMAGE_FILTER] });
        if (!img) return;
        await api.setBanner(id, img);
        await refresh();
        notify(t("toast.bannerSet"), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  const setBannerFromClipboard = useCallback(
    (id: string) =>
      withBusy(async () => {
        await api.setBannerClipboard(id);
        await refresh();
        notify(t("toast.bannerSet"), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  const clearBanner = useCallback(
    (id: string) =>
      withBusy(async () => {
        await api.clearBanner(id);
        await refresh();
        notify(t("toast.bannerRemoved"), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  const pickAndAdd = useCallback(
    (id: string, directory: boolean, toastKey: string) =>
      withBusy(async () => {
        const paths = await pickPaths({ multiple: true, directory });
        if (!paths.length) return;
        await api.addFiles(id, paths);
        await refresh();
        notify(t(toastKey, { count: paths.length }), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  const addFiles = useCallback(
    (id: string) => pickAndAdd(id, false, "toast.filesAdded"),
    [pickAndAdd],
  );
  const addFolder = useCallback(
    (id: string) => pickAndAdd(id, true, "toast.folderAdded"),
    [pickAndAdd],
  );

  const replaceFiles = useCallback(
    (id: string) =>
      withBusy(async () => {
        const ok = await confirm({
          title: t("confirm.replaceTitle"),
          message: t("confirm.replaceMsg"),
          okLabel: t("confirm.replaceOk"),
        });
        if (!ok) return;
        const paths = await pickPaths({ multiple: true, directory: false, filters: [ASSET_FILTER] });
        if (!paths.length) return;
        await api.replaceFiles(id, paths);
        await refresh();
        notify(t("toast.filesReplaced"), "success");
      }),
    [withBusy, refresh, notify, confirm, t],
  );

  const removeFile = useCallback(
    (id: string, relPath: string) =>
      withBusy(async () => {
        await api.removeFile(id, relPath);
        await refresh();
        notify(t("toast.fileRemoved"), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  const remove = useCallback(
    (pkg: Package) =>
      withBusy(async () => {
        const ok = await confirm({
          title: t("confirm.deleteTitle"),
          message: t("confirm.deleteMsg", { name: pkg.manifest.name }),
          okLabel: t("confirm.deleteOk"),
          danger: true,
        });
        if (!ok) return;
        await api.removePackage(pkg.id);
        setSelectedId((cur) => (cur === pkg.id ? null : cur));
        await refresh();
        notify(t("toast.removed", { name: pkg.manifest.name }), "success");
      }),
    [withBusy, refresh, notify, confirm, t],
  );

  // ---- Bundles (series) ----

  const createBundle = useCallback(
    (name: string, packageIds: string[] = []) =>
      withBusy(async () => {
        const b = await api.createBundle(name, packageIds);
        await refresh();
        setSelectedBundleId(b.id);
        notify(t("toast.bundleCreated"), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  /** Merge the given packages into an existing bundle's member list. */
  const addToBundle = useCallback(
    (bundleId: string, packageIds: string[]) =>
      withBusy(async () => {
        const b = (await api.getLibrary()).bundles.find((x) => x.id === bundleId);
        if (!b) return false;
        const merged = [...new Set([...b.package_ids, ...packageIds])];
        const added = merged.length - b.package_ids.length;
        await api.updateBundle({ ...b, package_ids: merged });
        await refresh();
        setSelectedBundleId(bundleId);
        notify(t("toast.bundleAdded", { count: added }), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  const setBundleBanner = useCallback(
    (id: string) =>
      withBusy(async () => {
        const [img] = await pickPaths({ multiple: false, directory: false, filters: [IMAGE_FILTER] });
        if (!img) return;
        await api.setBundleBanner(id, img);
        await refresh();
        notify(t("toast.bannerSet"), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  const setBundleBannerFromClipboard = useCallback(
    (id: string) =>
      withBusy(async () => {
        await api.setBundleBannerClipboard(id);
        await refresh();
        notify(t("toast.bannerSet"), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  const clearBundleBanner = useCallback(
    (id: string) =>
      withBusy(async () => {
        await api.clearBundleBanner(id);
        await refresh();
        notify(t("toast.bannerRemoved"), "success");
      }),
    [withBusy, refresh, notify, t],
  );

  const saveBundle = useCallback(
    (bundle: Bundle) =>
      withBusy(async () => {
        await api.updateBundle(bundle);
        await refresh();
      }),
    [withBusy, refresh],
  );

  const removeBundle = useCallback(
    (bundle: Bundle) =>
      withBusy(async () => {
        const ok = await confirm({
          title: t("bundle.deleteTitle"),
          message: t("bundle.deleteMsg", { name: bundle.name }),
          okLabel: t("common.delete"),
          danger: true,
        });
        if (!ok) return;
        await api.deleteBundle(bundle.id);
        setSelectedBundleId((cur) => (cur === bundle.id ? null : cur));
        await refresh();
        notify(t("toast.bundleDeleted"), "success");
      }),
    [withBusy, refresh, notify, confirm, t],
  );

  const installBundle = useCallback(
    (bundle: Bundle, params: InstallParams) =>
      withBusy(async () => {
        const done = await withConflicts((allow_overwrite) => withElevation(
          (elevated) =>
            api.installBundle(bundle.id, { ...params, elevated, allow_overwrite, effect_subdir: effectsSubdir() }),
          { message: t("elevate.msg"), okLabel: t("elevate.ok") },
        ));
        if (!done) return false;
        await refresh();
        notify(t("toast.bundleInstalled", { count: (done as Package[]).length }), "success");
      }),
    [withBusy, withConflicts, withElevation, refresh, notify, t],
  );

  const exportBundle = useCallback(
    (bundle: Bundle) =>
      withBusy(async () => {
        const safe = sanitizeFilename(bundle.name, "bundle");
        const dest = await save({ defaultPath: `${safe}.${PPF_EXT}`, filters: [PPF_FILTER] });
        if (!dest) return;
        await api.exportBundle(bundle.id, dest);
        notify(t("toast.bundleExported", { name: bundle.name }), "success");
      }),
    [withBusy, notify, t],
  );

  const selected = packages.find((p) => p.id === selectedId) ?? null;
  const selectedBundle = bundles.find((b) => b.id === selectedBundleId) ?? null;

  return {
    importPreview, resolvePreview, installConflicts, resolveConflicts,
    integrity, closeIntegrity: () => setIntegrity(null), checkIntegrity,
    history, closeHistory: () => setHistory(null), showHistory,
    revisionView, closeRevisions: () => setRevisionView(null), showRevisions, restoreRevision,
    packages,
    bundles,
    aeInstalls,
    selected,
    selectedId,
    select: setSelectedId,
    selectedBundle,
    selectedBundleId,
    selectBundle: setSelectedBundleId,
    createBundle,
    addToBundle,
    saveBundle,
    removeBundle,
    installBundle,
    exportBundle,
    setBundleBanner,
    setBundleBannerFromClipboard,
    clearBundleBanner,
    busy,
    toast,
    dismissToast: () => setToast(null),
    confirmState,
    resolveConfirm,
    adminModal,
    submitAdminUnlock,
    cancelAdminUnlock,
    importDropped,
    importFiles,
    importPackageZip,
    exportLibraryZip,
    exportPackage,
    detectAe,
    runInstaller,
    install,
    installAll,
    installSelected,
    uninstall,
    saveManifest,
    setBanner,
    setBannerFromClipboard,
    clearBanner,
    addFiles,
    addFolder,
    replaceFiles,
    removeFile,
    remove,
  };
}
