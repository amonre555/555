// Visualiseur du Now playing : des halos de lumière aux couleurs de la pochette qui respirent
// avec la musique, dessinés par un shader WebGL (la carte graphique calcule chaque pixel).
//
// Le son est analysé côté Rust (basses, médiums, aigus) ; on lit la mesure à chaque image.

import { createSignal, onCleanup, onMount } from "solid-js";
import { api } from "./api";

const KEY = "yato.visualizer";

function readPref(): boolean {
  try {
    return localStorage.getItem(KEY) !== "off";
  } catch {
    return true;
  }
}

const [visualizerOn, setOn] = createSignal(readPref());
export { visualizerOn };

export function setVisualizerOn(on: boolean) {
  setOn(on);
  try {
    localStorage.setItem(KEY, on ? "on" : "off");
  } catch {}
}

const VERT = `attribute vec2 p; void main() { gl_Position = vec4(p, 0.0, 1.0); }`;

// Trois halos qui dérivent lentement : le premier gonfle avec les basses, le deuxième avec les
// médiums, le troisième avec les aigus. Une onde fine en bas suit le mouvement de l'ensemble.
const FRAG = `
precision mediump float;
uniform vec2 res;
uniform float time, bass, mid, high, all;
uniform vec3 c1, c2;

float halo(vec2 p, vec2 c, float r) { vec2 d = p - c; return exp(-dot(d, d) / (r * r)); }

void main() {
  vec2 p = (gl_FragCoord.xy - 0.5 * res) / res.y;
  float t = time * 0.12;
  vec2 a = vec2(sin(t * 1.3) * 0.55, cos(t) * 0.28 + 0.05);
  vec2 b = vec2(cos(t * 0.9 + 1.0) * 0.65, sin(t * 1.1 + 2.0) * 0.32);
  vec2 c = vec2(sin(t * 0.7 + 4.0) * 0.45, cos(t * 1.4 + 3.0) * 0.36);
  float g1 = halo(p, a, 0.32 + 0.38 * bass);
  float g2 = halo(p, b, 0.26 + 0.26 * mid);
  float g3 = halo(p, c, 0.16 + 0.24 * high);
  float w = sin(p.x * 5.0 + time * 1.4) * 0.05 * (0.3 + mid) + sin(p.x * 12.0 - time * 2.2) * 0.025 * (0.2 + high);
  float line = exp(-pow((p.y + 0.36 - w) * 22.0, 2.0)) * (0.15 + 0.9 * all);
  // Couleurs profondes et intensité contenue : les halos restent derrière le texte.
  vec3 col = c1 * g1 * (0.2 + 0.7 * bass)
           + c2 * g2 * (0.15 + 0.55 * mid)
           + mix(c1, c2, 0.5) * g3 * (0.1 + 0.6 * high)
           + c1 * line * 0.35;
  col = min(col, vec3(0.55));
  gl_FragColor = vec4(col, 1.0);
}`;

/**
 * Couleur d'une variable du thème, en composantes 0–1. Les variables déclarées avec `@property`
 * (`<color>`) sont rendues en `rgb(r, g, b)` par le navigateur, les autres restent en `#rrggbb`.
 */
function cssColor(name: string, fallback: [number, number, number]): [number, number, number] {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const hex = /^#([0-9a-f]{6})$/i.exec(v);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return [(n >> 16) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
  }
  const rgb = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)/i.exec(v);
  if (rgb) return [Number(rgb[1]) / 255, Number(rgb[2]) / 255, Number(rgb[3]) / 255];
  return fallback;
}

export function Visualizer(props: { playing: boolean }) {
  let canvas!: HTMLCanvasElement;

  onMount(() => {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const gl = canvas.getContext("webgl", { alpha: false, antialias: false, premultipliedAlpha: false });
    if (!gl) return;

    const shader = (type: number, src: string) => {
      const s = gl.createShader(type)!;
      gl.shaderSource(s, src);
      gl.compileShader(s);
      return s;
    };
    const prog = gl.createProgram()!;
    gl.attachShader(prog, shader(gl.VERTEX_SHADER, VERT));
    gl.attachShader(prog, shader(gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) return;
    gl.useProgram(prog);

    gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(prog, "p");
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
    const u = (n: string) => gl.getUniformLocation(prog, n);
    const [uRes, uTime, uBass, uMid, uHigh, uAll, uC1, uC2] = ["res", "time", "bass", "mid", "high", "all", "c1", "c2"].map(u);

    // Demi-résolution : des halos flous n'ont pas besoin de plus, et la carte graphique respire.
    const resize = () => {
      const scale = Math.min(devicePixelRatio, 2) * 0.5;
      canvas.width = Math.max(1, Math.round(canvas.clientWidth * scale));
      canvas.height = Math.max(1, Math.round(canvas.clientHeight * scale));
      gl.viewport(0, 0, canvas.width, canvas.height);
    };
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);
    resize();

    const smooth = [0, 0, 0, 0];
    let target = [0, 0, 0, 0];
    let asking = false;
    let frame = 0;
    // Teintes « container » du thème : saturées mais sombres (les teintes claires blanchissaient tout).
    let colors = { c1: cssColor("--md-primary-container", [0.14, 0.2, 1]), c2: cssColor("--md-tertiary-container", [0.05, 0.08, 0.66]) };
    let raf = 0;
    const t0 = performance.now();

    const draw = (now: number) => {
      raf = requestAnimationFrame(draw);
      if (props.playing && !asking) {
        asking = true;
        api
          .audioLevels()
          .then((l) => (target = l))
          .catch(() => {})
          .finally(() => (asking = false));
      }
      const goal = props.playing ? target : [0, 0, 0, 0];
      for (let i = 0; i < 4; i++) {
        // Monte vite, redescend doucement : les coups de basse claquent, sans clignoter.
        const k = goal[i] > smooth[i] ? 0.45 : 0.08;
        smooth[i] += (goal[i] - smooth[i]) * k;
      }
      // Les couleurs suivent la pochette (relues de temps en temps, c'est peu coûteux).
      if (++frame % 30 === 0) {
        colors = { c1: cssColor("--md-primary-container", colors.c1), c2: cssColor("--md-tertiary-container", colors.c2) };
      }
      gl.uniform2f(uRes, canvas.width, canvas.height);
      gl.uniform1f(uTime, (now - t0) / 1000);
      gl.uniform1f(uBass, smooth[0]);
      gl.uniform1f(uMid, smooth[1]);
      gl.uniform1f(uHigh, smooth[2]);
      gl.uniform1f(uAll, smooth[3]);
      gl.uniform3fv(uC1, colors.c1);
      gl.uniform3fv(uC2, colors.c2);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    };
    raf = requestAnimationFrame(draw);

    onCleanup(() => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    });
  });

  return <canvas ref={canvas} class="np-visualizer" aria-hidden="true" />;
}
