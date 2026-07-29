import { api } from "../lib/api";
import { useBanner } from "../lib/useBanner";
import { useT } from "../lib/i18n";
import type { Bundle } from "../types";

interface Props {
  bundle: Bundle;
  selected: boolean;
  onSelect: (id: string) => void;
}

export function BundleCard({ bundle, selected, onSelect }: Props) {
  const banner = useBanner(bundle.id, bundle.banner, bundle.updated_at, api.readBundleBanner);
  const t = useT();

  return (
    <button
      className={`card bundle-card${selected ? " selected" : ""}`}
      onClick={() => onSelect(bundle.id)}
    >
      <div className="card-banner">
        {banner ? (
          <img src={banner} alt="" draggable={false} />
        ) : (
          <div className="card-banner-fallback">
            <span>{bundle.name || t("bundle.untitled")}</span>
          </div>
        )}
        <span className="card-kind">
          <i className="bi bi-collection" /> {t("bundle.count", { count: bundle.package_ids.length })}
        </span>
      </div>
      <div className="card-info">
        <div className="card-title">{bundle.name}</div>
        <div className="card-meta">{bundle.author && <span>{bundle.author}</span>}</div>
      </div>
    </button>
  );
}
