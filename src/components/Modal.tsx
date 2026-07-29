import { useEffect, type ReactNode } from "react";
import { useT } from "../lib/i18n";

interface Props {
  title: string;
  icon?: string;
  onClose: () => void;
  children: ReactNode;
  width?: number;
}

export function Modal({ title, icon, onClose, children, width = 460 }: Props) {
  const t = useT();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        style={{ width }}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <div className="modal-head">
          <h2>
            {icon && <i className={`bi ${icon}`} />} {title}
          </h2>
          <button className="icon-btn" onClick={onClose} aria-label={t("common.close")}>
            <i className="bi bi-x-lg" />
          </button>
        </div>
        <div className="modal-body">{children}</div>
      </div>
    </div>
  );
}
