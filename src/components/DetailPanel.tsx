import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Modal } from "./Modal";
import { FileList } from "./FileList";
import { DetailEmpty } from "./DetailEmpty";
import { useBanner } from "../lib/useBanner";
import { countLogicalFiles } from "../lib/files";
import { customInstallDirs } from "../lib/theme";
import { isInstallBlocked, isMacOS } from "../lib/platform";
import { toUrl } from "../lib/text";
import { useT } from "../lib/i18n";
import { api, type InstallParams } from "../lib/api";
import {
  KIND_LABELS,
  KIND_ORDER,
  KIND_TARGETS,
  type AeInstallation,
  type Manifest,
  type Package,
  type PackageKind,
} from "../types";

interface Props {
  pkg: Package | null;
  aeInstalls: AeInstallation[];
  busy: boolean;
  onInstall: (pkg: Package, params: InstallParams) => void;
  onUninstall: (pkg: Package, recordId: string) => void;
  onSaveManifest: (id: string, manifest: Manifest) => void;
  onSetBanner: (id: string) => void;
  onClearBanner: (id: string) => void;
  onAddFiles: (id: string) => void;
  onAddFolder: (id: string) => void;
  onReplaceFiles: (id: string) => void;
  onRemoveFile: (id: string, relPath: string) => void;
  onExportPackage: (pkg: Package) => void;
  onRunInstaller: (pkg: Package) => void;
  onDelete: (pkg: Package) => void;
}

export function DetailPanel(props: Props) {
  const { pkg } = props;
  const t = useT();
  if (!pkg) {
    return <DetailEmpty icon="bi-box-seam" hint={t("detail.selectHint")} />;
  }
  return <DetailBody key={pkg.id} {...props} pkg={pkg} />;
}

function DetailBody({
  pkg,
  aeInstalls,
  busy,
  onInstall,
  onUninstall,
  onSaveManifest,
  onSetBanner,
  onClearBanner,
  onAddFiles,
  onAddFolder,
  onReplaceFiles,
  onRemoveFile,
  onExportPackage,
  onRunInstaller,
  onDelete,
}: Props & { pkg: Package }) {
  const banner = useBanner(pkg.id, pkg.banner, pkg.updated_at);
  const t = useT();
  const m = pkg.manifest;
  // Custom install folders are configured in Settings.
  const customDirs = customInstallDirs();
  const baseName = (dir: string) => dir.split(/[/\\]/).filter(Boolean).pop() ?? dir;
  // Folder an install record's files live in (parent of the first stored path).
  const installDir = (paths: string[]): string | null =>
    paths.length ? paths[0].replace(/[/\\][^/\\]*$/, "") : null;
  const reveal = (paths: string[]) => paths[0] && api.revealPath(paths[0]);

  // Copy a path to the clipboard, briefly flagging which row was copied.
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const copyPath = (key: string, path: string) => {
    navigator.clipboard.writeText(path).then(() => {
      setCopiedKey(key);
      setTimeout(() => setCopiedKey((k) => (k === key ? null : k)), 1500);
    });
  };

  const [editing, setEditing] = useState(false);
  const [showBannerInfo, setShowBannerInfo] = useState(false);

  // Edit-mode form state.
  const [name, setName] = useState(m.name);
  const [version, setVersion] = useState(m.version);
  const [author, setAuthor] = useState(m.author);
  const [homepage, setHomepage] = useState(m.homepage);
  const [description, setDescription] = useState(m.description);
  const [tags, setTags] = useState(m.tags.join(", "));
  const [installAs, setInstallAs] = useState<PackageKind>(pkg.kind);

  // Install target: an AE index, or "custom:<i>" for a folder set in Settings.
  const [aeSel, setAeSel] = useState<string>(
    aeInstalls.length ? "0" : customDirs.length ? "custom:0" : "0",
  );

  const save = () => {
    onSaveManifest(pkg.id, {
      name: name.trim() || pkg.id,
      version: version.trim(),
      author: author.trim(),
      homepage: homepage.trim(),
      description,
      tags: tags.split(",").map((s) => s.trim()).filter(Boolean),
      install_as: installAs,
    });
    setEditing(false);
  };

  // Discard edits: reseed every form field from the saved manifest before exiting.
  const cancel = () => {
    setName(m.name);
    setVersion(m.version);
    setAuthor(m.author);
    setHomepage(m.homepage);
    setDescription(m.description);
    setTags(m.tags.join(", "));
    setInstallAs(pkg.kind);
    setEditing(false);
  };

  const doInstall = () => {
    if (aeSel.startsWith("custom:")) {
      const dir = customDirs[Number(aeSel.slice(7))];
      if (dir) onInstall(pkg, { custom_dir: dir });
    } else {
      const ae = aeInstalls[Number(aeSel)];
      if (ae) onInstall(pkg, { installation: ae });
    }
  };

  // Install state for the currently-selected target (an AE installation).
  const selectedAe = aeSel.startsWith("custom:") ? null : aeInstalls[Number(aeSel)];
  const selectedRecord = selectedAe
    ? pkg.installs.find((r) => r.ae?.root === selectedAe.root) ?? null
    : null;
  // Native installers have no target; a record just means "user ran it".
  const installerRecord = pkg.installs.find((r) => r.kind === "installer") ?? null;
  // Every other install location (other AE versions, custom folders).
  const otherRecords = pkg.installs.filter(
    (r) => r.id !== selectedRecord?.id && r.id !== installerRecord?.id,
  );

  return (
    <aside className="detail">
      <div className="detail-banner">
        {banner ? (
          <img src={banner} alt="" draggable={false} />
        ) : (
          <div className="detail-banner-fallback">
            <span>{m.name || t("untitled")}</span>
          </div>
        )}
        <span className={`detail-kind kind-${pkg.kind}`}>{KIND_LABELS[pkg.kind]}</span>
        {editing && (
          <div className="banner-actions">
            <button className="banner-edit" onClick={() => onSetBanner(pkg.id)}>
              <i className="bi bi-image" /> {t("detail.changeBanner")}
            </button>
            {pkg.banner && (
              <button
                className="banner-edit banner-info-btn"
                title={t("detail.removeBanner")}
                onClick={() => onClearBanner(pkg.id)}
              >
                <i className="bi bi-trash" />
              </button>
            )}
            <button
              className="banner-edit banner-info-btn"
              title={t("detail.bannerSizeTip")}
              onClick={() => setShowBannerInfo(true)}
            >
              <i className="bi bi-info-circle" />
            </button>
          </div>
        )}
      </div>

      <div className="detail-scroll">
        {/* -------- Header + edit toggle -------- */}
        <div className="detail-head">
          {editing ? (
            <input
              className="title-input"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          ) : (
            <>
              <h2 className="detail-title">{m.name}</h2>
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
            <div className="field-row">
              <label className="field">
                <span>{t("field.version")}</span>
                <input value={version} onChange={(e) => setVersion(e.target.value)} />
              </label>
              <label className="field">
                <span>{t("field.author")}</span>
                <input value={author} onChange={(e) => setAuthor(e.target.value)} />
              </label>
            </div>
            <label className="field">
              <span>{t("field.homepage")}</span>
              <input
                value={homepage}
                placeholder="https://…"
                onChange={(e) => setHomepage(e.target.value)}
              />
            </label>
            <label className="field">
              <span>{t("field.description")}</span>
              <textarea
                rows={3}
                value={description}
                onChange={(e) => setDescription(e.target.value)}
              />
            </label>
            <label className="field">
              <span>{t("field.tags")}</span>
              <input value={tags} onChange={(e) => setTags(e.target.value)} />
            </label>
            <label className="field">
              <span>{t("field.kind")}</span>
              <select
                value={installAs}
                onChange={(e) => setInstallAs(e.target.value as PackageKind)}
              >
                {KIND_ORDER.map((k) => (
                  <option key={k} value={k}>
                    {KIND_LABELS[k]} → {KIND_TARGETS[k]}
                  </option>
                ))}
              </select>
            </label>
          </section>
        ) : (
          <section className="detail-section info-view">
            <dl>
              {m.version && (
                <>
                  <dt>{t("field.version")}</dt>
                  <dd>{m.version}</dd>
                </>
              )}
              {m.author && (
                <>
                  <dt>{t("field.author")}</dt>
                  <dd className="ellipsis">{m.author}</dd>
                </>
              )}
              {m.homepage && (
                <>
                  <dt>URL</dt>
                  <dd className="ellipsis">
                    <a
                      href="#"
                      onClick={(e) => {
                        e.preventDefault();
                        openUrl(toUrl(m.homepage));
                      }}
                    >
                      {m.homepage}
                    </a>
                  </dd>
                </>
              )}
            </dl>
            {m.description && <p className="info-desc">{m.description}</p>}
            {m.tags.length > 0 && (
              <div className="card-tags">
                {m.tags.map((tag) => (
                  <span key={tag} className="tag">
                    {tag}
                  </span>
                ))}
              </div>
            )}
          </section>
        )}

        {/* -------- Install (view mode only) -------- */}
        {!editing && (
        <section className="detail-section">
          <h3>{t("section.install")}</h3>
          {pkg.kind === "installer" ? (
            installerRecord ? (
              <div className="installed-state">
                <span className="installed-badge">
                  <i className="bi bi-check-circle-fill" /> {t("install.installed")}
                </span>
                <div className="installed-actions">
                  <button className="btn" disabled={busy} onClick={() => onRunInstaller(pkg)}>
                    <i className="bi bi-box-arrow-up-right" /> {t("installer.rerun")}
                  </button>
                  <button
                    className="btn danger"
                    disabled={busy}
                    onClick={() => onUninstall(pkg, installerRecord.id)}
                  >
                    <i className="bi bi-x-circle" /> {t("installer.unmark")}
                  </button>
                </div>
              </div>
            ) : (
              <>
                <p className="settings-hint">{t("installer.hint")}</p>
                <button
                  className="btn install-btn"
                  onClick={() => onRunInstaller(pkg)}
                  disabled={busy}
                >
                  <i className="bi bi-box-arrow-up-right" /> {t("installer.run")}
                </button>
              </>
            )
          ) : isInstallBlocked(pkg) ? (
            <p className="settings-hint install-blocked">
              <i className="bi bi-exclamation-triangle" />{" "}
              {t(isMacOS ? "install.aexUnsupported" : "install.pluginUnsupported")}
            </p>
          ) : (
            <>
              <label className="field">
                <select value={aeSel} onChange={(e) => setAeSel(e.target.value)}>
                  {aeInstalls.map((ae, i) => (
                    <option key={ae.root} value={String(i)}>
                      {ae.name}
                    </option>
                  ))}
                  {customDirs.map((dir, i) => (
                    <option key={dir} value={`custom:${i}`}>
                      {t("install.customFolder")}: {baseName(dir)}
                    </option>
                  ))}
                </select>
              </label>
              {selectedRecord ? (
                <div className="installed-state">
                  <span className="installed-badge">
                    <i className="bi bi-check-circle-fill" /> {t("install.installed")}
                  </span>
                  {installDir(selectedRecord.paths) && (
                    <div className="install-location">
                      <code className="install-abs-path" title={installDir(selectedRecord.paths)!}>
                        {installDir(selectedRecord.paths)}
                      </code>
                      <button
                        className="icon-btn"
                        title={t("install.copyPath")}
                        onClick={() => copyPath(selectedRecord.id, installDir(selectedRecord.paths)!)}
                      >
                        <i className={`bi ${copiedKey === selectedRecord.id ? "bi-check-lg" : "bi-clipboard"}`} />
                      </button>
                      <button
                        className="icon-btn"
                        title={t("install.reveal")}
                        onClick={() => reveal(selectedRecord.paths)}
                      >
                        <i className="bi bi-folder2-open" />
                      </button>
                    </div>
                  )}
                  <div className="installed-actions">
                    <button
                      className="btn"
                      disabled={busy}
                      onClick={() =>
                        onInstall(pkg, { installation: selectedAe!, label: selectedRecord.label })
                      }
                    >
                      <i className="bi bi-arrow-clockwise" /> {t("install.updateBtn")}
                    </button>
                    <button
                      className="btn danger"
                      disabled={busy}
                      onClick={() => onUninstall(pkg, selectedRecord.id)}
                    >
                      <i className="bi bi-trash" /> {t("install.uninstall")}
                    </button>
                  </div>
                </div>
              ) : (
                <button className="btn install-btn" onClick={doInstall} disabled={busy}>
                  <i className="bi bi-download" /> {t("install.button")}
                </button>
              )}
            </>
          )}
        </section>
        )}

        {/* -------- Other install locations (view mode only) -------- */}
        {!editing && otherRecords.length > 0 && (
          <section className="detail-section">
            <h3>{t("install.otherTargets")}</h3>
            <ul className="install-list">
              {otherRecords.map((rec) => (
                <li key={rec.id}>
                  <div className="install-info">
                    <span className="install-label">{rec.label}</span>
                    <span className="install-path" title={installDir(rec.paths) ?? rec.target_dir}>
                      {installDir(rec.paths) ?? rec.target_dir}
                    </span>
                  </div>
                  <div className="install-buttons">
                    {installDir(rec.paths) && (
                      <>
                        <button
                          className="icon-btn"
                          title={t("install.copyPath")}
                          onClick={() => copyPath(rec.id, installDir(rec.paths)!)}
                        >
                          <i className={`bi ${copiedKey === rec.id ? "bi-check-lg" : "bi-clipboard"}`} />
                        </button>
                        <button
                          className="icon-btn"
                          title={t("install.reveal")}
                          onClick={() => reveal(rec.paths)}
                        >
                          <i className="bi bi-folder2-open" />
                        </button>
                      </>
                    )}
                    <button
                      className="icon-btn"
                      title={t("install.update")}
                      disabled={busy}
                      onClick={() =>
                        onInstall(
                          pkg,
                          rec.ae
                            ? { installation: rec.ae, label: rec.label }
                            : { custom_dir: rec.target_dir, label: rec.label },
                        )
                      }
                    >
                      <i className="bi bi-arrow-clockwise" />
                    </button>
                    <button
                      className="icon-btn danger-icon"
                      title={t("install.uninstall")}
                      disabled={busy}
                      onClick={() => onUninstall(pkg, rec.id)}
                    >
                      <i className="bi bi-trash" />
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          </section>
        )}

        {/* -------- Files -------- */}
        <section className="detail-section">
          <div className="section-head">
            <h3>{t("section.files", { count: countLogicalFiles(pkg) })}</h3>
            {editing && (
              <div className="section-actions">
                <button className="btn ghost xs" onClick={() => onAddFiles(pkg.id)}>
                  <i className="bi bi-file-earmark-plus" /> {t("files.addFiles")}
                </button>
                <button className="btn ghost xs" onClick={() => onAddFolder(pkg.id)}>
                  <i className="bi bi-folder-plus" /> {t("files.addFolder")}
                </button>
                <button className="btn ghost xs" onClick={() => onReplaceFiles(pkg.id)}>
                  <i className="bi bi-arrow-repeat" /> {t("files.replace")}
                </button>
              </div>
            )}
          </div>
          <FileList pkg={pkg} editing={editing} onRemoveFile={onRemoveFile} />
          {editing && <p className="settings-hint file-hint">{t("files.companionHint")}</p>}
        </section>

        {/* -------- Distribution export (edit mode only) -------- */}
        {editing && (
          <section className="detail-section">
            <h3>{t("section.distribute")}</h3>
            <button className="btn" onClick={() => onExportPackage(pkg)} disabled={busy}>
              <i className="bi bi-box-arrow-up" /> {t("distribute.export")}
            </button>
          </section>
        )}

        {/* -------- Danger -------- */}
        {editing && (
          <section className="detail-section danger-zone">
            <button className="btn danger" onClick={() => onDelete(pkg)}>
              <i className="bi bi-trash" /> {t("detail.deleteFromLibrary")}
            </button>
          </section>
        )}
      </div>

      {showBannerInfo && (
        <Modal
          title={t("bannerGuide.title")}
          icon="bi-image"
          width={440}
          onClose={() => setShowBannerInfo(false)}
        >
          <div className="banner-guide">
            <div className="banner-guide-ratio">
              <div className="ratio-box">16 : 9</div>
            </div>
            <dl className="banner-guide-list">
              <dt>{t("bannerGuide.ratio")}</dt>
              <dd>{t("bannerGuide.ratioVal")}</dd>
              <dt>{t("bannerGuide.recommended")}</dt>
              <dd>1280 × 720 px</dd>
              <dt>{t("bannerGuide.minimum")}</dt>
              <dd>640 × 360 px</dd>
              <dt>{t("bannerGuide.format")}</dt>
              <dd>PNG / JPG / WebP</dd>
            </dl>
            <p className="settings-hint">{t("bannerGuide.note")}</p>
          </div>
        </Modal>
      )}
    </aside>
  );
}
