/** Placeholder shown in the detail pane when nothing is selected. */
export function DetailEmpty({ icon, hint }: { icon: string; hint: string }) {
  return (
    <aside className="detail empty">
      <div className="detail-empty">
        <i className={`bi ${icon}`} />
        <p>{hint}</p>
      </div>
    </aside>
  );
}
