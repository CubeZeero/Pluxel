import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type InstallParams } from "../lib/api";
import { useBanner } from "../lib/useBanner";
import { DetailEmpty } from "./DetailEmpty";
import { toUrl } from "../lib/text";
import { useT } from "../lib/i18n";
import { KIND_LABELS, type AeInstallation, type Bundle, type Package } from "../types";

// Members shown before the list collapses behind a "show more" toggle.
const MEMBER_LIMIT = 4;

interface Props {
  bundle: Bundle | null;
  packages: Package[];
  aeInstalls: AeInstallation[];
  busy: boolean;
  onSave: (bundle: Bundle) => void;
  onInstall: (bundle: Bundle, params: InstallParams) => void;
  onExport: (bundle: Bundle) => void;
  onDelete: (bundle: Bundle) => void;
  onSetBanner: (id: string) => void;
  onClearBanner: (id: string) => void;
}

export function BundleDetail(props: Props) {
  const { bundle } = props;
  const t = useT();
  if (!bundle) {
    return <DetailEmpty icon="bi-collection" hint={t("bundle.selectHint")} />;
  }
  return <BundleBody key={bundle.id} {...props} bundle={bundle} />;
}

function BundleBody({
  bundle,
  packages,
  aeInstalls,
  busy,
  onSave,
  onInstall,
  onExport,
  onDelete,
  onSetBanner,
  onClearBanner,
}: Props & { bundle: Bundle }) {
  const t = useT();
  const banner = useBanner(bundle.id, bundle.banner, bundle.updated_at, api.readBundleBanner);

  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(bundle.name);
  const [author, setAuthor] = useState(bundle.author);
  const [homepage, setHomepage] = useState(bundle.homepage);
  const [description, setDescription] = useState(bundle.description);
  const [memberIds, setMemberIds] = useState<string[]>(bundle.package_ids);
  const [aeSel, setAeSel] = useState<string>(aeInstalls.length ? "0" : "custom");
  // Member list is collapsed to the first few by default.
  const [membersExpanded, setMembersExpanded] = useState(false);

  const byId = new Map(packages.map((p) => [p.id, p]));
  const members = memberIds.map((id) => byId.get(id)).filter((p): p is Package => !!p);
  const savedMembers = bundle.package_ids
    .map((id) => byId.get(id))
    .filter((p): p is Package => !!p);

  const memberList = editing ? members : savedMembers;
  const memberOverflow = memberList.length - MEMBER_LIMIT;
  const shownMembers = membersExpanded ? memberList : memberList.slice(0, MEMBER_LIMIT);

  const save = () => {
    onSave({
      ...bundle,
      name: name.trim() || bundle.id,
      author: author.trim(),
      homepage: homepage.trim(),
      description,
      package_ids: memberIds,
    });
    setEditing(false);
  };
  const cancel = () => {
    setName(bundle.name);
    setAuthor(bundle.author);
    setHomepage(bundle.homepage);
    setDescription(bundle.description);
    setMemberIds(bundle.package_ids);
    setEditing(false);
  };

  const doInstall = () => {
    const ae = aeInstalls[Number(aeSel)];
    if (ae) onInstall(bundle, { installation: ae });
  };

  return (
    <aside className="detail">
      <div className="detail-banner">
        {banner ? (
          <img src={banner} alt="" draggable={false} />
        ) : (
          <div className="detail-banner-fallback">
            <span>{bundle.name || t("bundle.namePlaceholder")}</span>
          </div>
        )}
        <span className="detail-kind kind-plugin">
          <i className="bi bi-collection" style={{ marginRight: 4 }} />
          {t("tab.bundles")}
        </span>
        {editing && (
          <div className="banner-actions">
            <button className="banner-edit" onClick={() => onSetBanner(bundle.id)}>
              <i className="bi bi-image" /> {t("detail.changeBanner")}
            </button>
            {bundle.banner && (
              <button
                className="banner-edit banner-info-btn"
                title={t("detail.removeBanner")}
                onClick={() => onClearBanner(bundle.id)}
              >
                <i className="bi bi-trash" />
              </button>
            )}
          </div>
        )}
      </div>

      <div className="detail-scroll">
        <div className="detail-head">
          {editing ? (
            <input className="title-input" value={name} onChange={(e) => setName(e.target.value)} />
          ) : (
            <>
              <h2 className="detail-title">{bundle.name}</h2>
              <button className="icon-btn" title={t("common.edit")} onClick={() => setEditing(true)}>
                <i className="bi bi-pencil" />
              </button>
            </>
          )}
        </div>
        {editing && (
          <div className="edit-bar">
            <button className="btn danger" onClick={cancel}>
              {t("common.cancel")}
            </button>
            <button className="btn primary-btn" onClick={save} disabled={busy}>
              <i className="bi bi-check-lg" /> {t("common.save")}
            </button>
          </div>
        )}

        {/* -------- Info -------- */}
        {editing ? (
          <section className="detail-section">
            <label className="field">
              <span>{t("field.author")}</span>
              <input value={author} onChange={(e) => setAuthor(e.target.value)} />
            </label>
            <label className="field">
              <span>{t("field.homepage")}</span>
              <input value={homepage} placeholder="https://…" onChange={(e) => setHomepage(e.target.value)} />
            </label>
            <label className="field">
              <span>{t("field.description")}</span>
              <textarea rows={3} value={description} onChange={(e) => setDescription(e.target.value)} />
            </label>
          </section>
        ) : (
          (bundle.author || bundle.homepage || bundle.description) && (
            <section className="detail-section info-view">
              <dl>
                {bundle.author && (
                  <>
                    <dt>{t("field.author")}</dt>
                    <dd className="ellipsis">{bundle.author}</dd>
                  </>
                )}
                {bundle.homepage && (
                  <>
                    <dt>URL</dt>
                    <dd className="ellipsis">
                      <a
                        href="#"
                        onClick={(e) => {
                          e.preventDefault();
                          openUrl(toUrl(bundle.homepage));
                        }}
                      >
                        {bundle.homepage}
                      </a>
                    </dd>
                  </>
                )}
              </dl>
              {bundle.description && <p className="info-desc">{bundle.description}</p>}
            </section>
          )
        )}

        {/* -------- Members -------- */}
        <section className="detail-section">
          <h3>{t("bundle.members", { count: memberList.length })}</h3>
          {memberList.length > 0 && (
            <ul className="install-list">
              {shownMembers.map((p) => (
                <li key={p.id} className={`kind-${p.kind}`}>
                  <div className="install-info">
                    <span className="install-label">
                      <span className="member-dot" /> {p.manifest.name}
                    </span>
                    <span className="install-path">{KIND_LABELS[p.kind]}</span>
                  </div>
                  {editing && (
                    <div className="install-buttons">
                      <button
                        className="icon-btn danger-icon"
                        title={t("common.delete")}
                        onClick={() => setMemberIds((ids) => ids.filter((m) => m !== p.id))}
                      >
                        <i className="bi bi-x-lg" />
                      </button>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          )}
          {memberOverflow > 0 && (
            <button className="bundle-more" onClick={() => setMembersExpanded((v) => !v)}>
              {membersExpanded ? (
                <>
                  <i className="bi bi-chevron-up" /> {t("bundle.showLess")}
                </>
              ) : (
                <>
                  <i className="bi bi-chevron-down" /> {t("bundle.showMore", { count: memberOverflow })}
                </>
              )}
            </button>
          )}
          {editing && <p className="settings-hint">{t("bundle.addHint")}</p>}
        </section>

        {/* -------- Install series (view mode) -------- */}
        {!editing && savedMembers.length > 0 && (
          <section className="detail-section">
            <h3>{t("section.install")}</h3>
            {aeInstalls.length > 0 && (
              <label className="field">
                <select value={aeSel} onChange={(e) => setAeSel(e.target.value)}>
                  {aeInstalls.map((ae, i) => (
                    <option key={ae.root} value={String(i)}>
                      {ae.name}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <button
              className="btn install-btn"
              onClick={doInstall}
              disabled={busy || !aeInstalls.length}
            >
              <i className="bi bi-download" /> {t("bundle.install")}
            </button>
          </section>
        )}

        {/* -------- Export / delete (edit mode) -------- */}
        {editing && (
          <>
            <section className="detail-section">
              <h3>{t("section.distribute")}</h3>
              <button
                className="btn"
                onClick={() => onExport(bundle)}
                disabled={busy || !savedMembers.length}
              >
                <i className="bi bi-box-arrow-up" /> {t("bundle.export")}
              </button>
            </section>
            <section className="detail-section danger-zone">
              <button className="btn danger" onClick={() => onDelete(bundle)} disabled={busy}>
                <i className="bi bi-trash" /> {t("bundle.delete")}
              </button>
            </section>
          </>
        )}
      </div>
    </aside>
  );
}
