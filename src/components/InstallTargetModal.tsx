import { Modal } from "./Modal";
import { useT } from "../lib/i18n";
import type { AeInstallation } from "../types";

interface Props {
  installs: AeInstallation[];
  count: number;
  onPick: (ae: AeInstallation) => void;
  onCancel: () => void;
}

/** Choose which After Effects installation to install the selected packages into. */
export function InstallTargetModal({ installs, count, onPick, onCancel }: Props) {
  const t = useT();
  return (
    <Modal title={t("multiInstall.title")} icon="bi-download" onClose={onCancel} width={440}>
      <p className="settings-hint">{t("multiInstall.pick", { count })}</p>
      <ul className="ae-list">
        {installs.map((ae) => (
          <li key={ae.root}>
            <button className="ae-pick" onClick={() => onPick(ae)}>
              <i className="bi bi-box-arrow-in-down" />
              <span>{ae.name}</span>
              <code>{ae.root}</code>
            </button>
          </li>
        ))}
      </ul>
    </Modal>
  );
}
