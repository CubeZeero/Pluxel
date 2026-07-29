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

export function PackageCard({
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
      className={`card kind-${pkg.kind}${selected && !selectMode ? " selected" : ""}${
        selectMode && checked ? " checked" : ""
      }${disabled ? " dimmed" : ""}`}
      onClick={() => (selectMode ? eligible && onCheck(pkg.id) : onSelect(pkg.id))}
      onContextMenu={(e) => {
        e.preventDefault();
        onContext(pkg, e.clientX, e.clientY);
      }}
    >
      {selectMode && eligible && (
        <span className={`card-check${checked ? " on" : ""}`}>
          <i className={`bi ${checked ? "bi-check-circle-fill" : "bi-circle"}`} />
        </span>
      )}
      <div className="card-banner">
        {banner ? (
          <img src={banner} alt="" draggable={false} />
        ) : (
          <div className="card-banner-fallback">
            <span>{m.name || t("untitled")}</span>
          </div>
        )}
        <span className="card-kind">{KIND_LABELS[pkg.kind]}</span>
      </div>
      <div className="card-info">
        <div className="card-title">{m.name}</div>
        <div className="card-meta">
          {m.version && <span>{m.version}</span>}
          {m.author && <span>{m.author}</span>}
        </div>
        {m.tags.length > 0 && (
          <div className="card-tags">
            {m.tags.slice(0, 3).map((tag) => (
              <span key={tag} className="tag">
                {tag}
              </span>
            ))}
          </div>
        )}
      </div>
    </button>
  );
}
