<script>
  import "$lib/theme.css";
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { getVersion } from "@tauri-apps/api/app";
  import { RefreshCw, Download, X, Search, ShieldAlert, Sparkles, Terminal, CheckCircle2, HelpCircle, EyeOff, Zap, Info } from "@lucide/svelte";
  import Radar from "$lib/Radar.svelte";
  import ItemRow from "$lib/ItemRow.svelte";
  import { api, SEV, SEV_ORDER, store, ago } from "$lib/api.js";

  let items = $state([]);
  let meta = $state(null);
  let scanning = $state(false);
  let progress = $state(null);
  let tab = $state("updates");
  let query = $state("");
  let ignored = $state(new Set(store.get("ignored", [])));
  let tasks = $state({}); // id → { name, state: queued|running|ok|error, lines }
  let logOpen = $state(null);
  let confirm = $state(null); // { title, text, list, manual, run }
  let scannedAt = $state(0);
  let now = $state(Date.now());
  let version = $state("");
  let appUpdate = $state(null);
  let appUpdating = $state(false);

  onMount(() => {
    const log = (m) => invoke("frontend_log", { msg: String(m) }).catch(() => {});
    window.addEventListener("error", (e) => log(`${e.message} @ ${e.filename}:${e.lineno}`));
    window.addEventListener("unhandledrejection", (e) => log(`Promise: ${e.reason?.stack || e.reason}`));
    getVersion().then((v) => (version = v));
    checkAppUpdate();
    const iv = setInterval(() => (now = Date.now()), 30000);
    const iv2 = setInterval(() => !appUpdate && checkAppUpdate(), 4 * 3600 * 1000);
    const un = [
      listen("scan-progress", (e) => (progress = e.payload)),
      listen("scan-done", (e) => {
        items = e.payload.items;
        meta = e.payload;
        scanning = false;
        scannedAt = Date.now();
      }),
      listen("task-log", (e) => {
        const t = tasks[e.payload.id];
        if (!t) return;
        t.state = "running";
        t.lines.push(e.payload.line);
        if (t.lines.length > 600) t.lines.splice(0, t.lines.length - 600);
        if (logOpen === e.payload.id) scrollLog();
      }),
      listen("task-done", (e) => {
        const t = tasks[e.payload.id];
        if (!t) return;
        t.state = e.payload.ok ? "ok" : "error";
        t.lines.push(e.payload.ok ? "✓ Erfolgreich abgeschlossen." : "✗ Fehlgeschlagen – Details siehe oben.");
        // Sobald nichts mehr läuft: Bestand neu einlesen, damit die neuen Versionen erscheinen
        if (!Object.values(tasks).some((x) => x.state === "running" || x.state === "queued")) setTimeout(() => scan(false), 800);
      }),
    ];
    scan(false);
    return () => {
      clearInterval(iv);
      clearInterval(iv2);
      un.forEach((p) => p.then((f) => f()));
    };
  });

  async function checkAppUpdate() {
    try {
      appUpdate = await check();
    } catch {} // offline oder noch kein Release
  }
  async function installAppUpdate() {
    appUpdating = true;
    try {
      await appUpdate.downloadAndInstall();
      await relaunch();
    } catch (e) {
      appUpdating = false;
      alertText = String(e);
    }
  }
  let alertText = $state("");

  async function scan(force) {
    if (scanning) return;
    scanning = true;
    progress = null;
    const started = await api.startScan(force).catch(() => false);
    if (!started) scanning = false;
  }

  // ------------------------------------------------------------------ Ableitungen
  const visible = (it) => !query || `${it.name} ${it.category ?? ""} ${it.brew_token ?? ""}`.toLowerCase().includes(query.toLowerCase());
  let apps = $derived(items.filter((i) => i.kind !== "formula"));
  let updates = $derived(
    apps
      .filter((i) => (i.status === "update" || i.status === "notice") && !ignored.has(i.id))
      .sort((a, b) => SEV_ORDER[a.severity] - SEV_ORDER[b.severity] || a.name.localeCompare(b.name, "de")),
  );
  let cli = $derived(items.filter((i) => i.kind === "formula" && !ignored.has(i.id)).sort((a, b) => SEV_ORDER[a.severity] - SEV_ORDER[b.severity] || a.name.localeCompare(b.name)));
  let current = $derived(apps.filter((i) => i.status === "current" && !ignored.has(i.id)));
  let unknown = $derived(apps.filter((i) => i.status === "unknown" && !ignored.has(i.id)));
  let ignoredList = $derived(items.filter((i) => ignored.has(i.id)));
  let counts = $derived({
    wichtig: updates.filter((i) => i.severity === "wichtig").length,
    empfohlen: updates.filter((i) => i.severity === "empfohlen").length,
    optional: updates.filter((i) => i.severity === "optional").length,
  });
  let radarItems = $derived(updates.filter((i) => i.status === "update" && i.severity));
  let autoImportant = $derived(updates.filter((i) => i.severity === "wichtig" && i.status === "update" && isBrew(i.actions[0]) && tasks[i.id]?.state !== "ok"));
  let manualImportant = $derived(updates.filter((i) => i.severity === "wichtig" && i.status === "update" && !isBrew(i.actions[0])));

  const TABS = [
    { id: "updates", label: "Updates", icon: ShieldAlert },
    { id: "cli", label: "Kommandozeile", icon: Terminal },
    { id: "current", label: "Aktuell", icon: CheckCircle2 },
    { id: "unknown", label: "Nicht prüfbar", icon: HelpCircle },
    { id: "ignored", label: "Ignoriert", icon: EyeOff },
  ];
  let lists = $derived({ updates, cli, current, unknown, ignored: ignoredList });
  let shown = $derived(lists[tab].filter(visible));
  let groups = $derived(
    tab === "updates"
      ? ["wichtig", "empfohlen", "optional"].map((s) => ({ s, rows: shown.filter((i) => i.severity === s) })).filter((g) => g.rows.length)
      : [{ s: null, rows: shown }],
  );

  function isBrew(a) {
    return a && (a.kind === "brew" || a.kind === "quit_and_brew");
  }

  // ------------------------------------------------------------------ Aktionen
  function act(item, index) {
    const a = item.actions[index];
    if (!a) return;
    const run = () => start(item, index);
    if (a.confirm) confirm = { title: `${item.name}: ${a.label}`, text: a.confirm, run };
    else run();
  }

  function start(item, index) {
    const a = item.actions[index];
    if (isBrew(a)) {
      const busy = Object.values(tasks).some((x) => x.state === "running" || x.state === "queued");
      tasks[item.id] = { name: item.name, state: busy ? "queued" : "running", lines: [] };
    }
    api.runAction(item.id, index).catch((e) => {
      if (tasks[item.id]) {
        tasks[item.id].state = "error";
        tasks[item.id].lines.push(String(e));
      } else alertText = String(e);
    });
  }

  function updateImportant() {
    confirm = {
      title: `${autoImportant.length} wichtige Updates installieren`,
      text: "Homebrew lädt die aktuellen Versionen direkt vom Hersteller und installiert sie nacheinander. Laufende Apps werden dafür beendet. Wenn Administratorrechte nötig sind, fragt macOS nach deinem Passwort.",
      list: autoImportant.map((i) => `${i.name}  ${i.installed} → ${i.latest}`),
      manual: manualImportant.map((i) => `${i.name} – ${i.actions[0]?.label ?? "beim Hersteller"}`),
      run: () => autoImportant.forEach((i) => start(i, 0)),
    };
  }

  function upgradeCli() {
    confirm = {
      title: `${cli.length} Kommandozeilen-Pakete aktualisieren`,
      text: "Führt „brew upgrade --formula“ aus. Das kann je nach Anzahl einige Minuten dauern.",
      run: () => {
        tasks["formula:*"] = { name: "Kommandozeilen-Pakete", state: "running", lines: [] };
        api.upgradeAllFormulae().catch((e) => {
          tasks["formula:*"].state = "error";
          tasks["formula:*"].lines.push(String(e));
        });
        logOpen = "formula:*";
      },
    };
  }

  function toggleIgnore(item) {
    const s = new Set(ignored);
    s.has(item.id) ? s.delete(item.id) : s.add(item.id);
    ignored = s;
    store.set("ignored", [...s]);
  }

  let logEl = $state(null);
  async function scrollLog() {
    await tick();
    if (logEl) logEl.scrollTop = logEl.scrollHeight;
  }
  function openLog(id) {
    logOpen = logOpen === id ? null : id;
    scrollLog();
  }
  let agoText = $derived((now, ago(scannedAt)));
  let runningCount = $derived(Object.values(tasks).filter((t) => t.state === "running" || t.state === "queued").length);
</script>

<div class="blobs" aria-hidden="true"><i></i><i></i><i></i></div>

<div class="app">
  <header data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <img class="logo" src="/logo.png" alt="" />
      <b data-tauri-drag-region>Update-Radar</b>
      {#if version}<small class="muted">{version}</small>{/if}
    </div>
    <div class="head-r">
      {#if runningCount}<span class="pill run"><RefreshCw size={12} class="spin" /> {runningCount} Update{runningCount > 1 ? "s" : ""} laufen</span>{/if}
      <button class="ghost small" disabled={scanning} onclick={() => scan(true)} title="Katalog neu laden und alles neu prüfen">
        <RefreshCw size={14} class={scanning ? "spin" : ""} /> Neu prüfen
      </button>
    </div>
  </header>

  {#if appUpdate}
    <div class="banner">
      <Download size={15} /> Update-Radar {appUpdate.version} ist da.
      <button class="primary small" disabled={appUpdating} onclick={installAppUpdate}>{appUpdating ? "Wird installiert …" : "Jetzt aktualisieren"}</button>
      <button class="ghost small" onclick={() => (appUpdate = null)} aria-label="Später"><X size={14} /></button>
    </div>
  {/if}

  <main>
    <section class="hero card">
      <Radar items={radarItems} {scanning} />
      <div class="summary">
        {#if scanning && !items.length}
          <h1>Dein Mac wird geprüft …</h1>
          <p class="muted step">{progress?.step ?? "Starte …"}</p>
          {#if progress?.total}
            <div class="bar"><i style="width:{(progress.done / progress.total) * 100}%"></i></div>
            <p class="muted small num">{progress.done} von {progress.total} Programmen</p>
          {/if}
        {:else}
          <h1>
            {#if updates.length}
              <span class="big num">{updates.length}</span> {updates.length === 1 ? "Update verfügbar" : "Updates verfügbar"}
            {:else}
              <span class="big ok">Alles aktuell</span>
            {/if}
          </h1>
          <div class="chips">
            {#each Object.entries(SEV) as [k, s]}
              <span class="chip" style="color:{s.color};background:{s.bg}"><b class="num">{counts[k]}</b> {s.label.toLowerCase()}</span>
            {/each}
            {#if cli.length}<span class="chip neutral"><b class="num">{cli.length}</b> Kommandozeile</span>{/if}
          </div>
          <p class="muted meta">
            {[`${apps.length} Programme geprüft`, meta?.macos && `macOS ${meta.macos}`, scannedAt && agoText].filter(Boolean).join("  ·  ")}
            {#if scanning}<span class="pill"><RefreshCw size={11} class="spin" /> {progress?.step ?? "prüft …"}</span>{/if}
          </p>
          <div class="cta">
            {#if autoImportant.length}
              <button class="primary" onclick={updateImportant}><Zap size={15} /> {autoImportant.length} wichtige Updates installieren</button>
            {:else if counts.wichtig}
              <span class="muted small">Wichtige Updates bitte einzeln anstoßen (App Store bzw. Hersteller).</span>
            {/if}
          </div>
        {/if}
        {#if meta && !meta.brew}
          <p class="note"><Info size={14} /> Homebrew ist nicht installiert – Updates werden nur angezeigt. Installation: <code>brew.sh</code></p>
        {/if}
        {#if meta?.catalogError}<p class="note warn"><Info size={14} /> {meta.catalogError}</p>{/if}
      </div>
    </section>

    <div class="toolbar">
      <nav>
        {#each TABS as t}
          <button class="tab" class:active={tab === t.id} onclick={() => (tab = t.id)}>
            <t.icon size={14} /> {t.label} <span class="cnt num">{lists[t.id].length}</span>
          </button>
        {/each}
      </nav>
      <label class="search"><Search size={14} /><input placeholder="Suchen …" bind:value={query} /></label>
    </div>

    <section class="list card">
      {#if tab === "cli" && cli.length}
        <div class="cli-head">
          <span class="muted">Über Homebrew installierte Werkzeuge und Bibliotheken.</span>
          <button class="primary small" disabled={tasks["formula:*"]?.state === "running"} onclick={upgradeCli}><Sparkles size={13} /> Alle aktualisieren</button>
        </div>
        {#if tasks["formula:*"]}
          <button class="log-all" onclick={() => openLog("formula:*")}>Protokoll: {tasks["formula:*"].lines.at(-1) ?? "startet …"}</button>
        {/if}
      {/if}
      {#each groups as g}
        {#if g.s}<h3 class="group" style="color:{SEV[g.s].color}">{SEV[g.s].label} <span class="muted num">{g.rows.length}</span></h3>{/if}
        {#each g.rows as it (it.id)}
          <ItemRow item={it} task={tasks[it.id]} ignored={ignored.has(it.id)} onaction={act} onignore={toggleIgnore} ontoggle={openLog} />
        {/each}
      {:else}
        <div class="empty muted">
          {#if scanning}Wird geprüft …{:else if query}Keine Treffer für „{query}“.{:else if tab === "updates"}Keine offenen Updates – sehr gut.{:else}Hier ist nichts.{/if}
        </div>
      {/each}
      {#if tab === "unknown" && shown.length}
        <p class="foot muted">Für diese Programme kennt Update-Radar keine verlässliche Update-Quelle (Eigenentwicklungen, Spiele, Hersteller ohne öffentlichen Katalog). Bitte bei Bedarf beim Hersteller prüfen.</p>
      {/if}
    </section>
  </main>

  {#if logOpen && tasks[logOpen]}
    <aside class="log card">
      <div class="log-head">
        <b>{tasks[logOpen].name}</b>
        <span class="muted small">{{ queued: "wartet", running: "läuft", ok: "fertig", error: "fehlgeschlagen" }[tasks[logOpen].state]}</span>
        <button class="ghost small" onclick={() => (logOpen = null)} aria-label="Schließen"><X size={14} /></button>
      </div>
      <pre bind:this={logEl}>{tasks[logOpen].lines.join("\n") || "Wartet auf Ausgabe …"}</pre>
    </aside>
  {/if}

  {#if confirm}
    <div class="modal-bg" role="presentation" onclick={() => (confirm = null)}>
      <div class="modal card" role="dialog" aria-modal="true" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === "Escape" && (confirm = null)}>
        <h2>{confirm.title}</h2>
        <p>{confirm.text}</p>
        {#if confirm.list?.length}<ul class="num">{#each confirm.list as l}<li>{l}</li>{/each}</ul>{/if}
        {#if confirm.manual?.length}
          <p class="muted small">Nicht automatisch möglich – bitte einzeln anstoßen:</p>
          <ul class="muted">{#each confirm.manual as l}<li>{l}</li>{/each}</ul>
        {/if}
        <div class="modal-acts">
          <button class="ghost" onclick={() => (confirm = null)}>Abbrechen</button>
          <button class="primary" onclick={() => { const r = confirm.run; confirm = null; r(); }}>Starten</button>
        </div>
      </div>
    </div>
  {/if}

  {#if alertText}
    <div class="toast card" role="alert">{alertText}<button class="ghost small" onclick={() => (alertText = "")}><X size={13} /></button></div>
  {/if}
</div>

<style>
  .app { position: relative; height: 100vh; display: flex; flex-direction: column; }
  header { height: 52px; flex-shrink: 0; display: flex; align-items: center; justify-content: space-between; padding: 0 18px 0 88px; }
  .brand { display: flex; align-items: center; gap: 9px; font-size: 14.5px; }
  .logo { width: 24px; height: 24px; border-radius: 6px; }
  .head-r { display: flex; align-items: center; gap: 10px; }
  .pill { display: inline-flex; align-items: center; gap: 5px; font-size: 11.5px; padding: 3px 9px; border-radius: 99px; background: var(--surface-2); color: var(--muted); margin-left: 6px; }
  .pill.run { background: var(--blue-bg); color: var(--blue); font-weight: 550; }
  .banner {
    margin: 0 24px 12px; padding: 8px 12px; display: flex; align-items: center; gap: 10px; justify-content: center; font-size: 13px;
    border-radius: 12px; background: var(--blue-bg); color: var(--blue); animation: drop 0.5s var(--ease) both;
  }
  @keyframes drop { from { opacity: 0; transform: translateY(-8px); } }
  main { flex: 1; min-height: 0; padding: 0 24px 22px; display: flex; flex-direction: column; gap: 14px; }

  .hero { display: flex; align-items: center; gap: 34px; padding: 22px 30px; flex-shrink: 0; }
  .summary { flex: 1; min-width: 0; }
  .summary h1 { font-size: 22px; display: flex; align-items: baseline; gap: 10px; }
  .big { font-size: 44px; font-weight: 700; letter-spacing: -0.03em; background: var(--grad); -webkit-background-clip: text; background-clip: text; color: transparent; }
  .big.ok { font-size: 34px; background: linear-gradient(135deg, #18a058, #22c1c3); -webkit-background-clip: text; background-clip: text; }
  .chips { display: flex; gap: 8px; margin: 12px 0 8px; flex-wrap: wrap; }
  .chip { font-size: 12.5px; padding: 4px 11px; border-radius: 99px; font-weight: 550; }
  .chip.neutral { background: var(--surface-2); color: var(--muted); }
  .meta { font-size: 12.5px; margin: 4px 0 0; display: flex; align-items: center; flex-wrap: wrap; }
  .cta { margin-top: 14px; min-height: 36px; display: flex; align-items: center; }
  .step { margin: 8px 0 12px; font-size: 13px; }
  .bar { height: 6px; border-radius: 99px; background: var(--surface-2); overflow: hidden; max-width: 420px; }
  .bar i { display: block; height: 100%; background: var(--grad); border-radius: 99px; transition: width 0.25s; }
  .small { font-size: 12px; }
  .note { margin: 12px 0 0; font-size: 12.5px; display: flex; align-items: center; gap: 6px; color: var(--muted); }
  .note.warn { color: var(--amber); }
  code { background: var(--surface-2); padding: 1px 6px; border-radius: 5px; }

  .toolbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-shrink: 0; }
  nav { display: flex; gap: 4px; background: var(--surface); border: 1px solid var(--border); padding: 4px; border-radius: 12px; }
  .tab { border: none; background: transparent; padding: 6px 12px; font-size: 13px; color: var(--muted); border-radius: 9px; box-shadow: none !important; }
  .tab.active { background: var(--surface-2); color: var(--text); }
  .cnt { font-size: 11px; padding: 0 6px; border-radius: 99px; background: var(--surface-2); }
  .tab.active .cnt { background: var(--surface); }
  .search { display: flex; align-items: center; gap: 6px; color: var(--muted); }
  .search input { width: 220px; padding: 7px 11px; }

  .list { flex: 1; min-height: 0; overflow-y: auto; padding: 8px; }
  .group { font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; padding: 12px 14px 4px; display: flex; gap: 8px; }
  .empty { padding: 60px 20px; text-align: center; }
  .foot { font-size: 12px; padding: 8px 14px; }
  .cli-head { display: flex; justify-content: space-between; align-items: center; padding: 8px 14px 4px; font-size: 13px; }
  .log-all { margin: 4px 14px 8px; font-size: 12px; border: none; background: var(--surface-2); color: var(--muted); max-width: 70%; overflow: hidden; text-overflow: ellipsis; }

  .log { position: fixed; right: 24px; bottom: 22px; width: min(620px, calc(100vw - 48px)); max-height: 46vh; display: flex; flex-direction: column; z-index: 30; box-shadow: var(--shadow-lg); animation: drop 0.3s var(--ease); }
  .log-head { display: flex; align-items: center; gap: 10px; padding: 10px 10px 8px 16px; border-bottom: 1px solid var(--border); }
  .log-head b { flex: 1; }
  pre { margin: 0; padding: 12px 16px; overflow: auto; font: 11.5px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; white-space: pre-wrap; user-select: text; -webkit-user-select: text; }

  .modal-bg { position: fixed; inset: 0; background: rgba(10, 12, 24, 0.35); backdrop-filter: blur(4px); display: grid; place-items: center; z-index: 50; animation: fade 0.2s; }
  .modal { width: min(520px, calc(100vw - 48px)); padding: 24px; box-shadow: var(--shadow-lg); animation: drop 0.3s var(--ease); outline: none; }
  .modal h2 { font-size: 17px; margin-bottom: 10px; }
  .modal p { line-height: 1.5; margin: 0 0 10px; font-size: 13.5px; }
  .modal ul { margin: 0 0 12px; padding-left: 18px; max-height: 200px; overflow: auto; font-size: 13px; line-height: 1.6; }
  .modal-acts { display: flex; justify-content: flex-end; gap: 8px; margin-top: 14px; }
  @keyframes fade { from { opacity: 0; } }
  .toast { position: fixed; left: 50%; bottom: 24px; transform: translateX(-50%); padding: 10px 10px 10px 16px; display: flex; gap: 8px; align-items: center; font-size: 13px; z-index: 60; color: var(--red); }

  .blobs { position: fixed; inset: 0; z-index: -1; overflow: hidden; filter: blur(80px); opacity: 0.5; }
  .blobs i { position: absolute; width: 480px; height: 480px; border-radius: 50%; animation: drift 28s ease-in-out infinite alternate; }
  .blobs i:nth-child(1) { background: #c9f2f2; top: -180px; left: -140px; }
  .blobs i:nth-child(2) { background: #dcdcff; bottom: -220px; right: -120px; animation-delay: -9s; }
  .blobs i:nth-child(3) { background: #e3f0ff; top: 35%; left: 45%; width: 360px; height: 360px; animation-delay: -17s; }
  @media (prefers-color-scheme: dark) { .blobs { opacity: 0.12; } }
  @keyframes drift { to { transform: translate(90px, 60px) scale(1.15); } }
  @media (prefers-reduced-motion: reduce) { .blobs i { animation: none; } }
</style>
