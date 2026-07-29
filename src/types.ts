// Mirrors the serde-serialized Rust models in `src-tauri/src/models.rs`.

export type PackageKind =
  | "script"
  | "script-ui-panel"
  | "plugin"
  | "zxp"
  | "installer"
  | "unknown";

export interface Manifest {
  name: string;
  version: string;
  author: string;
  description: string;
  homepage: string;
  tags: string[];
  install_as?: PackageKind | null;
}

export interface PackageFile {
  rel_path: string;
  kind: PackageKind;
  size: number;
}

export interface InstallRecord {
  id: string;
  label: string;
  kind: PackageKind;
  target_dir: string;
  ae?: AeInstallation | null;
  paths: string[];
  installed_at: string;
}

export interface Package {
  id: string;
  kind: PackageKind;
  manifest: Manifest;
  banner?: string | null;
  files: PackageFile[];
  installs: InstallRecord[];
  added_at: string;
  updated_at: string;
}

export interface Bundle {
  id: string;
  name: string;
  description: string;
  author: string;
  homepage: string;
  banner?: string | null;
  package_ids: string[];
  created_at: string;
  updated_at: string;
}

export interface Library {
  packages: Package[];
  bundles: Bundle[];
}

/** Kinds eligible for bundling / batch install: everything that is copy-installed
 * into AE (scripts, ScriptUI panels, effect plugins, .zxp). Native installers
 * (.pkg/.exe/.msi) are run individually, not bundled or batch-installed. */
export const BUNDLEABLE_KINDS: PackageKind[] = ["plugin", "script", "script-ui-panel", "zxp"];
export const isBundleable = (k: PackageKind) => BUNDLEABLE_KINDS.includes(k);

export interface AeInstallation {
  name: string;
  version: string;
  root: string;
  scripts_dir: string;
  scriptui_dir: string;
  plugins_dir: string;
  cep_extensions_dir: string;
}

export const KIND_LABELS: Record<PackageKind, string> = {
  script: "Script (.jsx)",
  "script-ui-panel": "ScriptUI Panel",
  plugin: "Effect (.plugin/.aex)",
  zxp: "Plugin (.zxp)",
  installer: "Installer (.pkg/.exe)",
  unknown: "Other",
};

export const KIND_TARGETS: Record<PackageKind, string> = {
  script: "Scripts",
  "script-ui-panel": "Scripts › ScriptUI Panels",
  plugin: "Plug-ins",
  zxp: "CEP Extensions",
  installer: "System installer",
  unknown: "Scripts",
};

/** Canonical display order for kinds across filters, settings and detail views. */
export const KIND_ORDER: PackageKind[] = [
  "script",
  "script-ui-panel",
  "plugin",
  "zxp",
  "installer",
  "unknown",
];

/** i18n key for each kind's localized short label. */
export const KIND_I18N: Record<PackageKind, string> = {
  script: "kind.script",
  "script-ui-panel": "kind.scriptui",
  plugin: "kind.plugin",
  zxp: "kind.zxp",
  installer: "kind.installer",
  unknown: "kind.unknown",
};

