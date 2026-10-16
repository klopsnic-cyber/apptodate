# AppToDate

Prüft die Programme auf dem Mac auf Updates, stuft sie nach Dringlichkeit ein und stößt die Aktualisierung an.
Tauri 2 + Svelte 5 + Rust. Gleiches Muster wie `~/Cowork/onlyfiles` (Updater, release.sh, CI).

## Woher die aktuellen Versionen kommen (src-tauri/src)
- `catalog.rs` – Homebrew-Cask-Katalog `formulae.brew.sh/api/cask.json` (~19 MB, 6 h Zwischenspeicher im App-Cache).
  Zuordnung installierte App → Cask: App-Artefakt-Name, `uninstall.delete`-Pfade, eindeutige `uninstall.quit`-Bundle-ID,
  zuletzt eindeutiger Programmname (dann `uncertain`, kein automatisches Update). Vorabversionen (`@preview`) nur,
  wenn installiert ebenfalls Vorabversion. `apps > 3` = Programmpaket (Office) → nur bei gleicher Hauptversion vergleichen.
- `scan.rs` – installierte Apps (`/Applications`, `~/Applications`, eine Unterordner-Ebene; Chrome/Safari-Web-Apps
  ausgelassen), App Store (iTunes-Lookup per Bundle-ID), Sparkle-Feed (`SUFeedURL`, Beta-Kanäle übersprungen),
  `brew outdated --formula`, `softwareupdate -l`. Office: Katalog nennt Build-Nummer → mit `CFBundleVersion` vergleichen.
- `version.rs` – Versionsvergleich mit Tests für echte Fälle (Brave-Chromium-Präfix, „8.0.1 (build 6301)“, „x,hash“,
  angehängtes 4. Build-Segment). Bei unplausiblem Schema `Unknown` statt falschem Update.
- `classify.rs` – wichtig (sicherheitsrelevante Kategorie / macOS), empfohlen (neue Hauptversion, Laufzeit), optional.
  Schlagworte mit Wortgrenzen (kurze Begriffe nur als ganzes Wort).

## Updates ausführen (lib.rs)
- Aktionen werden nur per (Eintrags-ID, Index) aus dem letzten Scan nachgeschlagen – das Frontend übergibt keine Befehle.
- Homebrew-Cask schon verwaltet → `brew upgrade --cask`; sonst `brew install --cask --force` (übernimmt die App,
  Einstellungen bleiben). Laufende App wird vorher per AppleScript beendet. Aufträge laufen nacheinander (Mutex).
- Admin-Rechte: `SUDO_ASKPASS` → Skript im App-Data-Ordner mit macOS-Passwortdialog (Homebrew nutzt dann `sudo -A`).
- App Store / Sparkle / macOS: nur öffnen (App Store, App selbst, Systemeinstellungen) – kein Eingriff.

## Entwickeln
```
npm install
npm run tauri dev
cd src-tauri && cargo test                     # Einheitentests
cargo test live -- --ignored --nocapture       # echter Durchlauf gegen den eigenen Mac (Tabelle)
npm run tauri build
```
Release: `./release.sh 0.2.0 "Was ist neu"` (Signaturschlüssel `~/.tauri/update-radar.key` + `.passwort`, als Repo-Secrets
`TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` hinterlegen – niemals ausgeben oder committen).
