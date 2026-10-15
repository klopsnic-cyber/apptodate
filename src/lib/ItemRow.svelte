<script>
  import { ArrowRight, MoreHorizontal, EyeOff, Eye, Check, Loader2, AlertTriangle, HelpCircle, Terminal, Monitor } from "@lucide/svelte";
  import { SEV, hue } from "$lib/api.js";

  let { item, task = null, ignored = false, onaction, onignore, ontoggle } = $props();
  let menu = $state(false);

  let primary = $derived(item.actions[0]);
  let extra = $derived(item.actions.slice(1));
  let sev = $derived(item.severity && SEV[item.severity]);
  let initials = $derived(item.name.replace(/[^A-Za-z0-9 ]/g, "").split(" ").filter(Boolean).slice(0, 2).map((w) => w[0]).join("").toUpperCase() || "?");
  let running = $derived(task?.state === "running" || task?.state === "queued");
</script>

<svelte:window onclick={() => (menu = false)} />

<div class="row" class:dim={ignored}>
  <div class="icon">
    {#if item.icon}
      <img src={item.icon} alt="" />
    {:else if item.kind === "formula"}
      <span class="glyph"><Terminal size={20} /></span>
    {:else if item.kind === "macos"}
      <span class="glyph"><Monitor size={20} /></span>
    {:else}
      <span class="letters" style="--h:{hue(item.name)}">{initials}</span>
    {/if}
  </div>

  <div class="main">
    <div class="title">
      <b>{item.name}</b>
      {#if sev}<span class="sev" style="color:{sev.color};background:{sev.bg}">{sev.label}</span>{/if}
      {#if item.category}<span class="tag">{item.category}</span>{/if}
      <span class="tag src">{item.source}</span>
      {#if item.uncertain}<span class="tag warn" title="Nur über den Namen zugeordnet"><HelpCircle size={11} /> unsicher</span>{/if}
    </div>
    <div class="ver num">
      {#if task?.state === "ok"}
        <span class="done"><Check size={13} /> aktualisiert</span>
      {:else if item.status === "update"}
        <span>{item.installed || "?"}</span><ArrowRight size={12} /><b>{item.latest}</b>
      {:else if item.status === "current"}
        <span>{item.installed}</span> <span class="ok"><Check size={12} /> aktuell</span>
      {:else}
        <span>{item.installed || "Version unbekannt"}</span>
        {#if item.latest}<span class="muted">· Katalog: {item.latest}</span>{/if}
      {/if}
    </div>
    {#if item.reasons.length}
      <div class="why muted">{item.reasons.join(" ")}</div>
    {/if}
    {#if task}
      <button class="log-link" onclick={() => ontoggle?.(item.id)}>
        {#if running}<Loader2 size={12} class="spin" /> {task.state === "queued" ? "Wartet …" : task.lines.at(-1) || "Läuft …"}
        {:else if task.state === "ok"}<Check size={12} /> Fertig – Protokoll anzeigen
        {:else}<AlertTriangle size={12} /> Fehlgeschlagen – Protokoll anzeigen{/if}
      </button>
    {/if}
  </div>

  <div class="acts">
    {#if primary && task?.state !== "ok"}
      <button class={item.status === "update" ? "primary small" : "small"} disabled={running} onclick={() => onaction(item, 0)}>
        {#if running}<Loader2 size={13} class="spin" />{/if}{primary.label}
      </button>
    {/if}
    <div class="more">
      <button class="ghost small icon-btn" aria-label="Weitere Aktionen" onclick={(e) => { e.stopPropagation(); menu = !menu; }}>
        <MoreHorizontal size={16} />
      </button>
      {#if menu}
        <div class="menu card" role="menu">
          {#each extra as a, i}
            <button class="ghost" onclick={() => onaction(item, i + 1)}>{a.label}</button>
          {/each}
          <button class="ghost" onclick={() => onignore(item)}>
            {#if ignored}<Eye size={14} /> Wieder anzeigen{:else}<EyeOff size={14} /> Ignorieren{/if}
          </button>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .row {
    display: grid; grid-template-columns: 44px 1fr auto; gap: 14px; align-items: center;
    padding: 12px 14px; border-radius: 14px; transition: background 0.2s;
  }
  .row:hover { background: var(--surface-2); }
  .dim { opacity: 0.55; }
  .icon { width: 44px; height: 44px; display: grid; place-items: center; }
  .icon img { width: 40px; height: 40px; filter: drop-shadow(0 2px 4px rgba(0, 0, 0, 0.12)); }
  .letters, .glyph {
    width: 38px; height: 38px; border-radius: 10px; display: grid; place-items: center; font-weight: 650; font-size: 14px; color: #fff;
  }
  .letters { background: linear-gradient(135deg, hsl(var(--h) 70% 58%), hsl(calc(var(--h) + 40) 70% 48%)); }
  .glyph { background: var(--surface-2); color: var(--muted); border: 1px solid var(--border); }
  .main { min-width: 0; }
  .title { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; }
  .title b { font-weight: 600; font-size: 14.5px; }
  .sev, .tag { font-size: 11px; font-weight: 600; padding: 2px 8px; border-radius: 99px; display: inline-flex; align-items: center; gap: 3px; }
  .tag { background: var(--surface-2); color: var(--muted); font-weight: 550; }
  .tag.warn { color: var(--amber); background: var(--amber-bg); }
  .ver { display: flex; align-items: center; gap: 6px; margin-top: 3px; font-size: 13px; color: var(--muted); }
  .ver b { color: var(--text); font-weight: 600; }
  .ok, .done { color: var(--green); display: inline-flex; align-items: center; gap: 3px; font-weight: 550; }
  .why { font-size: 12px; margin-top: 3px; line-height: 1.4; max-width: 680px; }
  .log-link {
    margin-top: 6px; padding: 3px 8px; font-size: 11.5px; font-weight: 500; border-radius: 7px; background: var(--surface-2);
    border: none; color: var(--muted); max-width: 520px; overflow: hidden; text-overflow: ellipsis; display: inline-flex;
  }
  .acts { display: flex; align-items: center; gap: 4px; }
  .more { position: relative; }
  .icon-btn { padding: 5px 7px; }
  .menu {
    position: absolute; right: 0; top: calc(100% + 4px); z-index: 20; min-width: 200px; padding: 6px; display: flex; flex-direction: column;
    box-shadow: var(--shadow-lg); animation: fade 0.15s var(--ease);
  }
  .menu button { justify-content: flex-start; font-weight: 500; padding: 7px 10px; }
  @keyframes fade { from { opacity: 0; transform: translateY(-4px); } }
  :global(.spin) { animation: rot 1s linear infinite; }
  @keyframes rot { to { transform: rotate(360deg); } }
</style>
