import { useEffect } from "react";

export interface MenuItem {
  label: string;
  icon?: string;
  danger?: boolean;
  onClick: () => void;
}

interface Props {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}

const WIDTH = 190;
const ITEM_H = 36;

export function ContextMenu({ x, y, items, onClose }: Props) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);

  // Keep the menu inside the viewport.
  const left = Math.min(x, window.innerWidth - WIDTH - 8);
  const top = Math.min(y, window.innerHeight - items.length * ITEM_H - 8);

  return (
    <div className="context-backdrop" onClick={onClose} onContextMenu={(e) => e.preventDefault()}>
      <div
        className="context-menu"
        style={{ left, top, width: WIDTH }}
        onClick={(e) => e.stopPropagation()}
      >
        {items.map((it) => (
          <button
            key={it.label}
            className={`context-item${it.danger ? " danger" : ""}`}
            onClick={() => {
              it.onClick();
              onClose();
            }}
          >
            {it.icon && <i className={`bi ${it.icon}`} />}
            {it.label}
          </button>
        ))}
      </div>
    </div>
  );
}
