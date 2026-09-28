// Draws the dithered desktop into <canvas id="desk">: a faint dot grid plus
// a few soft "dust clouds" of pixels. Seeded, so a window looks the same
// every time it opens; redrawn on resize and when macOS switches theme.
(() => {
  const c = document.getElementById('desk');
  if (!c) return;
  const seed = Number(c.dataset.seed || 7);
  // [x, y, radius] as fractions of the window, radius of its larger side
  const clouds = JSON.parse(c.dataset.clouds || '[[0.12,0.85,0.35],[0.92,0.45,0.3],[0.5,1.05,0.3]]');

  function rng(a) {
    return () => {
      a |= 0; a = a + 0x6D2B79F5 | 0;
      let t = Math.imul(a ^ a >>> 15, 1 | a);
      t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t;
      return ((t ^ t >>> 14) >>> 0) / 4294967296;
    };
  }

  function draw() {
    const w = innerWidth, h = innerHeight, px = devicePixelRatio || 1;
    c.width = w * px; c.height = h * px;
    c.style.width = w + 'px'; c.style.height = h + 'px';
    const g = c.getContext('2d');
    g.setTransform(px, 0, 0, px, 0, 0);
    const css = getComputedStyle(document.documentElement);
    g.fillStyle = css.getPropertyValue('--desk').trim();
    g.fillRect(0, 0, w, h);
    const dot = css.getPropertyValue('--desk-dot').trim();
    g.fillStyle = dot;

    g.globalAlpha = 0.28;
    for (let y = 2; y < h; y += 6) for (let x = 2; x < w; x += 6) g.fillRect(x, y, 1, 1);

    g.globalAlpha = 1;
    const r = rng(seed), big = Math.max(w, h), step = 3;
    for (let y = 0; y < h; y += step) {
      for (let x = 0; x < w; x += step) {
        let p = 0;
        for (const [cx, cy, cr] of clouds) {
          const d = Math.hypot(x - cx * w, y - cy * h) / (cr * big);
          if (d < 1) p = Math.max(p, (1 - d) ** 1.6);
        }
        if (p > 0 && r() < p * 0.75) g.fillRect(x, y, 2, 2);
      }
    }
  }

  let t;
  addEventListener('resize', () => { clearTimeout(t); t = setTimeout(draw, 80); });
  matchMedia('(prefers-color-scheme: dark)').addEventListener('change', draw);
  document.fonts.ready.then(draw);
  draw();
})();
