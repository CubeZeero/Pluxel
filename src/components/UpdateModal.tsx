import type { Update } from "@tauri-apps/plugin-updater";
import { Modal } from "./Modal";
import { useT } from "../lib/i18n";
import type { UpdatePhase } from "../lib/useUpdater";

interface Props {
  phase: UpdatePhase;
  update: Update | null;
  progress: number; // 0..1
  error: string | null;
  onInstall: () => void;
  onDismiss: () => void;
}

/** Shown when an update is available, downloading, up-to-date, or on error. */
export function UpdateModal({ phase, update, progress, error, onInstall, onDismiss }: Props) {
  const t = useT();
  if (phase !== "available" && phase !== "downloading" && phase !== "uptodate" && phase !== "error") {
    return null;
  }

  const downloading = phase === "downloading";
  const pct = Math.round(progress * 100);

  return (
    <Modal title={t("update.title")} icon="bi-arrow-up-circle" onClose={downloading ? () => {} : onDismiss}>
      {phase === "uptodate" && (
        <>
          <p className="update-msg">{t("update.upToDate")}</p>
          <div className="modal-actions">
            <button className="btn" onClick={onDismiss}>
              {t("common.close")}
            </button>
          </div>
        </>
      )}

      {phase === "error" && (
        <>
          <p className="update-msg">{t("update.failed")}</p>
          {error && <pre className="update-error">{error}</pre>}
          <div className="modal-actions">
            <button className="btn" onClick={onDismiss}>
              {t("common.close")}
            </button>
          </div>
        </>
      )}

      {(phase === "available" || downloading) && update && (
        <>
          <p className="update-msg">
            {t("update.available", { version: update.version })}
          </p>
          {update.body && <pre className="update-notes">{update.body}</pre>}

          {downloading ? (
            <div className="update-progress">
              <div className="update-bar">
                <div className="update-bar-fill" style={{ width: `${pct}%` }} />
              </div>
              <span className="update-pct">{t("update.downloading")} {pct}%</span>
            </div>
          ) : (
            <div className="modal-actions">
              <button className="btn" onClick={onDismiss}>
                {t("update.later")}
              </button>
              <button className="btn primary-btn" onClick={onInstall}>
                <i className="bi bi-download" /> {t("update.installNow")}
              </button>
            </div>
          )}
        </>
      )}
    </Modal>
  );
}
