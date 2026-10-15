<script>
  // Radar: dreht sich während der Prüfung, danach erscheint jedes Update als Punkt
  // (näher an der Mitte = dringender, Farbe = Dringlichkeit).
  let { items = [], scanning = false, size = 190 } = $props();

  const R = 90;
  const ring = { wichtig: 0.38, empfohlen: 0.62, optional: 0.86 };
  const color = { wichtig: "var(--red)", empfohlen: "var(--amber)", optional: "var(--blue)" };

  // Goldener Winkel: verteilt die Punkte gleichmäßig über den Kreis
  const GOLDEN = Math.PI * (3 - Math.sqrt(5));
  let dots = $derived(
    items.map((it, i) => {
      const a = i * GOLDEN - Math.PI / 3;
      const r = R * ring[it.severity] * (0.92 + ((i * 37) % 17) / 100);
      return { x: Math.cos(a) * r, y: Math.sin(a) * r, c: color[it.severity], d: (i % 12) * 0.06, name: it.name };
    }),
  );
</script>

<svg viewBox="-100 -100 200 200" width={size} height={size} class:scanning aria-hidden="true">
  <defs>
    <radialGradient id="bgr" cx="0" cy="0" r="100" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="var(--accent)" stop-opacity="0.16" />
      <stop offset="1" stop-color="var(--accent)" stop-opacity="0.02" />
    </radialGradient>
    <linearGradient id="sweep" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="#22c1c3" stop-opacity="0" />
      <stop offset="1" stop-color="#22c1c3" stop-opacity="0.55" />
    </linearGradient>
  </defs>
  <circle r={R} fill="url(#bgr)" stroke="var(--border)" />
  {#each [0.38, 0.62, 0.86] as f}
    <circle r={R * f} fill="none" stroke="var(--accent)" stroke-opacity="0.16" stroke-dasharray="2 4" />
  {/each}
  <line x1={-R} x2={R} y1="0" y2="0" stroke="var(--accent)" stroke-opacity="0.1" />
  <line y1={-R} y2={R} x1="0" x2="0" stroke="var(--accent)" stroke-opacity="0.1" />
  <g class="beam">
    <path d="M0 0 L{R} 0 A{R} {R} 0 0 0 {R * Math.cos(-0.9)} {R * Math.sin(-0.9)} Z" fill="url(#sweep)" />
    <line x1="0" y1="0" x2={R} y2="0" stroke="#22c1c3" stroke-width="1.6" stroke-linecap="round" />
  </g>
  {#if !scanning}
    {#each dots as d}
      <g class="dot" style="animation-delay:{d.d}s">
        <circle cx={d.x} cy={d.y} r="7" fill={d.c} opacity="0.18" class="halo" />
        <circle cx={d.x} cy={d.y} r="3.4" fill={d.c}><title>{d.name}</title></circle>
      </g>
    {/each}
  {/if}
  <circle r="4" fill="var(--accent)" />
</svg>

<style>
  svg { overflow: visible; display: block; }
  .beam { transform-origin: 0 0; opacity: 0; transition: opacity 0.4s; }
  .scanning .beam { opacity: 1; animation: spin 2.4s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .dot { animation: pop 0.6s var(--ease) both; transform-box: fill-box; transform-origin: center; }
  .halo { animation: pulse 2.6s ease-in-out infinite; transform-box: fill-box; transform-origin: center; }
  @keyframes pop { from { opacity: 0; transform: scale(0.2); } }
  @keyframes pulse { 50% { transform: scale(1.6); opacity: 0.05; } }
  @media (prefers-reduced-motion: reduce) { .scanning .beam, .halo { animation: none; } }
</style>
