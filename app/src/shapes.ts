// Formes de Material 3 Expressive : des cercles festonnés (« cookie ») et un carré arrondi.
//
// Toutes les formes sont des polygones avec le même nombre de points, pris aux mêmes angles :
// le navigateur sait alors animer `clip-path` de l'une à l'autre (chaque point glisse vers son
// homologue), ce qui donne une vraie transformation de forme.

/** Nombre de points de chaque forme (identique partout pour pouvoir passer de l'une à l'autre). */
const POINTS = 144;

/** Polygone CSS à partir d'un rayon (en %) pour chaque angle. */
function polygon(radius: (angle: number) => number): string {
  const coords: string[] = [];
  for (let i = 0; i < POINTS; i++) {
    const a = (i / POINTS) * Math.PI * 2;
    const r = radius(a);
    coords.push(`${(50 + r * Math.cos(a)).toFixed(2)}% ${(50 + r * Math.sin(a)).toFixed(2)}%`);
  }
  return `polygon(${coords.join(", ")})`;
}

/**
 * Cercle à `petals` festons ; `depth` : profondeur des creux (fraction du rayon). `rotation` en
 * degrés : la forme tourne, les points restent aux mêmes angles (pour l'animation).
 */
export function cookie(petals: number, depth: number, rotation = 0): string {
  const offset = (rotation * Math.PI) / 180;
  return polygon((a) => {
    // Onde adoucie : festons ronds, creux nets mais pas pointus.
    const wave = (Math.cos(petals * (a - offset)) + 1) / 2;
    return 50 * (1 - depth + depth * Math.pow(wave, 0.7));
  });
}

/**
 * Carré aux coins arrondis, exactement comme `border-radius` : `corner` = rayon des coins en %
 * du côté. Sert de forme d'arrivée : le relais avec le vrai `border-radius` est alors invisible.
 */
export function roundedSquare(corner: number): string {
  const c = Math.min(50, Math.max(0, corner));
  const inner = 50 - c;
  // Distance d'un point au bord du carré arrondi (négative à l'intérieur).
  const edge = (x: number, y: number) => Math.hypot(Math.max(Math.abs(x) - inner, 0), Math.max(Math.abs(y) - inner, 0)) - c;
  return polygon((a) => {
    const dx = Math.cos(a);
    const dy = Math.sin(a);
    // Rayon où la demi-droite partie du centre croise le bord (dichotomie).
    let lo = 0;
    let hi = 75;
    for (let i = 0; i < 40; i++) {
      const mid = (lo + hi) / 2;
      if (edge(mid * dx, mid * dy) > 0) hi = mid;
      else lo = mid;
    }
    return lo;
  });
}

/** Forme de la pochette du lecteur (et ses paramètres, réutilisés pour l'animation). */
export const PLAYER_COOKIE = { petals: 9, depth: 0.1 };

export function installShapes() {
  const root = document.documentElement.style;
  root.setProperty("--cookie-9", cookie(PLAYER_COOKIE.petals, PLAYER_COOKIE.depth));
  root.setProperty("--cookie-12", cookie(12, 0.07));
  root.setProperty("--cookie-6", cookie(6, 0.12));
}
