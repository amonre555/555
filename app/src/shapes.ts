// Carré aux coins arrondis, en polygone : utilisé pour le vol de la pochette entre le lecteur et
// la vue « En lecture ».
//
// Les formes ont toutes le même nombre de points, pris aux mêmes angles : le navigateur sait
// alors animer `clip-path` de l'une à l'autre (chaque point glisse vers son homologue).

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
