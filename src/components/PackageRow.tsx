import { useBanner } from "../lib/useBanner";
import { useT } from "../lib/i18n";
import { KIND_LABELS, isBundleable, type Package } from "../types";

interface Props {
  pkg: Package;
  selected: boolean;
  selectMode: boolean;
  checked: boolean;
  onSelect: (id: string) => void;
  onCheck: (id: string) => void;
  onContext: (pkg: Package, x: number, y: number) => void;
}

export function PackageRow({
  pkg,
  selected,
  selectMode,
  checked,
  onSelect,
  onCheck,
  onContext,
}: Props) {
  const banner = useBanner(pkg.id, pkg.banner, pkg.updated_at);
  const t = useT();
  const m = pkg.manifest;
  const eligible = isBundleable(pkg.kind);
  const disabled = selectMode && !eligible;

  return (
    <button
      className={`row kind-${pkg.kind}${selected && !selectMode ? " selected" : ""}${
        selectMode && checked ? " checked" : ""
      }${disabled ? " dimmed" : ""}`}
      onClick={() => (selectMode ? eligible && onCheck(pkg.id) : onSelect(pkg.id))}
      onContextMenu={(e) => {
        e.preventDefault();
        onContext(pkg, e.clientX, e.clientY);
      }}
    >
      {selectMode && eligible && (
        <span className={`row-check${checked ? " on" : ""}`}>
          <i className={`bi ${checked ? "bi-check-circle-fill" : "bi-circle"}`} />
        </span>
      )}
      <div className="row-thumb">
        {banner ? (
          <img src={banner} alt="" draggable={false} />
        ) : (
          <span className="row-thumb-fallback">{m.name.slice(0, 1).toUpperCase() || "?"}</span>
        )}
      </div>
      <div className="row-main">
        <span className="row-title">{m.name || t("untitled")}</span>
        {m.description && <span className="row-desc">{m.description}</span>}
      </div>
      <div className="row-meta">
        {m.version && <span>{m.version}</span>}
        {m.author && <span className="row-author">{m.author}</span>}
      </div>
      <span className="row-kind">
        <span className="row-dot" />
        {KIND_LABELS[pkg.kind]}
      </span>
      <span className="row-installed">
        {pkg.installs.length > 0 && (
          <i className="bi bi-check-circle-fill" title={t("installed")} />
        )}
      </span>
    </button>
  );
}
