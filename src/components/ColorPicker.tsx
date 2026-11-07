import { useState, type PointerEvent } from "react";
import { useT } from "../lib/i18n";

interface ColorPickerProps {
  value: string;
  onChange: (color: string) => void;
  label: string;
  presets?: string[];
}

type HsvColor = [hue: number, saturation: number, brightness: number];

interface PickerState {
  value: string;
  draft: string;
  hue: number;
}

function normalizeHex(value: string): string | null {
  const digits = value.trim().replace(/^#/, "");
  return /^[0-9a-f]{6}$/i.test(digits) ? `#${digits.toLowerCase()}` : null;
}

function hsv(value: string): HsvColor {
  const hex = normalizeHex(value) ?? "#000000";
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const max = Math.max(r, g, b);
  const d = max - Math.min(r, g, b);
  const h = !d ? 0 : max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, max ? d / max : 0, max];
}
function hex(h: number, s: number, v: number): string {
  const f = (n: number) => {
    const k = (n + h / 60) % 6;
    return Math.round(255 * (v - v * s * Math.max(0, Math.min(k, 4 - k, 1)))).toString(16).padStart(2, "0");
  };
  return `#${f(5)}${f(3)}${f(1)}`;
}

/** Inline picker: no native color dialog, with pointer and keyboard controls. */
export function ColorPicker({ value, onChange, label, presets = [] }: ColorPickerProps) {
  const t = useT();
  const [expanded, setExpanded] = useState(false);
  const [state, setState] = useState<PickerState>(() => ({
    value,
    draft: normalizeHex(value) ?? value,
    hue: hsv(value)[0],
  }));
  const [valueHue, s, v] = hsv(value);

  // Reconcile before rendering children, including while the editor is open.
  if (state.value !== value) {
    setState({
      value,
      draft: normalizeHex(value) ?? value,
      hue: s > 0 ? valueHue : state.hue,
    });
  }

  const { draft, hue } = state;
  const resetDraft = () => {
    setState((previous) => ({ ...previous, draft: normalizeHex(value) ?? value }));
  };
  const choose = (color: string, selectedHue?: number) => {
    const normalized = normalizeHex(color);
    if (!normalized) return;
    const [nextHue, nextSaturation] = hsv(normalized);
    setState({
      value: normalized,
      draft: normalized,
      hue: selectedHue ?? (nextSaturation > 0 ? nextHue : hue),
    });
    onChange(normalized);
  };
  const move = (e: PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    if (!rect.width || !rect.height) return;
    const saturation = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const brightness = 1 - Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height));
    choose(hex(hue, saturation, brightness), hue);
  };
  return (
    <div className="color-picker">
      <button
        type="button"
        className="color-trigger"
        aria-expanded={expanded}
        onClick={() => {
          resetDraft();
          setExpanded(!expanded);
        }}
      >
        <span className="color-preview" style={{ background: value }} />
        <span>{label}</span>
        <code>{value.toUpperCase()}</code>
        <i className={`bi bi-chevron-${expanded ? "up" : "down"}`} aria-hidden="true" />
      </button>
      {expanded && (
        <div className="color-editor">
          <div
            className="color-plane"
            style={{ backgroundColor: `hsl(${hue} 100% 50%)` }}
            aria-hidden="true"
            onPointerDown={(e) => {
              e.currentTarget.setPointerCapture(e.pointerId);
              move(e);
            }}
            onPointerMove={(e) => {
              if (e.currentTarget.hasPointerCapture(e.pointerId)) move(e);
            }}
          >
            <span style={{ left: `${s * 100}%`, top: `${(1 - v) * 100}%` }} />
          </div>
          <label className="color-control">
            {t("color.hue")}
            <input
              className="hue-slider"
              aria-label={`${label}: ${t("color.hue")}`}
              type="range"
              min="0"
              max="359"
              value={Math.min(359, hue)}
              onChange={(e) => {
                const h = Number(e.target.value);
                choose(hex(h, s, v), h);
              }}
            />
          </label>
          <label className="color-control">
            {t("color.saturation")}
            <input
              aria-label={`${label}: ${t("color.saturation")}`}
              type="range"
              min="0"
              max="100"
              value={Math.round(s * 100)}
              onChange={(e) => choose(hex(hue, Number(e.target.value) / 100, v), hue)}
            />
          </label>
          <label className="color-control">
            {t("color.brightness")}
            <input
              aria-label={`${label}: ${t("color.brightness")}`}
              type="range"
              min="0"
              max="100"
              value={Math.round(v * 100)}
              onChange={(e) => choose(hex(hue, s, Number(e.target.value) / 100), hue)}
            />
          </label>
          <label className="color-control">
            HEX
            <input
              aria-label={`${label}: HEX`}
              spellCheck={false}
              value={draft}
              maxLength={7}
              onChange={(e) => {
                const input = e.target.value;
                setState((previous) => ({ ...previous, draft: input }));
                if (normalizeHex(input)) choose(input);
              }}
              onBlur={resetDraft}
            />
          </label>
          {presets.length > 0 && (
            <div className="color-presets">
              {presets.map((color) => (
                <button
                  type="button"
                  key={color}
                  aria-label={color}
                  title={color}
                  style={{ background: color }}
                  onClick={() => choose(color)}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
