import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { api, type HelperStatus } from "../lib/api";
import { Modal } from "./Modal";
import { ColorPicker } from "./ColorPicker";
import {
  KIND_COLOR_PRESETS,
  useCustomInstallDirs,
  useEffectsFolder,
  useKindColors,
  useTheme,
} from "../lib/theme";
import { LANGS, useI18n } from "../lib/i18n";
import { KIND_I18N, KIND_ORDER, type AeInstallation } from "../types";


interface Props {
  onCheckIntegrity: () => void;
  onShowHistory: () => void;
  busy: boolean;
  packageCount: number;
  aeInstalls: AeInstallation[];
  onExportZip: () => void;
  onDetectAe: () => void;
  autoUpdate: boolean;
  onToggleAutoUpdate: (on: boolean) => void;
  onCheckUpdate: () => void;
  checkingUpdate: boolean;
  onClose: () => void;
}

export function SettingsModal({
  onCheckIntegrity,
  onShowHistory,
  busy,
  packageCount,
  aeInstalls,
  onExportZip,
  onDetectAe,
  autoUpdate,
  onToggleAutoUpdate,
  onCheckUpdate,
  checkingUpdate,
  onClose,
}: Props) {
  const [dataDir, setDataDir] = useState("");
  const [theme, setTheme] = useTheme();
  const { colors: kindColors, setColor: setKindColor, reset: resetKindColors } = useKindColors();
  const [effectsFolder, setEffectsFolder] = useEffectsFolder();
  const [customDirs, setCustomDirs] = useCustomInstallDirs();
  const { lang, setLang, t } = useI18n();

  const addCustomInstall = async () => {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === "string" && !customDirs.includes(dir)) {
      setCustomDirs([...customDirs, dir]);
    }
  };
  const removeCustomInstall = (dir: string) =>
    setCustomDirs(customDirs.filter((d) => d !== dir));

  // Admin auth session state (macOS only — Windows uses per-op UAC).
  const [adminSupported, setAdminSupported] = useState(false);
  const [adminUnlocked, setAdminUnlocked] = useState(false);

  const [helper, setHelper] = useState<HelperStatus | null>(null);
  const [helperBusy, setHelperBusy] = useState(false);
  const [adminError, setAdminError] = useState("");

  const refreshAdmin = async () => {
    const [adminResult, helperResult] = await Promise.allSettled([
      api.adminStatus(), api.helperStatus(),
    ]);
    if (adminResult.status === "fulfilled") {
      setAdminSupported(adminResult.value.supported);
      setAdminUnlocked(adminResult.value.unlocked);
    }
    if (helperResult.status === "fulfilled") setHelper(helperResult.value);
    const errors = [adminResult, helperResult]
      .filter((result) => result.status === "rejected")
      .map((result) => String(result.reason));
    if (errors.length) throw new Error(errors.join("\n"));
    setAdminError("");
  };

  const runAdminAction = async (action?: () => Promise<unknown>) => {
    setHelperBusy(true);
    setAdminError("");
    try {
      try {
        await action?.();
      } finally {
        await refreshAdmin();
      }
    } catch (error) {
      setAdminError(String(error));
    } finally {
      setHelperBusy(false);
    }
  };

  useEffect(() => {
    api.getDataDir().then(setDataDir).catch(() => void 0);
    const refresh = () => { void refreshAdmin().catch((error) => setAdminError(String(error))); };
    refresh();
    // Registration may need approval outside the app in System Settings.
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, []);

  return (
    <Modal title={t("settings.title")} icon="bi-gear" onClose={onClose} width={520}>
      <section className="settings-section">
        <h4>{t("settings.language")}</h4>
        <div className="seg-toggle">
          {LANGS.map((l) => (
            <button
              key={l.value}
              className={`seg-btn${lang === l.value ? " active" : ""}`}
              onClick={() => setLang(l.value)}
            >
              {l.label}
            </button>
          ))}
        </div>
      </section>

      <section className="settings-section">
        <h4>{t("settings.appearance")}</h4>
        <div className="seg-toggle">
          {(["light", "dark"] as const).map((option) => (
            <button key={option} className={`seg-btn${theme === option ? " active" : ""}`}
              aria-pressed={theme === option} onClick={() => setTheme(option)}>
              <i className={`bi bi-${option === "light" ? "sun" : "moon"}`} /> {t(`settings.${option}`)}
            </button>
          ))}
        </div>
      </section>
      <section className="settings-section">
        <div className="settings-head">
          <h4>{t("settings.kindColors")}</h4>
          <button className="btn ghost xs" onClick={resetKindColors}>
            <i className="bi bi-arrow-counterclockwise" /> {t("settings.reset")}
          </button>
        </div>
        <div className="kind-colors">
          {KIND_ORDER.map((kind) => (
            <ColorPicker key={kind} label={t(KIND_I18N[kind])} value={kindColors[kind]} onChange={(c) => setKindColor(kind, c)} presets={KIND_COLOR_PRESETS} />
          ))}
        </div>
      </section>

      <section className="settings-section">
        <h4>{t("settings.effectsFolder")}</h4>
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={effectsFolder}
            onChange={(e) => setEffectsFolder(e.target.checked)}
          />
          <span>{t("settings.effectsFolderToggle")}</span>
        </label>
        <p className="settings-hint">{t("settings.effectsFolderHint")}</p>
      </section>

      <section className="settings-section">
        <div className="settings-head">
          <h4>{t("settings.customInstall")}</h4>
          <button className="btn ghost xs" onClick={addCustomInstall}>
            <i className="bi bi-folder-plus" /> {t("settings.customInstallChoose")}
          </button>
        </div>
        {customDirs.length > 0 ? (
          <ul className="custom-dir-list">
            {customDirs.map((dir) => (
              <li key={dir} className="path-box">
                <code>{dir}</code>
                <button
                  className="icon-btn danger-icon"
                  title={t("settings.customInstallClear")}
                  onClick={() => removeCustomInstall(dir)}
                >
                  <i className="bi bi-x-lg" />
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="settings-hint">{t("settings.customInstallNone")}</p>
        )}
        <p className="settings-hint">{t("settings.customInstallHint")}</p>
      </section>

      {(adminSupported || helper?.supported) && (
        <section className="settings-section">
          <h4>{t("settings.adminAuth")}</h4>
          <p className="settings-hint">{t("settings.helperHint")}</p>
          <p className="settings-hint">{t("settings.helperRequirements")}</p>
          <p className="settings-hint">{t("settings.helperScope")}</p>
          <p role="status" aria-live="polite">
            {t("settings.helperStatus")}: {helper
              ? helper.enabled
                ? t("settings.helperEnabled")
                : !helper.available
                  ? t("settings.helperUnavailable")
                  : t(`settings.helperState.${["notRegistered", "requiresApproval", "notFound"].includes(helper.status) ? helper.status : "unknown"}`)
              : t("settings.helperUnknown")}
          </p>
          <p className="settings-hint">{t("settings.helperApproval")}</p>
          <div className="maintenance-actions">
            <button className="btn" disabled={busy || helperBusy || !helper?.available || helper.enabled}
              onClick={() => void runAdminAction(api.helperRegister)}>
              {t("settings.helperSetup")}
            </button>
            <button className="btn" disabled={busy || helperBusy || !helper?.available}
              onClick={() => void runAdminAction(api.helperUnregister)}>
              {t("settings.helperRemove")}
            </button>
            <button className="btn" disabled={helperBusy} onClick={() => void runAdminAction()}>
              <i className="bi bi-arrow-repeat" /> {t("settings.helperRefresh")}
            </button>
          </div>
          {helper?.enabled && (
            <>
              <p role="status" aria-live="polite">
                {t(helper.unlocked ? "settings.helperSessionUnlocked" : "settings.helperSessionLocked")}
              </p>
              {helper.unlocked && (
                <button className="btn" disabled={busy || helperBusy}
                  onClick={() => void runAdminAction(api.adminLock)}>
                  <i className="bi bi-lock" /> {t("settings.adminLock")}
                </button>
              )}
            </>
          )}
          {adminError && <p role="alert">{t("settings.helperError", { error: adminError })}</p>}
          {!helper?.enabled && adminSupported && (
            <>
              <p className="settings-hint">{t("settings.adminAuthHint")}</p>
              {adminUnlocked && (
                <button className="btn" disabled={busy || helperBusy}
                  onClick={() => void runAdminAction(api.adminLock)}>
                  <i className="bi bi-lock" /> {t("settings.adminLock")}
                </button>
              )}
            </>
          )}
        </section>
      )}

      <section className="settings-section">
        <h4>{t("settings.aeDetect")}</h4>
        {aeInstalls.length > 0 ? (
          <ul className="ae-list">
            {aeInstalls.map((ae) => (
              <li key={ae.root}>
                <i className="bi bi-check-circle-fill" />
                <span>{ae.name}</span>
                <code>{ae.root}</code>
              </li>
            ))}
          </ul>
        ) : (
          <p className="settings-hint">{t("settings.aeNone")}</p>
        )}
        <button className="btn" onClick={onDetectAe} disabled={busy}>
          <i className="bi bi-arrow-repeat" /> {t("settings.redetect")}
        </button>
      </section>

      <section className="settings-section">
        <h4>{t("settings.updates")}</h4>
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={autoUpdate}
            onChange={(e) => onToggleAutoUpdate(e.target.checked)}
          />
          <span>{t("settings.autoUpdate")}</span>
        </label>
        <button className="btn" onClick={onCheckUpdate} disabled={checkingUpdate}>
          <i className="bi bi-arrow-repeat" /> {t("settings.checkUpdate")}
        </button>
      </section>

      <section className="settings-section">
        <h4>{t("settings.backup")}</h4>
        <p className="settings-hint">{t("settings.backupHint", { count: packageCount })}</p>
        <button className="btn" onClick={onExportZip} disabled={busy || packageCount === 0}>
          <i className="bi bi-box-arrow-up" /> {t("settings.exportAll")}
        </button>
      </section>

      <section className="settings-section">
        <h4>{t("maintenance.tools")}</h4>
        <div className="maintenance-actions">
          <button className="btn" disabled={busy} onClick={onCheckIntegrity}><i className="bi bi-shield-check" /> {t("maintenance.check")}</button>
          <button className="btn" disabled={busy} onClick={onShowHistory}><i className="bi bi-clock-history" /> {t("maintenance.historyTitle")}</button>
        </div>
      </section>

      <section className="settings-section">
        <h4>{t("settings.dataDir")}</h4>
        <div className="path-box">
          <code>{dataDir || "…"}</code>
          <button
            className="icon-btn"
            title={t("settings.openFolder")}
            onClick={() => dataDir && openPath(dataDir)}
          >
            <i className="bi bi-folder2-open" />
          </button>
        </div>
      </section>
    </Modal>
  );
}
