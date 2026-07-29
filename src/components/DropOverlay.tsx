import { useT } from "../lib/i18n";

export function DropOverlay({ visible }: { visible: boolean }) {
  const t = useT();
  if (!visible) return null;
  return (
    <div className="drop-overlay">
      <div className="drop-inner">
        <i className="bi bi-download" />
        <p>{t("drop.title")}</p>
        <span>{t("drop.sub")}</span>
      </div>
    </div>
  );
}
