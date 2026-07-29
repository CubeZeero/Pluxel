import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { api } from "../lib/api";
import { Modal } from "./Modal";
import {
  ACCENT_PRESETS,
  useAccent,
  useCustomInstallDirs,
  useEffectsFolder,
  useKindColors,
  useTheme,
  type Theme,
} from "../lib/theme";
import { LANGS, useI18n } from "../lib/i18n";
import { KIND_I18N, KIND_ORDER, type AeInstallation } from "../types";

const THEMES: { value: Theme; key: string; icon: string }[] = [
  { value: "light", key: "settings.themeLight", icon: "bi-sun" },
  { value: "dark", key: "settings.themeDark", icon: "bi-moon-stars" },
];

interface Props {
  busy: boolean;
  packageCount: number;
  aeInstalls: AeInstallation[];
  onExportZip: () => void;
  onDetectAe: () => void;
  onClose: () => void;
}

export function SettingsModal({
  busy,
  packageCount,
  aeInstalls,
  onExportZip,
  onDetectAe,
  onClose,
}: Props) {
  const [dataDir, setDataDir] = useState("");
  const [theme, setTheme] = useTheme();
  const [accent, setAccent] = useAccent();
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

  const refreshAdmin = () => {
    api.adminStatus().then((s) => {
      setAdminSupported(s.supported);
      setAdminUnlocked(s.unlocked);
    });
  };

  useEffect(() => {
    api.getDataDir().then(setDataDir).catch(() => void 0);
    refreshAdmin();
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
        <h4>{t("settings.theme")}</h4>
        <div className="seg-toggle">
          {THEMES.map((o) => (
            <button
              key={o.value}
              className={`seg-btn${theme === o.value ? " active" : ""}`}
              onClick={() => setTheme(o.value)}
            >
              <i className={`bi ${o.icon}`} /> {t(o.key)}
            </button>
          ))}
        </div>
      </section>

      <section className="settings-section">
        <h4>{t("settings.accent")}</h4>
        <div className="accent-picker">
          {ACCENT_PRESETS.map((c) => (
            <button
              key={c}
              className={`accent-swatch${accent.toLowerCase() === c.toLowerCase() ? " active" : ""}`}
              style={{ background: c }}
              onClick={() => setAccent(c)}
              aria-label={c}
            />
          ))}
          <label className="accent-custom">
            <i className="bi bi-eyedropper" />
            <input type="color" value={accent} onChange={(e) => setAccent(e.target.value)} />
          </label>
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
            <label key={kind} className="kind-color-row">
              <span className="kind-swatch" style={{ background: kindColors[kind] }}>
                <input
                  type="color"
                  value={kindColors[kind]}
                  onChange={(e) => setKindColor(kind, e.target.value)}
                />
              </span>
              <span className="kind-color-name">{t(KIND_I18N[kind])}</span>
            </label>
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

      {adminSupported && (
        <section className="settings-section">
          <h4>{t("settings.adminAuth")}</h4>
          <p className="settings-hint">{t("settings.adminAuthHint")}</p>
          {adminUnlocked && (
            <button className="btn" onClick={() => api.adminLock().then(refreshAdmin)}>
              <i className="bi bi-lock" /> {t("settings.adminLock")}
            </button>
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
        <h4>{t("settings.backup")}</h4>
        <p className="settings-hint">{t("settings.backupHint", { count: packageCount })}</p>
        <button className="btn" onClick={onExportZip} disabled={busy || packageCount === 0}>
          <i className="bi bi-box-arrow-up" /> {t("settings.exportAll")}
        </button>
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
