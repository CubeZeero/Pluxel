import { useState } from "react";
import { Modal } from "./Modal";
import { useT } from "../lib/i18n";

interface Props {
  onSubmit: (password: string) => Promise<void>;
  onCancel: () => void;
}

export function AdminUnlockModal({ onSubmit, onCancel }: Props) {
  const t = useT();
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async () => {
    if (!password || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onSubmit(password);
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ""));
      setBusy(false);
    }
  };

  return (
    <Modal title={t("admin.title")} icon="bi-shield-lock" onClose={onCancel} width={420}>
      <p className="admin-explain">{t("admin.explain")}</p>
      <label className="field">
        <span>{t("admin.password")}</span>
        <input
          type="password"
          autoFocus
          value={password}
          disabled={busy}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
        />
      </label>
      {error && <p className="admin-error">{error}</p>}
      <div className="edit-bar">
        <button className="btn" onClick={onCancel} disabled={busy}>
          {t("common.cancel")}
        </button>
        <button className="btn primary-btn" onClick={submit} disabled={busy || !password}>
          <i className="bi bi-unlock" /> {t("admin.unlock")}
        </button>
      </div>
    </Modal>
  );
}
