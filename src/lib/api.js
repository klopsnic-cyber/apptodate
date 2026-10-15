import { invoke } from "@tauri-apps/api/core";

export const api = {
  startScan: (force = false) => invoke("start_scan", { force }),
  runAction: (id, index) => invoke("run_action", { id, index }),
  upgradeAllFormulae: () => invoke("upgrade_all_formulae"),
};

export const SEV = {
  wichtig: { label: "Wichtig", color: "var(--red)", bg: "var(--red-bg)" },
  empfohlen: { label: "Empfohlen", color: "var(--amber)", bg: "var(--amber-bg)" },
  optional: { label: "Optional", color: "var(--blue)", bg: "var(--blue-bg)" },
};
export const SEV_ORDER = { wichtig: 0, empfohlen: 1, optional: 2 };

/** Ignorierte Einträge – nur lokal, bequem; fehlt der Speicher, geht nichts verloren. */
export const store = {
  get(k, d) {
    try { return JSON.parse(localStorage.getItem(k)) ?? d; } catch { return d; }
  },
  set(k, v) {
    try { localStorage.setItem(k, JSON.stringify(v)); } catch {}
  },
};

export function ago(ts) {
  if (!ts) return "";
  const m = Math.round((Date.now() - ts) / 60000);
  if (m < 1) return "gerade eben";
  if (m < 60) return `vor ${m} Min.`;
  const h = Math.round(m / 60);
  return h < 24 ? `vor ${h} Std.` : new Date(ts).toLocaleDateString("de-DE");
}

/** Farbe für Buchstaben-Symbol, wenn eine App kein eigenes Icon hat */
export function hue(s) {
  let h = 0;
  for (const c of s) h = (h * 31 + c.charCodeAt(0)) % 360;
  return h;
}
