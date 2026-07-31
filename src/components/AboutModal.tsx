import { openUrl } from "@tauri-apps/plugin-opener";
import { Modal } from "./Modal";
import { useT } from "../lib/i18n";
import appIcon from "../assets/app-icon.png";

// Main open-source libraries Pluxel is built on, with their licenses.
const LIBS: { name: string; license: string }[] = [
  { name: "Tauri", license: "MIT / Apache-2.0" },
  { name: "React", license: "MIT" },
  { name: "Vite", license: "MIT" },
  { name: "TypeScript", license: "Apache-2.0" },
  { name: "Bootstrap Icons", license: "MIT" },
  { name: "serde / serde_json", license: "MIT / Apache-2.0" },
  { name: "zip", license: "MIT" },
  { name: "walkdir", license: "MIT / Unlicense" },
  { name: "chrono", license: "MIT / Apache-2.0" },
  { name: "uuid", license: "MIT / Apache-2.0" },
];

export function AboutModal({ onClose }: { onClose: () => void }) {
  const t = useT();
  return (
    <Modal title={t("toolbar.about")} icon="bi-info-circle" onClose={onClose}>
      <div className="about">
        <img className="about-logo" src={appIcon} alt="" draggable={false} />
        <h3>Pluxel</h3>
        <p className="about-ver">version 1.0.1</p>
        <p className="about-desc">{t("about.desc")}</p>
        <ul className="about-list">
          <li>{t("about.list1")}</li>
          <li>{t("about.list2")}</li>
          <li>{t("about.list3")}</li>
        </ul>

        <div className="about-links">
          <button className="btn ghost xs" onClick={() => openUrl("https://pluxel.cubezeero.com")}>
            <i className="bi bi-globe2" /> {t("about.website")}
          </button>
          <button className="btn ghost xs" onClick={() => openUrl("https://github.com/CubeZeero/Pluxel")}>
            <i className="bi bi-github" /> GitHub
          </button>
        </div>

        <h4 className="about-libs-title">{t("about.libs")}</h4>
        <ul className="about-libs">
          {LIBS.map((l) => (
            <li key={l.name}>
              <span className="lib-name">{l.name}</span>
              <span className="lib-lic">{l.license}</span>
            </li>
          ))}
        </ul>

        <p className="about-copyright">© 2026 cubezeero · {t("about.license")}</p>
        <p className="about-foot">{t("about.built")}</p>
      </div>
    </Modal>
  );
}
