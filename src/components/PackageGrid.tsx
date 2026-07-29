import type { Package } from "../types";
import { useT } from "../lib/i18n";
import { PackageCard } from "./PackageCard";
import { PackageRow } from "./PackageRow";

interface Props {
  packages: Package[];
  selectedId: string | null;
  totalCount: number;
  viewMode: "grid" | "list";
  selectMode: boolean;
  checkedIds: Set<string>;
  onSelect: (id: string) => void;
  onCheck: (id: string) => void;
  onContext: (pkg: Package, x: number, y: number) => void;
}

export function PackageGrid({
  packages,
  selectedId,
  totalCount,
  viewMode,
  selectMode,
  checkedIds,
  onSelect,
  onCheck,
  onContext,
}: Props) {
  const t = useT();

  if (totalCount === 0) {
    return (
      <div className="grid empty">
        <div className="empty-state">
          <div className="empty-mark">◆</div>
          <h2>{t("empty.title")}</h2>
          <p>{t("empty.body")}</p>
        </div>
      </div>
    );
  }

  if (packages.length === 0) {
    return (
      <div className="grid empty">
        <div className="empty-state">
          <p>{t("empty.noMatch")}</p>
        </div>
      </div>
    );
  }

  if (viewMode === "list") {
    return (
      <div className="list-view">
        {packages.map((p) => (
          <PackageRow
            key={p.id}
            pkg={p}
            selected={p.id === selectedId}
            selectMode={selectMode}
            checked={checkedIds.has(p.id)}
            onSelect={onSelect}
            onCheck={onCheck}
            onContext={onContext}
          />
        ))}
      </div>
    );
  }

  return (
    <div className="grid">
      {packages.map((p) => (
        <PackageCard
          key={p.id}
          pkg={p}
          selected={p.id === selectedId}
          selectMode={selectMode}
          checked={checkedIds.has(p.id)}
          onSelect={onSelect}
          onCheck={onCheck}
          onContext={onContext}
        />
      ))}
    </div>
  );
}
