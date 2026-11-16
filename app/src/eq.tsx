// Égaliseur : une courbe de réponse qu'on façonne en tirant trois points (basses, médiums,
// aigus), comme sur un égaliseur de logiciel de musique, plus les préréglages, la réverbération
// et la vitesse. Utilisé dans le panneau de la barre du lecteur et dans la page Réglages.

import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { FLAT_FX, type FxSettings } from "./api";
import { fx, loadFx, patchFx, setFx } from "./fx-store";
import { IconEq } from "./icons";
import { t } from "./i18n";

export const PRESETS: { id: "normal" | "slowed" | "spedUp" | "bass"; fx: FxSettings }[] = [
  { id: "normal", fx: FLAT_FX },
  { id: "slowed", fx: { ...FLAT_FX, reverb: 0.45, speed: 0.85 } },
  { id: "spedUp", fx: { ...FLAT_FX, speed: 1.25 } },
  { id: "bass", fx: { ...FLAT_FX, bass: 8 } },
];

const presetName = (id: (typeof PRESETS)[number]["id"]) =>
  ({ normal: t.presetNormal, slowed: t.presetSlowed, spedUp: t.presetSpedUp, bass: t.presetBass })[id];

const sameFx = (a: FxSettings, b: FxSettings) =>
  (Object.keys(b) as (keyof FxSettings)[]).every((k) => Math.abs(a[k] - b[k]) < 1e-3);

// ---------- Courbe de réponse (mêmes filtres que le moteur audio, voir fx.rs) ----------

const RATE = 48_000;
type Coefs = [number, number, number, number, number, number]; // b0 b1 b2 a0 a1 a2

function shelf(freq: number, db: number, high: boolean): Coefs {
  const a = 10 ** (db / 40);
  const w = (2 * Math.PI * freq) / RATE;
  const cos = Math.cos(w);
  const k = Math.sqrt(a) * Math.sin(w) * Math.SQRT2;
  const s = high ? -1 : 1;
  return [
    a * (a + 1 - s * (a - 1) * cos + k),
    s * 2 * a * (a - 1 - s * (a + 1) * cos),
    a * (a + 1 - s * (a - 1) * cos - k),
    a + 1 + s * (a - 1) * cos + k,
    -s * 2 * (a - 1 + s * (a + 1) * cos),
    a + 1 + s * (a - 1) * cos - k,
  ];
}

function peaking(freq: number, db: number, q: number): Coefs {
  const a = 10 ** (db / 40);
  const w = (2 * Math.PI * freq) / RATE;
  const alpha = Math.sin(w) / (2 * q);
  const cos = Math.cos(w);
  return [1 + alpha * a, -2 * cos, 1 - alpha * a, 1 + alpha / a, -2 * cos, 1 - alpha / a];
}

/** Gain (dB) d'un filtre à la fréquence `f`. */
function gainAt(c: Coefs, f: number): number {
  const w = (2 * Math.PI * f) / RATE;
  const [b0, b1, b2, a0, a1, a2] = c;
  const nr = b0 + b1 * Math.cos(w) + b2 * Math.cos(2 * w);
  const ni = -(b1 * Math.sin(w) + b2 * Math.sin(2 * w));
  const dr = a0 + a1 * Math.cos(w) + a2 * Math.cos(2 * w);
  const di = -(a1 * Math.sin(w) + a2 * Math.sin(2 * w));
  return 10 * Math.log10((nr * nr + ni * ni) / (dr * dr + di * di));
}

const BANDS = [
  { key: "bass", freq: 120 },
  { key: "mid", freq: 1000 },
  { key: "treble", freq: 6000 },
] as const;
type BandKey = (typeof BANDS)[number]["key"];

// Géométrie du graphe (unités du viewBox).
const W = 400;
const H = 168;
const PAD_X = 14;
const PAD_Y = 16;
const MAX_DB = 15;
const xOf = (f: number) => PAD_X + (Math.log10(f / 20) / 3) * (W - 2 * PAD_X);
const yOf = (db: number) => H / 2 - (db / MAX_DB) * (H / 2 - PAD_Y);
const dbOf = (y: number) => ((H / 2 - y) / (H / 2 - PAD_Y)) * MAX_DB;

function curve(s: FxSettings): string {
  const filters = [shelf(120, s.bass, false), peaking(1000, s.mid, 0.8), shelf(6000, s.treble, true)];
  const pts: string[] = [];
  for (let i = 0; i <= 160; i++) {
    const f = 20 * 10 ** ((3 * i) / 160);
    const db = filters.reduce((sum, c) => sum + gainAt(c, f), 0);
    pts.push(`${xOf(f).toFixed(1)},${yOf(Math.max(-MAX_DB, Math.min(MAX_DB, db))).toFixed(1)}`);
  }
  return pts.join(" ");
}

const GRID_F = [50, 100, 200, 500, 1000, 2000, 5000, 10000];
const LABELS = [
  { f: 100, text: "100" },
  { f: 1000, text: "1k" },
  { f: 10000, text: "10k" },
];

function EqGraph() {
  let svg!: SVGSVGElement;
  const [active, setActive] = createSignal<BandKey | null>(null);

  /** Point du graphe sous le pointeur (coordonnées du viewBox). */
  const toView = (e: PointerEvent) => {
    const r = svg.getBoundingClientRect();
    return { x: ((e.clientX - r.left) / r.width) * W, y: ((e.clientY - r.top) / r.height) * H };
  };
  const setGain = (key: BandKey, db: number) => patchFx({ [key]: Math.round(Math.max(-12, Math.min(12, db)) * 2) / 2 });

  const down = (e: PointerEvent) => {
    const { x, y } = toView(e);
    // Le point le plus proche, horizontalement : on peut attraper une bande n'importe où.
    const band = BANDS.reduce((best, b) => (Math.abs(xOf(b.freq) - x) < Math.abs(xOf(best.freq) - x) ? b : best));
    setActive(band.key);
    svg.setPointerCapture(e.pointerId);
    setGain(band.key, dbOf(y));
  };
  const move = (e: PointerEvent) => {
    const key = active();
    if (key && svg.hasPointerCapture(e.pointerId)) setGain(key, dbOf(toView(e).y));
  };
  const up = (e: PointerEvent) => {
    if (svg.hasPointerCapture(e.pointerId)) svg.releasePointerCapture(e.pointerId);
    setActive(null);
  };

  const label = (key: BandKey) => ({ bass: t.bass, mid: t.mid, treble: t.treble })[key];
  const dbText = (v: number) => `${v > 0 ? "+" : ""}${v.toFixed(1)} dB`;

  return (
    <svg
      ref={svg}
      class="eq-graph"
      viewBox={`0 0 ${W} ${H}`}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
    >
      <defs>
        <linearGradient id="eq-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stop-color="var(--md-primary)" stop-opacity="0.35" />
          <stop offset="1" stop-color="var(--md-primary)" stop-opacity="0.02" />
        </linearGradient>
      </defs>
      <For each={GRID_F}>{(f) => <line class="eq-grid" x1={xOf(f)} x2={xOf(f)} y1={PAD_Y / 2} y2={H - PAD_Y / 2} />}</For>
      <For each={[-12, -6, 6, 12]}>{(db) => <line class="eq-grid" x1={PAD_X} x2={W - PAD_X} y1={yOf(db)} y2={yOf(db)} />}</For>
      <line class="eq-zero" x1={PAD_X} x2={W - PAD_X} y1={yOf(0)} y2={yOf(0)} />
      <For each={LABELS}>{(l) => <text class="eq-label" x={xOf(l.f) + 4} y={H - 5}>{l.text}</text>}</For>
      <text class="eq-label" x={W - PAD_X - 2} y={yOf(12) - 3} text-anchor="end">+12</text>
      <text class="eq-label" x={W - PAD_X - 2} y={yOf(-12) + 11} text-anchor="end">-12</text>

      <polygon class="eq-area" points={`${xOf(20)},${yOf(0)} ${curve(fx())} ${xOf(20000)},${yOf(0)}`} fill="url(#eq-fill)" />
      <polyline class="eq-curve" points={curve(fx())} />

      <For each={BANDS}>
        {(b) => {
          const v = () => fx()[b.key];
          return (
            <g
              class="eq-node"
              classList={{ on: active() === b.key }}
              tabindex="0"
              role="slider"
              aria-label={label(b.key)}
              aria-valuemin={-12}
              aria-valuemax={12}
              aria-valuenow={v()}
              aria-valuetext={dbText(v())}
              onKeyDown={(e) => {
                const step = { ArrowUp: 0.5, ArrowRight: 0.5, ArrowDown: -0.5, ArrowLeft: -0.5 }[e.key];
                if (step) {
                  e.preventDefault();
                  setGain(b.key, v() + step);
                }
              }}
              onDblClick={() => setGain(b.key, 0)}
            >
              <circle class="eq-node-halo" cx={xOf(b.freq)} cy={yOf(v())} r="14" />
              <circle class="eq-node-dot" cx={xOf(b.freq)} cy={yOf(v())} r="6.5" />
              <Show when={active() === b.key}>
                <text class="eq-value" x={xOf(b.freq)} y={yOf(v()) + (v() > 6 ? 30 : -20)} text-anchor="middle">
                  {label(b.key)} {dbText(v())}
                </text>
              </Show>
            </g>
          );
        }}
      </For>
    </svg>
  );
}

function FxRange(props: { label: string; min: number; max: number; step: number; value: number; text: string; onInput: (v: number) => void }) {
  return (
    <label class="eq-range">
      <span class="eq-range-label">{props.label}</span>
      <input
        type="range"
        class="slider"
        min={props.min}
        max={props.max}
        step={props.step}
        value={props.value}
        style={{ "--pct": `${((props.value - props.min) / (props.max - props.min)) * 100}%` }}
        onInput={(e) => props.onInput(Number(e.currentTarget.value))}
      />
      <span class="eq-range-value">{props.text}</span>
    </label>
  );
}

/** Tout l'égaliseur : préréglages, courbe, réverbération et vitesse. */
export function EqPanel() {
  onMount(loadFx);
  return (
    <div class="eq-panel">
      <div class="eq-presets" role="radiogroup" aria-label={t.effects}>
        <For each={PRESETS}>
          {(p) => (
            <button
              role="radio"
              aria-checked={sameFx(fx(), p.fx)}
              classList={{ "eq-chip": true, on: sameFx(fx(), p.fx) }}
              onClick={() => setFx(p.fx)}
            >
              {presetName(p.id)}
            </button>
          )}
        </For>
      </div>
      <EqGraph />
      <p class="eq-hint">{t.eqHint}</p>
      <FxRange
        label={t.reverb}
        min={0}
        max={100}
        step={5}
        value={Math.round(fx().reverb * 100)}
        text={fx().reverb === 0 ? t.off : `${Math.round(fx().reverb * 100)} %`}
        onInput={(v) => patchFx({ reverb: v / 100 })}
      />
      <FxRange
        label={t.speed}
        min={50}
        max={150}
        step={5}
        value={Math.round(fx().speed * 100)}
        text={`${fx().speed.toFixed(2)}×`}
        onInput={(v) => patchFx({ speed: v / 100 })}
      />
    </div>
  );
}

/** Bouton de la barre du lecteur qui ouvre l'égaliseur au-dessus d'elle. */
export function EqButton() {
  const [open, setOpen] = createSignal(false);
  let root!: HTMLDivElement;
  onMount(() => {
    loadFx();
    const outside = (e: PointerEvent) => open() && !root.contains(e.target as Node) && setOpen(false);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && open() && setOpen(false);
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", esc);
    onCleanup(() => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", esc);
    });
  });
  const changed = () => !sameFx(fx(), FLAT_FX);
  return (
    <div class="eq-pop" ref={root}>
      <button
        classList={{ "icon-btn": true, active: open() || changed() }}
        title={t.equalizer}
        aria-expanded={open()}
        onClick={() => setOpen(!open())}
      >
        <IconEq />
      </button>
      <Show when={open()}>
        <div class="eq-popover" role="dialog" aria-label={t.equalizer}>
          <div class="eq-head">
            <span class="eq-title">{t.equalizer}</span>
            <button class="eq-reset" onClick={() => setFx(FLAT_FX)} disabled={!changed()}>
              {t.eqReset}
            </button>
          </div>
          <EqPanel />
        </div>
      </Show>
    </div>
  );
}
