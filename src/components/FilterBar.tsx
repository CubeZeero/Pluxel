import { useT } from "../lib/i18n";
import { KIND_I18N, KIND_LABELS, KIND_ORDER, type PackageKind } from "../types";

interface Props {
  counts: Partial<Record<PackageKind, number>>;
  active: Set<PackageKind>;
  total: number;
  onToggle: (k: PackageKind) => void;
  onClear: () => void;
  bundledCount: number;
  showBundled: boolean;
  onToggleBundled: () => void;
}

export function FilterBar({
  counts,
  active,
  total,
  onToggle,
  onClear,
  bundledCount,
  showBundled,
  onToggleBundled,
}: Props) {
  const t = useT();
  // Nothing to show and no bundled packages to reveal → hide the bar entirely.
  if (total === 0 && bundledCount === 0) return null;
  const kinds = KIND_ORDER.filter((k) => counts[k]);

  return (
    <div className="filter-bar">
      <button
        className={`filter-chip${active.size === 0 ? " active" : ""}`}
        onClick={onClear}
      >
        {t("filter.all")} <span className="filter-count">{total}</span>
      </button>
      {kinds.map((k) => (
        <button
          key={k}
          className={`filter-chip kind-${k}${active.has(k) ? " active" : ""}`}
          onClick={() => onToggle(k)}
          title={KIND_LABELS[k]}
        >
          <span className="filter-dot" />
          {t(KIND_I18N[k])} <span className="filter-count">{counts[k]}</span>
        </button>
      ))}
      {bundledCount > 0 && (
        <button
          className={`filter-chip bundled-chip${showBundled ? " active" : ""}`}
          onClick={onToggleBundled}
        >
          <i className="bi bi-collection" /> {t("filter.bundled")}{" "}
          <span className="filter-count">{bundledCount}</span>
        </button>
      )}
    </div>
  );
}
