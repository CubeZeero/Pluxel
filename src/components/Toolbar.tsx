import { useState } from "react";
import { useT } from "../lib/i18n";
import { ContextMenu } from "./ContextMenu";
import appIcon from "../assets/app-icon.png";

interface Props {
  query: string;
  onQuery: (v: string) => void;
  busy: boolean;
  onImportPackage: () => void;
  onImportFiles: () => void;
  onOpenSettings: () => void;
  onOpenAbout: () => void;
  viewMode: "grid" | "list";
  onViewMode: (v: "grid" | "list") => void;
  mode: "packages" | "bundles";
  onMode: (m: "packages" | "bundles") => void;
  selectMode: boolean;
  onToggleSelect: () => void;
}

export function Toolbar(props: Props) {
  const t = useT();
  const [addMenu, setAddMenu] = useState<{ x: number; y: number } | null>(null);

  return (
    <header className="toolbar">
      <div className="brand">
        <img className="brand-mark" src={appIcon} alt="" draggable={false} />
        <span className="brand-name">Pluxel</span>
      </div>

      <div className="seg-toggle tab-toggle">
        <button
          className={`seg-btn${props.mode === "packages" ? " active" : ""}`}
          onClick={() => props.onMode("packages")}
        >
          {t("tab.packages")}
        </button>
        <button
          className={`seg-btn${props.mode === "bundles" ? " active" : ""}`}
          onClick={() => props.onMode("bundles")}
        >
          {t("tab.bundles")}
        </button>
      </div>

      <div className="toolbar-search">
        <i className="bi bi-search" />
        <input
          type="search"
          placeholder={t("toolbar.search")}
          value={props.query}
          onChange={(e) => props.onQuery(e.target.value)}
        />
      </div>

      <div className="toolbar-actions">
        <button
          className="btn primary-btn"
          disabled={props.busy}
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            setAddMenu({ x: r.left, y: r.bottom + 4 });
          }}
        >
          <i className="bi bi-plus-lg" /> {t("toolbar.add")} <i className="bi bi-chevron-down" />
        </button>
        <span className="sep" />
        <div className="view-toggle">
          <button
            className={`icon-btn${props.viewMode === "grid" ? " active" : ""}`}
            onClick={() => props.onViewMode("grid")}
            title={t("toolbar.gridView")}
          >
            <i className="bi bi-grid-3x3-gap-fill" />
          </button>
          <button
            className={`icon-btn${props.viewMode === "list" ? " active" : ""}`}
            onClick={() => props.onViewMode("list")}
            title={t("toolbar.listView")}
          >
            <i className="bi bi-list-ul" />
          </button>
        </div>
        {props.mode === "packages" && (
          <button
            className={`icon-btn${props.selectMode ? " active" : ""}`}
            onClick={props.onToggleSelect}
            title={t("select.mode")}
          >
            <i className="bi bi-check2-square" />
          </button>
        )}
        <span className="sep" />
        <button className="icon-btn" onClick={props.onOpenSettings} title={t("toolbar.settings")}>
          <i className="bi bi-gear" />
        </button>
        <button className="icon-btn" onClick={props.onOpenAbout} title={t("toolbar.about")}>
          <i className="bi bi-info-circle" />
        </button>
      </div>

      {addMenu && (
        <ContextMenu
          x={addMenu.x}
          y={addMenu.y}
          onClose={() => setAddMenu(null)}
          items={[
            {
              label: t("toolbar.addPackage"),
              icon: "bi-box-seam",
              onClick: props.onImportPackage,
            },
            {
              label: t("toolbar.addAssets"),
              icon: "bi-file-earmark-plus",
              onClick: props.onImportFiles,
            },
          ]}
        />
      )}
    </header>
  );
}
