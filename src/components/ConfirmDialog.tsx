import { Modal } from "./Modal";
import { useT } from "../lib/i18n";
import type { ConfirmOptions } from "../lib/useLibrary";

interface Props extends ConfirmOptions {
  onResult: (ok: boolean) => void;
}

export function ConfirmDialog({
  title,
  message,
  okLabel,
  cancelLabel,
  danger,
  onResult,
}: Props) {
  const t = useT();
  return (
    <Modal
      title={title}
      icon={danger ? "bi-exclamation-triangle" : "bi-question-circle"}
      width={400}
      onClose={() => onResult(false)}
    >
      <p className="confirm-message">{message}</p>
      <div className="confirm-actions">
        <button className="btn ghost" onClick={() => onResult(false)}>
          {cancelLabel ?? t("common.cancel")}
        </button>
        <button
          className={`btn ${danger ? "danger-solid" : "primary-btn"}`}
          onClick={() => onResult(true)}
          autoFocus
        >
          {okLabel ?? "OK"}
        </button>
      </div>
    </Modal>
  );
}
