import { useCallback, useEffect, useMemo, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { listen } from "@tauri-apps/api/event";
import { useLibrary } from "./lib/useLibrary";
import { useT } from "./lib/i18n";
import type { AeInstallation, Package, PackageKind } from "./types";
import { Toolbar } from "./components/Toolbar";
import { PackageGrid } from "./components/PackageGrid";
import { DetailPanel } from "./components/DetailPanel";
import { Toast } from "./components/Toast";
import { SettingsModal } from "./components/SettingsModal";
import { AboutModal } from "./components/AboutModal";
import { DropOverlay } from "./components/DropOverlay";
import { ContextMenu } from "./components/ContextMenu";
import { FilterBar } from "./components/FilterBar";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { AdminUnlockModal } from "./components/AdminUnlockModal";
import { InstallTargetModal } from "./components/InstallTargetModal";
import { BundleList } from "./components/BundleList";
import { BundleDetail } from "./components/BundleDetail";
import { UpdateModal } from "./components/UpdateModal";
import { useUpdater } from "./lib/useUpdater";
import "./App.css";

export default function App() {
  const lib = useLibrary();
  const t = useT();
  const updater = useUpdater();

  // View-only UI state.
  const [query, setQuery] = useState("");
  const [activeKinds, setActiveKinds] = useState<Set<PackageKind>>(new Set());
  const [viewMode, setViewMode] = useState<"grid" | "list">(
    () => (localStorage.getItem("pluxel.view") as "grid" | "list") || "grid",
  );
  const [mode, setMode] = useState<"packages" | "bundles">("packages");
  const [showBundled, setShowBundled] = useState(
    () => localStorage.getItem("pluxel.showBundled") === "1",
  );
  const [selectMode, setSelectMode] = useState(false);
  const [bundleSel, setBundleSel] = useState<Set<string>>(new Set());
  const [addTarget, setAddTarget] = useState<string>("");
  const [installTargetOpen, setInstallTargetOpen] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showAbout, setShowAbout] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; pkg: Package } | null>(null);

  useEffect(() => localStorage.setItem("pluxel.view", viewMode), [viewMode]);
  useEffect(() => localStorage.setItem("pluxel.showBundled", showBundled ? "1" : "0"), [showBundled]);

  const toggleKind = useCallback((k: PackageKind) => {
    setActiveKinds((prev) => {
      const next = new Set(prev);
      next.has(k) ? next.delete(k) : next.add(k);
      return next;
    });
  }, []);

  // Suppress the WebView's native right-click menu (Reload, etc.), except in
  // text fields where copy/paste is useful.
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if ((e.target as HTMLElement).closest("input, textarea")) return;
      e.preventDefault();
    };
    document.addEventListener("contextmenu", handler);
    return () => document.removeEventListener("contextmenu", handler);
  }, []);

  // Native drag & drop of files onto the window.
  const { importDropped } = lib;
  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === "enter" || p.type === "over") setDragging(true);
      else if (p.type === "leave") setDragging(false);
      else if (p.type === "drop") {
        setDragging(false);
        if (p.paths?.length) importDropped(p.paths);
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [importDropped]);

  // Native menu (File / Edit) actions.
  const { importPackageZip, importFiles } = lib;
  useEffect(() => {
    const unlisten = listen<string>("menu-action", (e) => {
      switch (e.payload) {
        case "import_package":
          importPackageZip();
          break;
        case "import_assets":
          importFiles();
          break;
        case "toggle_select":
          setMode("packages");
          setSelectMode((s) => !s);
          setBundleSel(new Set());
          break;
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [importPackageZip, importFiles]);

  // Packages that belong to a bundle — hidden from the flat list by default so
  // large series don't clutter it (they're managed via their bundle).
  const bundledIds = useMemo(
    () => new Set(lib.bundles.flatMap((b) => b.package_ids)),
    [lib.bundles],
  );
  // Base list for counts/filtering respects the "show bundled" toggle.
  const visible = useMemo(
    () => (showBundled ? lib.packages : lib.packages.filter((p) => !bundledIds.has(p.id))),
    [lib.packages, bundledIds, showBundled],
  );

  const kindCounts = useMemo(() => {
    const counts: Partial<Record<PackageKind, number>> = {};
    for (const p of visible) counts[p.kind] = (counts[p.kind] ?? 0) + 1;
    return counts;
  }, [visible]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return visible.filter((p) => {
      if (activeKinds.size && !activeKinds.has(p.kind)) return false;
      if (!q) return true;
      const m = p.manifest;
      return (
        m.name.toLowerCase().includes(q) ||
        m.author.toLowerCase().includes(q) ||
        m.description.toLowerCase().includes(q) ||
        m.tags.some((t) => t.toLowerCase().includes(q))
      );
    });
  }, [visible, query, activeKinds]);

  // Install the checked packages into one AE target, then exit select mode.
  const runInstallSelected = (ae: AeInstallation) => {
    lib.installSelected([...bundleSel], ae);
    setInstallTargetOpen(false);
    setSelectMode(false);
    setBundleSel(new Set());
  };

  return (
    <div className="app">
      <Toolbar
        query={query}
        onQuery={setQuery}
        busy={lib.busy}
        onImportPackage={lib.importPackageZip}
        onImportFiles={lib.importFiles}
        onOpenSettings={() => setShowSettings(true)}
        onOpenAbout={() => setShowAbout(true)}
        viewMode={viewMode}
        onViewMode={setViewMode}
        mode={mode}
        onMode={(m) => {
          setMode(m);
          setSelectMode(false);
          setBundleSel(new Set());
        }}
        selectMode={selectMode}
        onToggleSelect={() => {
          setSelectMode((s) => !s);
          setBundleSel(new Set());
        }}
      />

      {mode === "packages" ? (
        <div className="body">
          <div className="library-pane">
            <FilterBar
              counts={kindCounts}
              active={activeKinds}
              total={visible.length}
              onToggle={toggleKind}
              onClear={() => setActiveKinds(new Set())}
              bundledCount={bundledIds.size}
              showBundled={showBundled}
              onToggleBundled={() => setShowBundled((s) => !s)}
            />
            <PackageGrid
              packages={filtered}
              selectedId={lib.selectedId}
              onSelect={lib.select}
              onContext={(pkg, x, y) => {
                lib.select(pkg.id);
                setMenu({ x, y, pkg });
              }}
              totalCount={lib.packages.length}
              viewMode={viewMode}
              selectMode={selectMode}
              checkedIds={bundleSel}
              onCheck={(id) =>
                setBundleSel((prev) => {
                  const next = new Set(prev);
                  next.has(id) ? next.delete(id) : next.add(id);
                  return next;
                })
              }
            />
            {selectMode && (
              <div className="library-footer select-footer">
                <span className="select-count">
                  {t("select.count", { count: bundleSel.size })}
                </span>
                <div className="select-actions">
                  <button className="btn ghost" onClick={() => setBundleSel(new Set())}>
                    {t("select.clear")}
                  </button>
                  <button
                    className="btn"
                    disabled={bundleSel.size === 0 || lib.busy || lib.aeInstalls.length === 0}
                    title={lib.aeInstalls.length === 0 ? t("settings.aeNone") : undefined}
                    onClick={() => {
                      if (lib.aeInstalls.length === 1) runInstallSelected(lib.aeInstalls[0]);
                      else setInstallTargetOpen(true);
                    }}
                  >
                    <i className="bi bi-download" /> {t("select.install")}
                  </button>
                  {lib.bundles.length > 0 && (
                    <div className="select-add-existing">
                      <select
                        value={addTarget}
                        onChange={(e) => setAddTarget(e.target.value)}
                      >
                        <option value="">{t("select.chooseBundle")}</option>
                        {lib.bundles.map((b) => (
                          <option key={b.id} value={b.id}>
                            {b.name}
                          </option>
                        ))}
                      </select>
                      <button
                        className="btn"
                        disabled={!addTarget || bundleSel.size === 0 || lib.busy}
                        onClick={() => {
                          lib.addToBundle(addTarget, [...bundleSel]);
                          setMode("bundles");
                          setSelectMode(false);
                          setBundleSel(new Set());
                          setAddTarget("");
                        }}
                      >
                        <i className="bi bi-plus-lg" /> {t("select.addToBundle")}
                      </button>
                    </div>
                  )}
                  <button
                    className="btn primary-btn"
                    disabled={bundleSel.size === 0 || lib.busy}
                    onClick={() => {
                      lib.createBundle(t("bundle.untitled"), [...bundleSel]);
                      setMode("bundles");
                      setSelectMode(false);
                      setBundleSel(new Set());
                    }}
                  >
                    <i className="bi bi-collection" /> {t("select.createBundle")}
                  </button>
                </div>
              </div>
            )}
          </div>

          <DetailPanel
            key={lib.selected?.id ?? "none"}
            pkg={lib.selected}
            aeInstalls={lib.aeInstalls}
            busy={lib.busy}
            onInstall={lib.install}
            onUninstall={lib.uninstall}
            onSaveManifest={lib.saveManifest}
            onSetBanner={lib.setBanner}
            onClearBanner={lib.clearBanner}
            onAddFiles={lib.addFiles}
            onAddFolder={lib.addFolder}
            onReplaceFiles={lib.replaceFiles}
            onRemoveFile={lib.removeFile}
            onExportPackage={lib.exportPackage}
            onRunInstaller={lib.runInstaller}
            onDelete={lib.remove}
          />
        </div>
      ) : (
        <div className="body">
          <BundleList
            bundles={lib.bundles}
            selectedId={lib.selectedBundleId}
            busy={lib.busy}
            viewMode={viewMode}
            onSelect={lib.selectBundle}
            onCreate={() => lib.createBundle(t("bundle.untitled"))}
          />
          <BundleDetail
            key={lib.selectedBundle?.id ?? "none"}
            bundle={lib.selectedBundle}
            packages={lib.packages}
            aeInstalls={lib.aeInstalls}
            busy={lib.busy}
            onSave={lib.saveBundle}
            onInstall={lib.installBundle}
            onExport={lib.exportBundle}
            onDelete={lib.removeBundle}
            onSetBanner={lib.setBundleBanner}
            onClearBanner={lib.clearBundleBanner}
          />
        </div>
      )}

      {showSettings && (
        <SettingsModal
          busy={lib.busy}
          packageCount={lib.packages.length}
          aeInstalls={lib.aeInstalls}
          onExportZip={lib.exportLibraryZip}
          onDetectAe={lib.detectAe}
          autoUpdate={updater.enabled}
          onToggleAutoUpdate={updater.setEnabled}
          onCheckUpdate={updater.checkNow}
          checkingUpdate={updater.phase === "checking"}
          onClose={() => setShowSettings(false)}
        />
      )}
      {showAbout && <AboutModal onClose={() => setShowAbout(false)} />}

      <UpdateModal
        phase={updater.phase}
        update={updater.update}
        progress={updater.progress}
        error={updater.error}
        onInstall={updater.install}
        onDismiss={updater.dismiss}
      />

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={[
            {
              label: t("menu.export"),
              icon: "bi-box-arrow-up",
              onClick: () => lib.exportPackage(menu.pkg),
            },
            {
              label: t("menu.delete"),
              icon: "bi-trash",
              danger: true,
              onClick: () => lib.remove(menu.pkg),
            },
          ]}
        />
      )}

      {lib.confirmState && (
        <ConfirmDialog {...lib.confirmState} onResult={lib.resolveConfirm} />
      )}

      {lib.adminModal && (
        <AdminUnlockModal onSubmit={lib.submitAdminUnlock} onCancel={lib.cancelAdminUnlock} />
      )}

      {installTargetOpen && (
        <InstallTargetModal
          installs={lib.aeInstalls}
          count={bundleSel.size}
          onPick={runInstallSelected}
          onCancel={() => setInstallTargetOpen(false)}
        />
      )}

      <DropOverlay visible={dragging} />
      {lib.toast && <Toast toast={lib.toast} onDone={lib.dismissToast} />}
    </div>
  );
}
