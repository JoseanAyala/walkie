export interface DitherOptions {
  seed: number;
  /** [x, y, radius] as fractions of the canvas; radius of its larger side */
  clouds: [number, number, number][];
  desk: string;
  dot: string;
}

/** mulberry32: a tiny seeded PRNG, so the pattern is stable across opens. */
export function rng(seed: number): () => number {
  let a = seed;
  return () => {
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** How likely a pixel at (x, y) is to be set: 0 outside every cloud. */
export function density(
  x: number,
  y: number,
  w: number,
  h: number,
  clouds: DitherOptions["clouds"],
) {
  const big = Math.max(w, h);
  let p = 0;
  for (const [cx, cy, cr] of clouds) {
    const d = Math.hypot(x - cx * w, y - cy * h) / (cr * big);
    if (d < 1) p = Math.max(p, (1 - d) ** 1.6);
  }
  return p;
}

export function dither(c: HTMLCanvasElement, o: DitherOptions) {
  const w = innerWidth;
  const h = innerHeight;
  const px = devicePixelRatio || 1;
  c.width = w * px;
  c.height = h * px;
  c.style.width = `${w}px`;
  c.style.height = `${h}px`;
  const g = c.getContext("2d");
  if (!g) return;
  g.setTransform(px, 0, 0, px, 0, 0);
  g.fillStyle = o.desk;
  g.fillRect(0, 0, w, h);
  g.fillStyle = o.dot;

  g.globalAlpha = 0.28;
  for (let y = 2; y < h; y += 6) for (let x = 2; x < w; x += 6) g.fillRect(x, y, 1, 1);

  g.globalAlpha = 1;
  const r = rng(o.seed);
  for (let y = 0; y < h; y += 3) {
    for (let x = 0; x < w; x += 3) {
      const p = density(x, y, w, h, o.clouds);
      if (p > 0 && r() < p * 0.75) g.fillRect(x, y, 2, 2);
    }
  }
}
