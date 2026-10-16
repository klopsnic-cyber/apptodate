mod catalog;
mod classify;
mod scan;
mod version;

use scan::{Action, Item};
use serde_json::json;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Default)]
struct App {
    catalog: Mutex<Option<Arc<catalog::Catalog>>>,
    items: Mutex<Vec<Item>>,
    scanning: AtomicBool,
    /// läuft gerade ein Homebrew-Vorgang? (Homebrew verträgt keine parallelen Läufe)
    brew_busy: Mutex<()>,
}

fn cache_dir(app: &AppHandle) -> PathBuf {
    app.path().app_cache_dir().unwrap_or_else(|_| std::env::temp_dir().join("apptodate"))
}

/// Parallel abarbeiten (Netzabfragen, Symbole) mit begrenzter Thread-Zahl.
fn par_map<T: Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::with_capacity(items.len()));
    std::thread::scope(|s| {
        for _ in 0..threads.min(items.len()).max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(x) = items.get(i) else { break };
                let r = f(x);
                out.lock().unwrap().push((i, r));
            });
        }
    });
    let mut v = out.into_inner().unwrap();
    v.sort_by_key(|(i, _)| *i);
    v.into_iter().map(|(_, r)| r).collect()
}

/// Startet die Prüfung im Hintergrund. Fortschritt: Event `scan-progress`, Ergebnis: `scan-done`.
#[tauri::command]
fn start_scan(force: bool, app: AppHandle, state: State<'_, Arc<App>>) -> bool {
    if state.scanning.swap(true, Ordering::SeqCst) {
        return false;
    }
    let st = state.inner().clone();
    std::thread::spawn(move || {
        let progress = |step: &str, done: usize, total: usize| {
            let _ = app.emit("scan-progress", json!({ "step": step, "done": done, "total": total }));
        };
        let started = std::time::Instant::now();
        // macOS-Updates dauern am längsten → parallel starten
        let mac = std::thread::spawn(scan::macos_updates);

        progress("Installierte Programme werden gelesen …", 0, 0);
        let apps = scan::installed_apps();

        progress("Aktuelle Versionen werden abgerufen …", 0, 0);
        let cache = cache_dir(&app);
        let cached = st.catalog.lock().unwrap().clone();
        let cat = match (cached, force) {
            (Some(c), false) => Ok(c),
            _ => catalog::load(&cache, force).map(Arc::new),
        };
        let (cat, catalog_error) = match cat {
            Ok(c) => {
                *st.catalog.lock().unwrap() = Some(c.clone());
                (c, None)
            }
            Err(e) => (Arc::new(catalog::Catalog::default()), Some(e)),
        };
        let has_brew = scan::brew_path().is_some();
        let brew_casks = scan::brew_casks();

        let total = apps.len();
        let done = AtomicUsize::new(0);
        let mut items: Vec<Item> = par_map(&apps, 10, |a| {
            let mut it = scan::evaluate(a, &cat, &brew_casks);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(&format!("Prüfe {} …", a.name), n, total);
            it.icon = scan::icon(a, &cache);
            it
        });

        progress("Kommandozeilen-Tools (Homebrew) …", total, total);
        items.extend(scan::brew_outdated_formulae());
        progress("macOS-Updates werden abgefragt …", total, total);
        items.extend(scan::macos_items(mac.join().unwrap_or_default()));

        *st.items.lock().unwrap() = items.clone();
        st.scanning.store(false, Ordering::SeqCst);
        let _ = app.emit(
            "scan-done",
            json!({
                "items": items,
                "catalog": cat.origin,
                "catalogError": catalog_error,
                "brew": has_brew,
                "macos": scan::macos_version(),
                "secs": started.elapsed().as_secs_f64(),
            }),
        );
    });
    true
}

fn find_action(state: &App, id: &str, index: usize) -> Option<(Item, Action)> {
    let items = state.items.lock().unwrap();
    let it = items.iter().find(|i| i.id == id)?;
    Some((it.clone(), it.actions.get(index)?.clone()))
}

/// Führt die Aktion `index` des Eintrags `id` aus (nur Aktionen aus dem letzten Scan).
/// Homebrew-Ausgaben kommen live als Event `task-log`, das Ende als `task-done`.
#[tauri::command]
fn run_action(id: String, index: usize, app: AppHandle, state: State<'_, Arc<App>>) -> Result<(), String> {
    let (item, action) = find_action(&state, &id, index).ok_or("Aktion nicht gefunden – bitte neu prüfen.")?;
    match action.kind {
        "open_url" => {
            let url = action.url.ok_or("keine Adresse")?;
            open(&url)
        }
        "open_app" => open(item.path.as_deref().ok_or("kein Pfad")?),
        "brew" | "quit_and_brew" => {
            let quit = if action.kind == "quit_and_brew" { item.bundle_id.clone() } else { None };
            spawn_brew(item.id, action.args, quit, app, state.inner().clone())
        }
        _ => Err("unbekannte Aktion".into()),
    }
}

/// Aktualisiert alle veralteten Kommandozeilen-Pakete auf einmal (`brew upgrade --formula`).
#[tauri::command]
fn upgrade_all_formulae(app: AppHandle, state: State<'_, Arc<App>>) -> Result<(), String> {
    spawn_brew("formula:*".into(), vec!["upgrade".into(), "--formula".into()], None, app, state.inner().clone())
}

fn spawn_brew(id: String, args: Vec<String>, quit_bundle: Option<String>, app: AppHandle, st: Arc<App>) -> Result<(), String> {
    let brew = scan::brew_path().ok_or("Homebrew ist nicht installiert.")?;
    let askpass = askpass_script(&app)?;
    std::thread::spawn(move || {
        let _guard = st.brew_busy.lock().unwrap(); // Homebrew-Aufträge laufen nacheinander
        let log = |line: &str| {
            let _ = app.emit("task-log", json!({ "id": id, "line": line }));
        };
        if let Some(b) = &quit_bundle {
            quit_app(b, &log);
        }
        log(&format!("$ brew {}", args.join(" ")));
        let ok = run_streaming(brew, &args, &askpass, &log);
        let _ = app.emit("task-done", json!({ "id": id, "ok": ok }));
    });
    Ok(())
}

fn open(target: &str) -> Result<(), String> {
    Command::new("/usr/bin/open").arg(target).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Beendet eine laufende App sauber (wie ⌘Q), damit sie ersetzt werden kann.
fn quit_app(bundle_id: &str, log: &impl Fn(&str)) {
    if !bundle_id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')) {
        return;
    }
    let running = Command::new("/usr/bin/osascript")
        .args(["-e", &format!("application id \"{bundle_id}\" is running")])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false);
    if running {
        log("App wird beendet …");
        let _ = Command::new("/usr/bin/osascript").args(["-e", &format!("tell application id \"{bundle_id}\" to quit")]).output();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

fn run_streaming(brew: &str, args: &[String], askpass: &PathBuf, log: &(impl Fn(&str) + Sync)) -> bool {
    let child = Command::new(brew)
        .args(args)
        .env("PATH", scan::PATH_ENV)
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .env("HOMEBREW_NO_COLOR", "1")
        .env("NONINTERACTIVE", "1")
        .env("SUDO_ASKPASS", askpass)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            log(&format!("Homebrew konnte nicht gestartet werden: {e}"));
            return false;
        }
    };
    let (out, err) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    std::thread::scope(|s| {
        s.spawn(|| BufReader::new(err).lines().map_while(Result::ok).for_each(|l| log(&l)));
        BufReader::new(out).lines().map_while(Result::ok).for_each(|l| log(&l));
    });
    child.wait().map(|s| s.success()).unwrap_or(false)
}

/// Kleines Hilfsskript, über das `sudo` (von Homebrew bei Paket-Installern genutzt) das Admin-Passwort
/// in einem normalen macOS-Dialog abfragt.
fn askpass_script(app: &AppHandle) -> Result<PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let p = dir.join("askpass.sh");
    let script = r#"#!/bin/sh
/usr/bin/osascript <<'EOF'
set r to display dialog "AppToDate: Für dieses Update werden Administratorrechte benötigt. Bitte das Passwort deines Mac-Benutzers eingeben." default answer "" with hidden answer with title "AppToDate" with icon caution buttons {"Abbrechen", "OK"} default button "OK"
return text returned of r
EOF
"#;
    std::fs::write(&p, script).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    Ok(p)
}

#[tauri::command]
fn frontend_log(msg: String) {
    eprintln!("[frontend] {msg}");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Arc::new(App::default()))
        .invoke_handler(tauri::generate_handler![start_scan, run_action, upgrade_all_formulae, frontend_log])
        .run(tauri::generate_context!())
        .expect("Fehler beim Starten von AppToDate");
}

#[cfg(test)]
mod live {
    /// Echter Durchlauf gegen den eigenen Mac: `cargo test live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_scan() {
        let cache = std::env::temp_dir().join("apptodate-test");
        let cat = crate::catalog::load(&cache, false).unwrap();
        let casks = crate::scan::brew_casks();
        let apps = crate::scan::installed_apps();
        let items = crate::par_map(&apps, 10, |a| crate::scan::evaluate(a, &cat, &casks));
        for it in items.iter().chain(crate::scan::brew_outdated_formulae().iter()) {
            println!(
                "{:<9} {:<10} {:<30} {:<22} → {:<18} {:<10} {:<22} {}",
                it.status, it.severity.unwrap_or("-"), it.name, it.installed, it.latest.as_deref().unwrap_or("-"),
                it.source, it.category.unwrap_or("-"),
                it.actions.iter().map(|a| a.label.as_str()).collect::<Vec<_>>().join("|")
            );
        }
    }
}
