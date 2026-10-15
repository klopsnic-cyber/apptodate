//! Homebrew-Cask-Katalog (https://formulae.brew.sh/api/cask.json): aktuelle Versionen von rund 7.800 Programmen.
//! Wird lokal zwischengespeichert und über App-Namen, Bundle-IDs und Programmnamen den installierten Apps zugeordnet.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const URL: &str = "https://formulae.brew.sh/api/cask.json";
const MAX_AGE: Duration = Duration::from_secs(6 * 3600);

#[derive(Deserialize, Clone, Debug)]
pub struct Cask {
    pub token: String,
    #[serde(default)]
    pub name: Vec<String>,
    pub version: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    #[serde(default)]
    pub auto_updates: Option<bool>,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub artifacts: Vec<Value>,
    /// Anzahl der Apps, die der Eintrag installiert (> 3 = Programmpaket wie Microsoft Office)
    #[serde(skip)]
    pub apps: usize,
}

#[derive(Default)]
pub struct Catalog {
    pub casks: Vec<Cask>,
    by_app: HashMap<String, Vec<usize>>,
    by_bundle: HashMap<String, Vec<usize>>,
    by_name: HashMap<String, Vec<usize>>,
    /// Woher die Daten stammen: „aktuell geladen“, „Zwischenspeicher vom …“
    pub origin: String,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Match {
    /// über App-Namen oder Bundle-ID eindeutig zugeordnet
    Exact,
    /// nur über den Programmnamen zugeordnet
    Name,
}

fn app_names(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) if s.ends_with(".app") => out.push(base(s)),
        Value::Array(a) => a.iter().for_each(|x| app_names(x, out)),
        Value::Object(o) => {
            if let Some(t) = o.get("target") {
                app_names(t, out)
            }
        }
        _ => {}
    }
}

fn strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        _ => {}
    }
}

fn base(p: &str) -> String {
    p.rsplit('/').next().unwrap_or(p).to_lowercase()
}

impl Catalog {
    pub fn build(casks: Vec<Cask>, origin: String) -> Self {
        let mut c = Catalog { casks, origin, ..Default::default() };
        let mut counts = Vec::with_capacity(c.casks.len());
        for (i, k) in c.casks.iter().enumerate() {
            let mut apps = Vec::new();
            let mut bundles = Vec::new();
            for a in &k.artifacts {
                let Some(o) = a.as_object() else { continue };
                if let Some(v) = o.get("app") {
                    app_names(v, &mut apps);
                }
                if let Some(u) = o.get("uninstall").and_then(|u| u.as_array()) {
                    for e in u {
                        if let Some(d) = e.get("delete") {
                            let mut ds = Vec::new();
                            strings(d, &mut ds);
                            apps.extend(ds.iter().filter(|p| p.starts_with("/Applications/") && p.ends_with(".app")).map(|p| base(p)));
                        }
                        if let Some(q) = e.get("quit") {
                            strings(q, &mut bundles);
                        }
                    }
                }
            }
            apps.sort();
            apps.dedup();
            counts.push(apps.len());
            for a in apps {
                c.by_app.entry(a).or_default().push(i);
            }
            bundles.sort();
            bundles.dedup();
            for b in bundles {
                c.by_bundle.entry(b.to_lowercase()).or_default().push(i);
            }
            for n in &k.name {
                c.by_name.entry(n.to_lowercase()).or_default().push(i);
            }
        }
        for (k, n) in c.casks.iter_mut().zip(counts) {
            k.apps = n;
        }
        c
    }

    /// Bester Katalogeintrag für eine installierte App.
    pub fn find(&self, app_file: &str, bundle_id: &str, installed: &str) -> Option<(&Cask, Match)> {
        let pick = |ids: &Vec<usize>| -> Option<&Cask> {
            let inst = crate::version::parse(installed);
            let major = inst.as_ref().map(|v| v.nums[0]);
            let inst_pre = inst.as_ref().is_some_and(|v| v.pre.is_some());
            // Vorabversionen (z. B. powershell@preview) nur, wenn auch die installierte eine ist
            let cands: Vec<&Cask> = ids
                .iter()
                .map(|&i| &self.casks[i])
                .filter(|c| !c.disabled && (inst_pre || crate::version::parse(&c.version).is_none_or(|v| v.pre.is_none())))
                .collect();
            let same_major = |c: &&&Cask| major.is_some() && crate::version::parse(&c.version).map(|v| v.nums[0]) == major;
            // gleiche Hauptversion ohne „@“ > gleiche Hauptversion (z. B. audacity@3) > ohne „@“ > erster
            cands
                .iter()
                .find(|c| same_major(c) && !c.token.contains('@'))
                .or_else(|| cands.iter().find(|c| same_major(c)))
                .or_else(|| cands.iter().find(|c| !c.token.contains('@')))
                .or(cands.first())
                .copied()
        };
        let app = app_file.to_lowercase();
        if let Some(c) = self.by_app.get(&app).and_then(pick) {
            return Some((c, Match::Exact));
        }
        if let Some(ids) = self.by_bundle.get(&bundle_id.to_lowercase()) {
            if ids.len() == 1 {
                return Some((&self.casks[ids[0]], Match::Exact));
            }
        }
        let name = app.trim_end_matches(".app");
        if let Some(ids) = self.by_name.get(name) {
            let pre = crate::version::parse(&self.casks[ids[0]].version).is_some_and(|v| v.pre.is_some());
            if ids.len() == 1 && !self.casks[ids[0]].disabled && !pre {
                return Some((&self.casks[ids[0]], Match::Name));
            }
        }
        None
    }

}

fn cache_file(cache_dir: &Path) -> PathBuf {
    cache_dir.join("cask.json")
}

/// Lädt den Katalog: frisch aus dem Netz, wenn der Zwischenspeicher älter als 6 Stunden ist
/// oder `force` gesetzt ist; ohne Netz wird der Zwischenspeicher verwendet.
pub fn load(cache_dir: &Path, force: bool) -> Result<Catalog, String> {
    let file = cache_file(cache_dir);
    let age = std::fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
    let fresh = matches!(age, Some(a) if a < MAX_AGE);
    let mut origin = String::from("Homebrew-Katalog (Zwischenspeicher)");
    if force || !fresh {
        match download() {
            Ok(bytes) => {
                let _ = std::fs::create_dir_all(cache_dir);
                let tmp = file.with_extension("tmp");
                if std::fs::write(&tmp, &bytes).is_ok() {
                    let _ = std::fs::rename(&tmp, &file);
                }
                origin = "Homebrew-Katalog (soeben aktualisiert)".into();
            }
            Err(e) if file.exists() => origin = format!("Homebrew-Katalog (Zwischenspeicher – Aktualisierung fehlgeschlagen: {e})"),
            Err(e) => return Err(format!("Katalog konnte nicht geladen werden: {e}")),
        }
    }
    let data = std::fs::read(&file).map_err(|e| e.to_string())?;
    let casks: Vec<Cask> = serde_json::from_slice(&data).map_err(|e| format!("Katalog unlesbar: {e}"))?;
    Ok(Catalog::build(casks, origin))
}

fn download() -> Result<Vec<u8>, String> {
    let r = ureq::get(URL).timeout(Duration::from_secs(60)).call().map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut r.into_reader().take(100 << 20), &mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cask(token: &str, name: &str, version: &str, artifacts: Value) -> Cask {
        serde_json::from_value(json!({"token": token, "name": [name], "version": version, "artifacts": artifacts})).unwrap()
    }

    #[test]
    fn matching() {
        let c = Catalog::build(
            vec![
                cask("google-chrome", "Google Chrome", "155.0", json!([{"app": ["Google Chrome.app"]}])),
                cask("audacity", "Audacity", "4.0", json!([{"app": ["Audacity.app"]}])),
                cask("audacity@3", "Audacity", "3.7.9", json!([{"app": ["Audacity.app"]}])),
                cask("teamviewer", "TeamViewer", "15.82", json!([{"uninstall": [{"quit": ["com.teamviewer.TeamViewer"], "delete": "/Applications/TeamViewer.app"}]}, {"pkg": ["x.pkg"]}])),
                cask("microsoft-word", "Microsoft Word", "16.114", json!([{"uninstall": [{"quit": "com.microsoft.autoupdate2"}]}])),
                cask("microsoft-excel", "Microsoft Excel", "16.114", json!([{"uninstall": [{"quit": "com.microsoft.autoupdate2"}]}])),
                cask("renamed", "Renamed", "2.0", json!([{"app": ["Src.app", {"target": "Ziel.app"}]}])),
            ],
            "test".into(),
        );
        assert_eq!(c.find("Google Chrome.app", "com.google.Chrome", "155").unwrap().0.token, "google-chrome");
        assert_eq!(c.find("Audacity.app", "x", "3.7.8").unwrap().0.token, "audacity@3");
        assert_eq!(c.find("Audacity.app", "x", "4.0").unwrap().0.token, "audacity");
        assert_eq!(c.find("TeamViewer.app", "com.teamviewer.TeamViewer", "15").unwrap().0.token, "teamviewer");
        let (w, m) = c.find("Microsoft Word.app", "com.microsoft.Word", "16").unwrap();
        assert_eq!((w.token.as_str(), m), ("microsoft-word", Match::Name)); // autoupdate2 mehrdeutig → Name
        assert_eq!(c.find("Ziel.app", "y", "1").unwrap().0.token, "renamed");
        assert!(c.find("Unbekannt.app", "z", "1").is_none());
    }
}
