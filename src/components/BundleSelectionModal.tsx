import { useEffect, useRef, useState, type FormEvent } from "react";
import { useT } from "../lib/i18n";
import type { Bundle } from "../types";
import { Modal } from "./Modal";

interface Props {
  count: number;
  bundles: Bundle[];
  busy: boolean;
  onCreate: (name: string) => Promise<boolean>;
  onAdd: (id: string) => Promise<boolean>;
  onSuccess: () => void;
  onCancel: () => void;
}

export function BundleSelectionModal({ count, bundles, busy, onCreate, onAdd, onSuccess, onCancel }: Props) {
  const t = useT();
  const [mode, setMode] = useState<"create" | "add">("create");
  const [name, setName] = useState("");
  const [target, setTarget] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState(false);
  const submitting = useRef(false);
  const root = useRef<HTMLFormElement>(null);
  const working = busy || pending;
  const valid = count > 0 && (mode === "create" ? !!name.trim() : bundles.some((b) => b.id === target));

  useEffect(() => {
    const previous = document.activeElement;
    const dialog = root.current?.closest<HTMLElement>('[role="dialog"]');
    if (!dialog) return;
    dialog.setAttribute("aria-label", t("selection.bundleActions"));
    const trap = (event: KeyboardEvent) => {
      if (event.key !== "Tab") return;
      const controls = Array.from(dialog.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled)'));
      const index = controls.indexOf(document.activeElement as HTMLElement);
      if (!controls.length) { event.preventDefault(); return; }
      if (event.shiftKey && index <= 0) { event.preventDefault(); controls[controls.length - 1].focus(); }
      else if (!event.shiftKey && (index < 0 || index === controls.length - 1)) { event.preventDefault(); controls[0].focus(); }
    };
    dialog.addEventListener("keydown", trap);
    return () => {
      dialog.removeEventListener("keydown", trap);
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, [t]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (working || submitting.current || !valid) return;
    submitting.current = true;
    setPending(true);
    setError(false);
    try {
      const ok = await (mode === "create" ? onCreate(name.trim()) : onAdd(target));
      if (ok) onSuccess();
      else setError(true);
    } catch {
      setError(true);
    } finally {
      submitting.current = false;
      setPending(false);
    }
  };

  return <Modal title={t("selection.bundleActions")} icon="bi-collection" width={440}
    onClose={() => { if (!working && !submitting.current) onCancel(); }}>
    <form ref={root} className="selection-bundle-form" onSubmit={submit} aria-busy={working}>
      <p className="settings-hint">{t("selection.count", { count })}</p>
      <div className="seg-toggle selection-bundle-tabs" role="group" aria-label={t("selection.bundleActions")}>
        <button type="button" className={`seg-btn${mode === "create" ? " active" : ""}`} aria-pressed={mode === "create"}
          disabled={working} onClick={() => { setMode("create"); setError(false); }}>{t("selection.createTab")}</button>
        <button type="button" className={`seg-btn${mode === "add" ? " active" : ""}`} aria-pressed={mode === "add"}
          disabled={working} onClick={() => { setMode("add"); setError(false); }}>{t("selection.addTab")}</button>
      </div>
      {mode === "create" ? <label className="field">
        <span>{t("selection.name")}</span>
        <input autoFocus value={name} disabled={working} placeholder={t("bundle.untitled")}
          onChange={(event) => setName(event.target.value)} />
      </label> : <label className="field">
        <span>{t("selection.existing")}</span>
        <select autoFocus value={target} disabled={working || !bundles.length} onChange={(event) => setTarget(event.target.value)}>
          <option value="">{t("selection.chooseBundle")}</option>
          {bundles.map((bundle) => <option key={bundle.id} value={bundle.id}>{bundle.name}</option>)}
        </select>
        {!bundles.length && <span className="settings-hint">{t("selection.noBundles")}</span>}
      </label>}
      {error && <p className="selection-error" role="alert">{t("selection.failed")}</p>}
      <div className="modal-actions">
        <button type="button" className="btn ghost" disabled={working} onClick={onCancel}>{t("common.cancel")}</button>
        <button type="submit" className="btn primary-btn" disabled={working || !valid}>
          {t(working ? "selection.working" : mode === "create" ? "selection.create" : "selection.add")}
        </button>
      </div>
    </form>
  </Modal>;
}
