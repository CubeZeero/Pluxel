import { useT } from "../lib/i18n";
import { BundleCard } from "./BundleCard";
import type { Bundle } from "../types";

interface Props {
  bundles: Bundle[];
  selectedId: string | null;
  busy: boolean;
  viewMode: "grid" | "list";
  onSelect: (id: string) => void;
  onCreate: () => void;
}

export function BundleList({ bundles, selectedId, busy, viewMode, onSelect, onCreate }: Props) {
  const t = useT();

  return (
    <div className="bundle-pane">
      <div className="bundle-pane-head">
        <button className="btn primary-btn" onClick={onCreate} disabled={busy}>
          <i className="bi bi-plus-lg" /> {t("bundle.new")}
        </button>
      </div>
      {bundles.length === 0 ? (
        <div className="empty-state bundle-empty">
          <div className="empty-mark">◆</div>
          <h2>{t("bundle.emptyTitle")}</h2>
          <p>{t("bundle.emptyBody")}</p>
        </div>
      ) : viewMode === "grid" ? (
        <div className="grid">
          {bundles.map((b) => (
            <BundleCard
              key={b.id}
              bundle={b}
              selected={b.id === selectedId}
              onSelect={onSelect}
            />
          ))}
        </div>
      ) : (
        <ul className="bundle-list">
          {bundles.map((b) => (
            <li key={b.id}>
              <button
                className={`bundle-row${b.id === selectedId ? " selected" : ""}`}
                onClick={() => onSelect(b.id)}
              >
                <i className="bi bi-collection" />
                <span className="bundle-row-name">{b.name}</span>
                <span className="bundle-row-count">
                  {t("bundle.count", { count: b.package_ids.length })}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
