//! Bestandsaufnahme: installierte Apps, Homebrew, App Store, Sparkle-Feeds und macOS-Updates.

use crate::catalog::{Catalog, Match};
use crate::version::{self, Cmp};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub const PATH_ENV: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

#[derive(Serialize, Clone, Debug)]
pub struct Action {
    /// brew | open_app | open_url | quit_and_brew
    pub kind: &'static str,
    pub label: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Hinweis, der vor dem Ausführen angezeigt wird
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Item {
    pub id: String,
    /// app | formula | macos
    pub kind: &'static str,
    pub name: String,
    pub path: Option<String>,
    pub bundle_id: Option<String>,
    pub installed: String,
    pub latest: Option<String>,
    /// update | current | unknown | notice
    pub status: &'static str,
    /// wichtig | empfohlen | optional (nur bei Updates und Hinweisen)
    pub severity: Option<&'static str>,
    pub category: Option<&'static str>,
    pub reasons: Vec<String>,
    /// Homebrew | Sparkle | App Store | macOS | Hersteller
    pub source: String,
    pub uncertain: bool,
    pub brew_token: Option<String>,
    pub brew_managed: bool,
    pub self_updating: bool,
    pub homepage: Option<String>,
    pub desc: Option<String>,
    pub actions: Vec<Action>,
    /// App-Symbol als Data-URL
    pub icon: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Installierte Apps

#[derive(Clone, Debug)]
pub struct App {
    pub path: PathBuf,
    pub file: String,
    pub name: String,
    pub bundle_id: String,
    pub short: String,
    pub build: String,
    pub feed: Option<String>,
    pub mas: bool,
    pub icon_file: Option<PathBuf>,
}

pub fn installed_apps() -> Vec<App> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut dirs = vec![PathBuf::from("/Applications"), PathBuf::from(format!("{home}/Applications"))];
    // eine Ebene Unterordner (z. B. „/Applications/Utilities“, Hersteller-Ordner)
    for d in dirs.clone() {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() && p.extension().is_none_or(|x| x != "app") {
                    dirs.push(p);
                }
            }
        }
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| x != "app") {
                continue;
            }
            if let Some(a) = read_app(&p) {
                if seen.insert(a.bundle_id.clone() + &a.path.to_string_lossy()) {
                    out.push(a);
                }
            }
        }
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

fn read_app(p: &Path) -> Option<App> {
    let info: plist::Dictionary = plist::from_file(p.join("Contents/Info.plist")).ok()?;
    let s = |k: &str| info.get(k).and_then(|v| v.as_string()).map(str::to_string);
    let file = p.file_name()?.to_string_lossy().into_owned();
    let bundle = s("CFBundleIdentifier").unwrap_or_default();
    // Web-Apps von Chrome/Safari werden mit dem Browser aktualisiert → nicht aufführen
    if bundle.starts_with("com.google.Chrome.app.") || bundle.starts_with("com.apple.Safari.WebApp") || p.to_string_lossy().contains("Chrome Apps.localized") {
        return None;
    }
    let name = s("CFBundleDisplayName").or_else(|| s("CFBundleName")).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| file.trim_end_matches(".app").to_string());
    let res = p.join("Contents/Resources");
    let icon_file = s("CFBundleIconFile")
        .map(|f| if f.ends_with(".icns") { f } else { format!("{f}.icns") })
        .map(|f| res.join(f))
        .filter(|f| f.exists())
        .or_else(|| Some(res.join("AppIcon.icns")).filter(|f| f.exists()));
    Some(App {
        path: p.to_path_buf(),
        name,
        bundle_id: bundle,
        short: s("CFBundleShortVersionString").unwrap_or_default().trim().to_string(),
        build: s("CFBundleVersion").unwrap_or_default().trim().to_string(),
        feed: s("SUFeedURL").filter(|u| u.starts_with("https://") || u.starts_with("http://")),
        mas: p.join("Contents/_MASReceipt/receipt").exists(),
        icon_file,
        file,
    })
}

/// App-Symbol als PNG-Data-URL (über `sips`, mit Zwischenspeicher).
pub fn icon(app: &App, cache: &Path) -> Option<String> {
    use base64::Engine;
    let src = app.icon_file.as_ref()?;
    let mtime = std::fs::metadata(src).and_then(|m| m.modified()).ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let key = format!("{:x}-{mtime}.png", fxhash(&src.to_string_lossy()));
    let out = cache.join("icons").join(key);
    if !out.exists() {
        let _ = std::fs::create_dir_all(out.parent()?);
        let ok = Command::new("/usr/bin/sips")
            .args(["-s", "format", "png", "-Z", "64"])
            .arg(src)
            .arg("--out")
            .arg(&out)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            return None;
        }
    }
    let bytes = std::fs::read(&out).ok()?;
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

fn fxhash(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

// ---------------------------------------------------------------------------------------------
// Homebrew

pub fn brew_path() -> Option<&'static str> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"].into_iter().find(|p| Path::new(p).exists())
}

fn brew(args: &[&str]) -> Option<String> {
    let b = brew_path()?;
    let o = Command::new(b)
        .args(args)
        .env("PATH", PATH_ENV)
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .output()
        .ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Über Homebrew installierte Casks: Token → installierte Version
pub fn brew_casks() -> HashMap<String, String> {
    brew(&["list", "--cask", "--versions"])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.to_string(), it.last().unwrap_or("").to_string()))
        })
        .collect()
}

#[derive(serde::Deserialize)]
struct Outdated {
    #[serde(default)]
    formulae: Vec<OutdatedFormula>,
}
#[derive(serde::Deserialize)]
struct OutdatedFormula {
    name: String,
    installed_versions: Vec<String>,
    current_version: String,
    #[serde(default)]
    pinned: bool,
}

pub fn brew_outdated_formulae() -> Vec<Item> {
    let Some(j) = brew(&["outdated", "--formula", "--json=v2"]) else { return vec![] };
    let Ok(o) = serde_json::from_str::<Outdated>(&j) else { return vec![] };
    o.formulae
        .into_iter()
        .filter(|f| !f.pinned)
        // Homebrew meldet gelegentlich eine niedrigere Version als „outdated“ (z. B. zurückgezogene Version) → nicht anbieten
        .filter(|f| f.installed_versions.last().is_some_and(|i| version::compare(i, &f.current_version) != Cmp::Older))
        .map(|f| {
            let installed = f.installed_versions.last().cloned().unwrap_or_default();
            let mut it = Item {
                id: format!("formula:{}", f.name),
                kind: "formula",
                name: f.name.clone(),
                path: None,
                bundle_id: None,
                installed,
                latest: Some(f.current_version.clone()),
                status: "update",
                severity: None,
                category: Some("Kommandozeile"),
                reasons: vec![],
                source: "Homebrew".into(),
                uncertain: false,
                brew_token: Some(f.name.clone()),
                brew_managed: true,
                self_updating: false,
                homepage: Some(format!("https://formulae.brew.sh/formula/{}", f.name)),
                desc: None,
                actions: vec![Action { kind: "brew", label: "Aktualisieren".into(), args: vec!["upgrade".into(), f.name.clone()], url: None, confirm: None }],
                icon: None,
            };
            crate::classify::rate(&mut it);
            it
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Sparkle-Feed (eigener Update-Kanal vieler Mac-Apps)

/// Neueste stabile Version aus einem Appcast: (Anzeige-Version, Build)
pub fn sparkle_latest(feed: &str) -> Option<(String, String)> {
    let body = http_get(feed)?;
    parse_appcast(&body)
}

pub fn parse_appcast(xml: &str) -> Option<(String, String)> {
    const NS: &str = "http://www.andymatuschak.org/xml-namespaces/sparkle";
    let doc = roxmltree::Document::parse(xml).ok()?;
    let mut best: Option<(String, String)> = None;
    for item in doc.descendants().filter(|n| n.has_tag_name("item")) {
        let child = |name: &str| item.children().find(|c| c.tag_name().name() == name && c.tag_name().namespace() == Some(NS)).and_then(|c| c.text()).map(|t| t.trim().to_string());
        if child("channel").is_some() {
            continue; // Beta-/Testkanäle überspringen
        }
        let enc = item.children().find(|c| c.has_tag_name("enclosure"));
        let attr = |name: &str| enc.and_then(|e| e.attribute((NS, name))).map(str::to_string);
        let build = child("version").or_else(|| attr("version")).unwrap_or_default();
        let short = child("shortVersionString").or_else(|| attr("shortVersionString")).unwrap_or_else(|| build.clone());
        if short.is_empty() {
            continue;
        }
        let newer = match &best {
            None => true,
            Some((s, b)) => version::compare(s, &short) == Cmp::Newer || (version::compare(s, &short) == Cmp::Same && version::compare(b, &build) == Cmp::Newer),
        };
        if newer {
            best = Some((short, build));
        }
    }
    best
}

// ---------------------------------------------------------------------------------------------
// App Store

pub struct StoreInfo {
    pub version: String,
    pub url: String,
}

pub fn appstore_lookup(bundle_id: &str) -> Option<StoreInfo> {
    let body = http_get(&format!("https://itunes.apple.com/lookup?bundleId={bundle_id}&country=de&entity=macSoftware"))?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    let r = v["results"].get(0)?;
    Some(StoreInfo {
        version: r["version"].as_str()?.to_string(),
        url: r["trackViewUrl"].as_str()?.replacen("https://", "macappstore://", 1),
    })
}

fn http_get(url: &str) -> Option<String> {
    ureq::get(url)
        .timeout(Duration::from_secs(10))
        .set("User-Agent", "AppToDate/1.0 (macOS)")
        .call()
        .ok()?
        .into_string()
        .ok()
}

// ---------------------------------------------------------------------------------------------
// macOS

pub fn macos_version() -> String {
    Command::new("/usr/bin/sw_vers").arg("-productVersion").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

/// Ausstehende Apple-Updates aus `softwareupdate -l`
pub fn macos_updates() -> Vec<(String, String, String, bool)> {
    let Ok(o) = Command::new("/usr/sbin/softwareupdate").arg("-l").env("LANG", "en_US.UTF-8").output() else { return vec![] };
    parse_softwareupdate(&String::from_utf8_lossy(&o.stdout))
}

/// (Label, Titel, Version, Neustart nötig)
pub fn parse_softwareupdate(out: &str) -> Vec<(String, String, String, bool)> {
    let mut res = Vec::new();
    let mut label: Option<String> = None;
    for line in out.lines() {
        let t = line.trim();
        if let Some(l) = t.strip_prefix("* Label:") {
            label = Some(l.trim().to_string());
        } else if let (Some(l), Some(rest)) = (label.as_ref(), t.strip_prefix("Title:")) {
            let field = |k: &str| rest.split(',').find_map(|p| p.trim().strip_prefix(k).map(|v| v.trim().to_string()));
            let title = rest.split(',').next().unwrap_or("").trim().to_string();
            let ver = field("Version:").unwrap_or_default();
            let restart = field("Action:").is_some_and(|a| a.contains("restart"));
            res.push((l.clone(), title, ver, restart));
            label = None;
        }
    }
    res
}

// ---------------------------------------------------------------------------------------------
// Zusammenführen

pub fn evaluate(app: &App, cat: &Catalog, brew_casks: &HashMap<String, String>) -> Item {
    let installed = if app.short.is_empty() { app.build.clone() } else { app.short.clone() };
    let mut it = Item {
        id: format!("app:{}", app.path.to_string_lossy()),
        kind: "app",
        name: app.name.clone(),
        path: Some(app.path.to_string_lossy().into_owned()),
        bundle_id: Some(app.bundle_id.clone()).filter(|b| !b.is_empty()),
        installed: installed.clone(),
        latest: None,
        status: "unknown",
        severity: None,
        category: None,
        reasons: vec![],
        source: "Hersteller".into(),
        uncertain: false,
        brew_token: None,
        brew_managed: false,
        self_updating: app.feed.is_some(),
        homepage: None,
        desc: None,
        actions: vec![],
        icon: None,
    };
    let open_app = Action { kind: "open_app", label: "App öffnen".into(), args: vec![], url: None, confirm: None };

    // selbst erstellte AppleScript-/Automator-Programme haben keinen Hersteller
    if app.bundle_id.starts_with("com.apple.ScriptEditor.id.") || app.bundle_id.starts_with("com.apple.Automator.") {
        it.reasons.push("Selbst erstelltes AppleScript- bzw. Automator-Programm – es gibt keine Hersteller-Updates.".into());
        return it;
    }
    // Apple-Programme ohne App Store kommen mit macOS-Updates
    if app.bundle_id.starts_with("com.apple.") && !app.mas {
        it.source = "macOS".into();
        it.reasons.push("Wird mit den macOS-Updates aktualisiert.".into());
        crate::classify::rate(&mut it);
        return it;
    }

    if app.mas {
        it.source = "App Store".into();
        match appstore_lookup(&app.bundle_id) {
            Some(s) => {
                it.latest = Some(s.version.clone());
                it.status = status(version::compare(&installed, &s.version));
                it.actions.push(Action { kind: "open_url", label: "Im App Store öffnen".into(), args: vec![], url: Some(s.url), confirm: None });
            }
            None => {
                it.reasons.push("Aktuelle Version im App Store nicht abrufbar – im App Store unter „Updates“ prüfen.".into());
                it.actions.push(Action { kind: "open_url", label: "App-Store-Updates öffnen".into(), args: vec![], url: Some("macappstore://showUpdatesPage".into()), confirm: None });
            }
        }
        crate::classify::rate(&mut it);
        return it;
    }

    if let Some((c, m)) = cat.find(&app.file, &app.bundle_id, &installed) {
        it.source = "Homebrew".into();
        it.brew_token = Some(c.token.clone());
        it.brew_managed = brew_casks.contains_key(&c.token);
        it.homepage = c.homepage.clone();
        it.desc = c.desc.clone();
        it.uncertain = m == Match::Name;
        it.self_updating |= c.auto_updates == Some(true);
        let latest = c.version.split(',').next().unwrap_or(&c.version).to_string();
        if c.version != "latest" {
            it.latest = Some(latest.clone());
            it.status = status(version::compare(&installed, &latest));
            // Synology & Co.: Katalog verwendet Build-Nummern – mit CFBundleVersion vergleichen
            if it.status == "unknown" && !app.build.is_empty() {
                it.status = status(version::compare(&app.build, &latest));
            }
            // Office & Co.: Katalog nennt die Build-Nummer („16.114.26101123“), die installierte Build ist identisch
            if it.status == "update" && !app.build.is_empty() && matches!(version::compare(&app.build, &latest), Cmp::Same | Cmp::Older) {
                it.status = "current";
            }
            // Programmpaket (z. B. Microsoft Office mit Hilfs-Apps): nur bei gleicher Hauptversion vergleichbar
            let major = |v: &str| version::parse(v).map(|x| x.nums[0]);
            if c.apps > 3 && major(&installed) != major(&latest) {
                it.status = "unknown";
                it.latest = None;
                it.reasons.push(format!("Teil des Programmpakets „{}“ mit eigener Versionszählung – wird mit dem Paket aktualisiert.", c.name.first().unwrap_or(&c.token)));
            }
        }
        if c.deprecated || c.disabled {
            it.reasons.push("Homebrew führt diese App als eingestellt – prüfen, ob der Hersteller noch Updates liefert.".into());
            if it.status != "update" {
                it.status = "notice";
            }
        }
        let quit = if app.bundle_id.is_empty() { String::new() } else { format!(" {} wird dafür beendet, falls sie läuft.", app.name) };
        if it.status == "update" {
            if it.brew_managed {
                it.actions.push(Action {
                    kind: "quit_and_brew",
                    label: "Aktualisieren".into(),
                    args: vec!["upgrade".into(), "--cask".into(), c.token.clone()],
                    url: None,
                    confirm: Some(format!("Homebrew aktualisiert {} auf {latest}.{quit}", app.name)),
                });
            } else if !it.uncertain {
                it.actions.push(Action {
                    kind: "quit_and_brew",
                    label: "Aktualisieren".into(),
                    args: vec!["install".into(), "--cask".into(), "--force".into(), c.token.clone()],
                    url: None,
                    confirm: Some(format!(
                        "Homebrew lädt {} {latest} vom Hersteller und ersetzt die installierte App. Einstellungen und Daten bleiben erhalten; künftige Updates laufen dann ebenfalls über AppToDate.{quit}",
                        app.name
                    )),
                });
            }
            if it.self_updating || it.uncertain {
                it.actions.push(open_app.clone());
            }
        }
    } else if let Some(feed) = &app.feed {
        it.source = "Sparkle".into();
        match sparkle_latest(feed) {
            Some((short, build)) => {
                it.latest = Some(short.clone());
                let mut st = status(version::compare(&installed, &short));
                if st == "current" && !build.is_empty() && version::compare(&app.build, &build) == Cmp::Newer {
                    st = "update";
                }
                it.status = st;
            }
            None => it.reasons.push("Update-Feed des Herstellers nicht erreichbar.".into()),
        }
        if it.status == "update" {
            let mut a = open_app.clone();
            a.label = "App öffnen & aktualisieren".into();
            a.confirm = Some(format!("{} hat einen eigenen Updater. Nach dem Öffnen im App-Menü „Nach Updates suchen …“ wählen, falls er nicht von selbst erscheint.", app.name));
            it.actions.push(a);
        }
    } else {
        it.reasons.push("Keine Update-Quelle bekannt – Version bitte beim Hersteller prüfen.".into());
    }
    if let Some(h) = &it.homepage {
        it.actions.push(Action { kind: "open_url", label: "Website".into(), args: vec![], url: Some(h.clone()), confirm: None });
    }
    crate::classify::rate(&mut it);
    it
}

fn status(c: Cmp) -> &'static str {
    match c {
        Cmp::Newer => "update",
        Cmp::Same | Cmp::Older => "current",
        Cmp::Unknown => "unknown",
    }
}

pub fn macos_items(updates: Vec<(String, String, String, bool)>) -> Vec<Item> {
    let current = macos_version();
    let settings = Action {
        kind: "open_url",
        label: "Softwareupdate öffnen".into(),
        args: vec![],
        url: Some("x-apple.systempreferences:com.apple.Software-Update-Settings.extension".into()),
        confirm: None,
    };
    if updates.is_empty() {
        let mut it = macos_item("macos:system".into(), format!("macOS {current}"), current.clone(), None, "current");
        it.reasons.push("Keine ausstehenden Apple-Updates gemeldet.".into());
        it.actions.push(settings);
        return vec![it];
    }
    updates
        .into_iter()
        .map(|(label, title, ver, restart)| {
            let mut it = macos_item(format!("macos:{label}"), title, current.clone(), Some(ver), "update");
            if restart {
                it.reasons.push("Neustart erforderlich.".into());
            }
            it.actions.push(settings.clone());
            crate::classify::rate(&mut it);
            it
        })
        .collect()
}

fn macos_item(id: String, name: String, installed: String, latest: Option<String>, status: &'static str) -> Item {
    Item {
        id,
        kind: "macos",
        name,
        path: None,
        bundle_id: None,
        installed,
        latest,
        status,
        severity: None,
        category: Some("Betriebssystem"),
        reasons: vec![],
        source: "macOS".into(),
        uncertain: false,
        brew_token: None,
        brew_managed: false,
        self_updating: false,
        homepage: None,
        desc: None,
        actions: vec![],
        icon: None,
    }
}

#[cfg(test)]
pub fn test_item() -> Item {
    let mut it = macos_item("t".into(), String::new(), String::new(), None, "current");
    it.kind = "app";
    it.category = None;
    it.source = "Homebrew".into();
    it
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appcast_skips_beta_channel() {
        let xml = r#"<?xml version="1.0"?><rss xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle"><channel>
            <item><title>2.8-rc.2</title><sparkle:channel>beta</sparkle:channel><sparkle:version>297</sparkle:version><sparkle:shortVersionString>2.8-rc.2</sparkle:shortVersionString></item>
            <item><title>2.7.4</title><sparkle:version>280</sparkle:version><sparkle:shortVersionString>2.7.4</sparkle:shortVersionString></item>
            <item><title>2.7.3</title><enclosure url="x" sparkle:version="271" sparkle:shortVersionString="2.7.3"/></item>
        </channel></rss>"#;
        assert_eq!(parse_appcast(xml), Some(("2.7.4".into(), "280".into())));
    }

    #[test]
    fn softwareupdate_output() {
        let out = "Software Update Tool\n\nFinding available software\nSoftware Update found the following new or updated software:\n* Label: macOS Tahoe 26.1-25B78\n\tTitle: macOS Tahoe 26.1, Version: 26.1, Size: 7341210KiB, Recommended: YES, Action: restart,\n* Label: Safari27.1\n\tTitle: Safari, Version: 27.1, Size: 200000KiB, Recommended: YES,\n";
        let r = parse_softwareupdate(out);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], ("macOS Tahoe 26.1-25B78".into(), "macOS Tahoe 26.1".into(), "26.1".into(), true));
        assert!(!r[1].3);
    }
}
