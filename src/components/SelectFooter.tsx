import { useState } from "react";
import { useT } from "../lib/i18n";
import type { Bundle } from "../types";
import { BundleSelectionModal } from "./BundleSelectionModal";

interface Props {
  count: number;
  busy: boolean;
  canInstall: boolean;
  bundles: Bundle[];
  onClear: () => void;
  onInstall: () => void;
  onCreate: (name: string) => Promise<boolean>;
  onAdd: (id: string) => Promise<boolean>;
  onBundled: () => void;
}

export function SelectFooter({ count, busy, canInstall, bundles, onClear, onInstall, onCreate, onAdd, onBundled }: Props) {
  const t = useT();
  const [bundleOpen, setBundleOpen] = useState(false);
  return <>
    <div className="library-footer select-footer">
      <div className="selection-summary">
        <span className="select-count" aria-live="polite">{t("selection.count", { count })}</span>
        <button className="btn ghost xs" disabled={busy || !count} onClick={onClear}>{t("selection.clear")}</button>
      </div>
      <div className="select-actions">
        <button className="btn primary-btn" disabled={busy || !count || !canInstall}
          title={!canInstall ? t("settings.aeNone") : undefined} onClick={onInstall}>
          <i className="bi bi-download" /> {t("selection.install")}
        </button>
        <button className="btn" disabled={busy || !count} onClick={() => setBundleOpen(true)}>
          <i className="bi bi-collection" /> {t("selection.bundleActions")}
        </button>
      </div>
    </div>
    {bundleOpen && <BundleSelectionModal count={count} bundles={bundles} busy={busy}
      onCreate={onCreate} onAdd={onAdd} onCancel={() => setBundleOpen(false)}
      onSuccess={() => { setBundleOpen(false); onBundled(); }} />}
  </>;
}
