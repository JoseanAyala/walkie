// The overlay's label decoding in from glyph noise: each frame settles a
// little more of the text, left to right. Pure, so it stays out of the component.

/** Block glyphs from the spinner's family: the noise a label resolves from. */
export const NOISE = "░▒▓▖▘▝▗█";
/** Frames to settle the whole text, FRAME_MS apart (about 250ms). */
export const FRAMES = 5;
export const FRAME_MS = 50;

/** `text` at `frame` of its decode: the first frame/FRAMES of it settled,
 *  the rest noise. Spaces stay spaces, so the words keep their shape. */
export function scramble(text: string, frame: number, rand: () => number = Math.random): string {
  if (frame >= FRAMES) return text;
  const chars = Array.from(text);
  const settled = Math.floor((Math.max(0, frame) / FRAMES) * chars.length);
  return chars
    .map((c, i) =>
      i < settled || c === " " ? c : NOISE[Math.floor(rand() * NOISE.length) % NOISE.length],
    )
    .join("");
}
