// A small force-directed layout for the graph view: linked nodes pull
// together, all nodes push apart. Deterministic, so the picture doesn't
// shuffle each time it is drawn.

export interface LayoutEdge {
  a: string;
  b: string;
}

export function forceLayout(ids: string[], edges: LayoutEdge[]): Map<string, { x: number; y: number }> {
  const n = ids.length;
  const index = new Map(ids.map((id, i) => [id, i]));
  const pos = ids.map((_, i) => {
    const ang = (i / Math.max(1, n)) * Math.PI * 2;
    const r = 60 * Math.sqrt(n) + 40;
    return { x: Math.cos(ang) * r, y: Math.sin(ang) * r };
  });
  const links = edges.map((e) => [index.get(e.a), index.get(e.b)] as const).filter((p): p is readonly [number, number] => p[0] !== undefined && p[1] !== undefined && p[0] !== p[1]);
  const k = 150;
  const iterations = n > 400 ? 80 : 220;
  let temp = k * 2;
  for (let it = 0; it < iterations; it += 1) {
    const disp = pos.map(() => ({ x: 0, y: 0 }));
    for (let i = 0; i < n; i += 1) {
      for (let j = i + 1; j < n; j += 1) {
        let dx = pos[i]!.x - pos[j]!.x;
        let dy = pos[i]!.y - pos[j]!.y;
        let d = Math.hypot(dx, dy);
        if (d < 0.01) {
          dx = (i % 7) - 3 || 1;
          dy = (j % 5) - 2 || 1;
          d = Math.hypot(dx, dy);
        }
        const f = (k * k) / d;
        disp[i]!.x += (dx / d) * f;
        disp[i]!.y += (dy / d) * f;
        disp[j]!.x -= (dx / d) * f;
        disp[j]!.y -= (dy / d) * f;
      }
    }
    for (const [a, b] of links) {
      const dx = pos[a]!.x - pos[b]!.x;
      const dy = pos[a]!.y - pos[b]!.y;
      const d = Math.max(0.01, Math.hypot(dx, dy));
      const f = (d * d) / k;
      disp[a]!.x -= (dx / d) * f;
      disp[a]!.y -= (dy / d) * f;
      disp[b]!.x += (dx / d) * f;
      disp[b]!.y += (dy / d) * f;
    }
    for (let i = 0; i < n; i += 1) {
      // A weak pull to the middle keeps separate clusters on screen.
      disp[i]!.x -= pos[i]!.x * 0.4;
      disp[i]!.y -= pos[i]!.y * 0.4;
      const d = Math.max(0.01, Math.hypot(disp[i]!.x, disp[i]!.y));
      const step = Math.min(d, temp);
      pos[i]!.x += (disp[i]!.x / d) * step;
      pos[i]!.y += (disp[i]!.y / d) * step;
    }
    temp = Math.max(2, temp * 0.97);
  }
  return new Map(ids.map((id, i) => [id, pos[i]!]));
}
