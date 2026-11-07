import { invoke } from "@tauri-apps/api/core";
import type {
  AeInstallation,
  Bundle,
  Library,
  Manifest,
  Package,
  PackageKind,
  PreparedImport, ActivityEvent, Revision, IntegrityEntry,
} from "../types";

export interface HelperStatus {
  supported: boolean;
  available: boolean;
  enabled: boolean;
  unlocked: boolean;
  status: string;
}

export interface InstallParams {
  allow_overwrite?: boolean;
  installation?: AeInstallation | null;
  kind?: PackageKind | null;
  custom_dir?: string | null;
  label?: string | null;
  elevated?: boolean | null;
  effect_subdir?: string | null;
}

export const api = {
  takeOperationWarnings: () => invoke<string[]>("take_operation_warnings"),
  prepareImport: (paths: string[]) => invoke<PreparedImport>("prepare_import", { paths }),
  commitImport: (token: string) => invoke<Package[]>("commit_import", { token }),
  discardImport: (id: string, replacementId: string) => invoke<void>("discard_import", { id, replacementId }),
  cancelImport: (token: string) => invoke<void>("cancel_import", { token }),
  checkIntegrity: (id?: string) => invoke<IntegrityEntry[]>("check_integrity", { id: id ?? null }),
  getHistory: () => invoke<ActivityEvent[]>("get_history"),
  getRevisions: (id: string) => invoke<Revision[]>("get_revisions", { id }),
  restoreRevision: (id: string, revisionId: string) => invoke<Package>("restore_revision", { id, revisionId }),
  reinstallPackage: (id: string, params: InstallParams) => invoke<Package>("reinstall_package", { id, params }),
  getLibrary: () => invoke<Library>("get_library"),

  getDataDir: () => invoke<string>("get_data_dir"),

  importPaths: (paths: string[]) => invoke<Package[]>("import_paths", { paths }),

  addFiles: (id: string, paths: string[]) =>
    invoke<Package>("add_files", { id, paths }),

  replaceFiles: (id: string, paths: string[]) =>
    invoke<Package>("replace_files", { id, paths }),

  removeFile: (id: string, relPath: string) =>
    invoke<Package>("remove_file", { id, relPath }),

  removePackage: (id: string) => invoke<void>("remove_package", { id }),

  replacePackage: (targetId: string, sourceId: string) =>
    invoke<Package>("replace_package", { targetId, sourceId }),

  updateManifest: (id: string, manifest: Manifest) =>
    invoke<Package>("update_manifest", { id, manifest }),

  setBanner: (id: string, sourcePath: string) =>
    invoke<Package>("set_banner", { id, sourcePath }),

  setBannerClipboard: (id: string) =>
    invoke<Package>("set_banner_clipboard", { id }),

  clearBanner: (id: string) => invoke<Package>("clear_banner", { id }),

  readBanner: (id: string) => invoke<string | null>("read_banner", { id }),

  detectAe: () => invoke<AeInstallation[]>("detect_ae"),

  /** Reveal a file/folder in Finder (macOS) or Explorer (Windows). */
  revealPath: (path: string) => invoke<void>("reveal_path", { path }),

  helperStatus: () => invoke<HelperStatus>("helper_status"),
  helperRegister: () => invoke<HelperStatus>("helper_register"),
  helperUnregister: () => invoke<HelperStatus>("helper_unregister"),
  helperAuthenticate: () => invoke<void>("helper_authenticate"),

  adminStatus: () => invoke<{ supported: boolean; unlocked: boolean }>("admin_status"),
  adminUnlock: (password: string) => invoke<void>("admin_unlock", { password }),
  adminLock: () => invoke<void>("admin_lock"),

  runInstaller: (id: string) => invoke<Package>("run_installer", { id }),

  installPackage: (id: string, params: InstallParams) =>
    invoke<Package>("install_package", { id, params }),

  installAllAe: (id: string, params: InstallParams) =>
    invoke<Package>("install_all_ae", { id, params }),

  installPackages: (ids: string[], params: InstallParams) =>
    invoke<Package[]>("install_packages", { ids, params }),

  uninstallPackage: (id: string, recordId: string, elevated?: boolean) =>
    invoke<Package>("uninstall_package", { id, recordId, elevated }),

  exportLibrary: (packageIds: string[], dest: string) =>
    invoke<number>("export_library", { packageIds, dest }),

  exportPackage: (id: string, dest: string) =>
    invoke<void>("export_package", { id, dest }),

  importArchive: (zipPath: string) =>
    invoke<Package[]>("import_archive", { zipPath }),

  createBundle: (name: string, packageIds: string[]) =>
    invoke<Bundle>("create_bundle", { name, packageIds }),

  updateBundle: (bundle: Bundle) => invoke<Bundle>("update_bundle", { bundle }),

  deleteBundle: (id: string) => invoke<void>("delete_bundle", { id }),

  setBundleBanner: (id: string, sourcePath: string) =>
    invoke<Bundle>("set_bundle_banner", { id, sourcePath }),

  setBundleBannerClipboard: (id: string) =>
    invoke<Bundle>("set_bundle_banner_clipboard", { id }),

  clearBundleBanner: (id: string) => invoke<Bundle>("clear_bundle_banner", { id }),

  readBundleBanner: (id: string) =>
    invoke<string | null>("read_bundle_banner", { id }),

  installBundle: (id: string, params: InstallParams) =>
    invoke<Package[]>("install_bundle", { id, params }),

  exportBundle: (id: string, dest: string) =>
    invoke<void>("export_bundle", { id, dest }),
};
