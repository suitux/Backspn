function rng(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 2 ** 32;
  };
}

export interface Bar {
  x: number;
  low: number;
  high: number;
}

export function bars(seed: number, n: number): Bar[] {
  const r = rng(seed);
  const out: Bar[] = [];
  for (let i = 0; i < n; i++) {
    const beat = i % 8 === 0 ? 1 : i % 8 === 1 ? 0.8 : 0.45;
    const phrase = Math.floor(i / 128) % 4 === 3 ? 0.35 : 1;
    const low = Math.min(1, (0.25 + 0.6 * beat * phrase) * (0.85 + 0.3 * r()));
    const high = Math.min(1, low * (0.35 + 0.35 * r()) + 0.12 * r());
    out.push({ x: i, low, high });
  }
  return out;
}

export function path(list: Bar[], h: number, key: 'low' | 'high', w = 2): string {
  const mid = h / 2;
  return list
    .map((b) => {
      const a = (b[key] * h) / 2;
      return `M${b.x * (w + 1)} ${(mid - a).toFixed(1)}h${w}v${(2 * a).toFixed(1)}h-${w}z`;
    })
    .join('');
}
