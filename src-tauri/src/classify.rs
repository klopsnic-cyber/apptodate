//! Einstufung: wichtig (sicherheitsrelevant), empfohlen (neue Hauptversion, Laufzeitumgebungen), optional.

use crate::scan::Item;
use crate::version;

/// (Kategorie, sicherheitsrelevant, Suchbegriffe in Token/Name/Bundle-ID)
const CATEGORIES: &[(&str, bool, &[&str])] = &[
    ("Browser", true, &["chrome", "firefox", "brave", "edge", "opera", "vivaldi", "arc", "chromium", "tor-browser", "torbrowser", "orion", "safari", "zen-browser", "waterfox", "librewolf"]),
    ("Fernzugriff", true, &["anydesk", "teamviewer", "rustdesk", "splashtop", "parsec", "jump-desktop", "remote", "vnc", "rdp", "royal-tsx", "royaltsx", "pcvisit", "meshcommander", "chrome-remote"]),
    ("VPN & Netzwerk", true, &["tunnelblick", "openvpn", "wireguard", "forticlient", "cisco", "globalprotect", "viscosity", "tailscale", "nordvpn", "protonvpn", "mullvad", "zerotier", "lanscan", "wireshark"]),
    ("Passwörter", true, &["keeper", "1password", "bitwarden", "lastpass", "dashlane", "keepassxc", "keepass", "enpass", "strongbox", "proton-pass"]),
    ("Kommunikation", true, &["zoom", "teams", "slack", "webex", "signal", "whatsapp", "telegram", "discord", "skype", "element", "threema", "pascom", "thunderbird", "outlook", "spark", "airmail", "teamspeak", "mumble"]),
    ("Office & PDF", true, &["microsoft-word", "microsoft-excel", "microsoft-powerpoint", "microsoft-onenote", "microsoft word", "microsoft excel", "microsoft powerpoint", "microsoft onenote", "libreoffice", "onlyoffice", "acrobat", "adobe-reader", "pdf", "pdfgear", "pdfexpert", "pdf-expert", "foxit", "openoffice"]),
    ("Dateien & Sync", true, &["cyberduck", "filezilla", "transmit", "qbittorrent", "transmission", "dropbox", "onedrive", "google-drive", "googledrive", "synology", "nextcloud", "owncloud", "keka", "the-unarchiver", "betterzip", "winrar"]),
    ("Laufzeit & Entwicklung", false, &["java", "temurin", "zulu", "openjdk", "docker", "node", "python", "ollama", "virtualbox", "vmware", "utm", "parallels", "iterm", "visual-studio-code", "vscode", "powershell", "git"]),
];

/// sicherheitsrelevante Kommandozeilen-Pakete
const SECURITY_FORMULAE: &[&str] = &["openssl", "openssl@3", "curl", "openssh", "gnupg", "git", "libssh2", "gnutls", "ca-certificates", "sudo", "wget", "python@3", "node", "nmap"];

pub fn rate(it: &mut Item) {
    let hay = format!(
        "{} {} {}",
        it.brew_token.as_deref().unwrap_or(""),
        it.name.to_lowercase(),
        it.bundle_id.as_deref().unwrap_or("").to_lowercase()
    );
    if it.category.is_none() {
        let words: Vec<&str> = hay.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
        // kurze Begriffe nur als ganzes Wort („arc“ ≠ „search“), lange auch als Wortteil („teamviewerqs“)
        let hit = |k: &str| {
            if k.contains(['-', ' ']) {
                hay.contains(k)
            } else {
                words.iter().any(|w| *w == k || (k.len() >= 6 && w.contains(k)))
            }
        };
        it.category = CATEGORIES.iter().find(|(_, _, keys)| keys.iter().any(|k| hit(k))).map(|c| c.0);
    }
    let security = match it.kind {
        "formula" => SECURITY_FORMULAE.iter().any(|f| it.name == *f || it.name.starts_with(&format!("{f}@"))),
        "macos" => true,
        _ => CATEGORIES.iter().any(|(c, sec, _)| *sec && Some(*c) == it.category),
    };
    match it.status {
        "update" => {
            let latest = it.latest.clone().unwrap_or_default();
            let major = version::is_major_jump(&it.installed, &latest);
            if it.kind == "macos" {
                let t = it.name.to_lowercase();
                if t.contains("security") || t.contains("sicherheit") || t.contains("macos") || t.contains("safari") || t.contains("xprotect") {
                    it.severity = Some("wichtig");
                    it.reasons.insert(0, "Apple-Update – enthält in der Regel Sicherheitskorrekturen.".into());
                } else {
                    it.severity = Some("empfohlen");
                    it.reasons.insert(0, "Ausstehendes Apple-Update.".into());
                }
            } else if security {
                it.severity = Some("wichtig");
                let what = it.category.unwrap_or("Sicherheitsrelevantes Paket");
                it.reasons.insert(0, format!("{what}: verarbeitet Daten aus dem Netz – Updates schließen hier häufig Sicherheitslücken."));
            } else if major {
                it.severity = Some("empfohlen");
                it.reasons.insert(0, "Neue Hauptversion verfügbar – ältere Versionen erhalten oft keine Korrekturen mehr.".into());
            } else if it.category == Some("Laufzeit & Entwicklung") {
                it.severity = Some("empfohlen");
                it.reasons.insert(0, "Laufzeitumgebung bzw. Entwicklerwerkzeug – andere Programme bauen darauf auf.".into());
            } else {
                it.severity = Some("optional");
                it.reasons.insert(0, "Fehlerkorrekturen und Verbesserungen.".into());
            }
            if major && it.severity == Some("wichtig") {
                it.reasons.push("Neue Hauptversion – Änderungen vorher kurz prüfen.".into());
            }
        }
        "notice" => it.severity = Some(if security { "wichtig" } else { "empfohlen" }),
        _ => {}
    }
    if it.uncertain && it.status == "update" {
        it.reasons.push("Zuordnung nur über den Programmnamen – Version vor dem Update bitte prüfen.".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, token: &str, bundle: &str) -> Item {
        let mut it = crate::scan::test_item();
        it.name = name.into();
        it.brew_token = Some(token.into()).filter(|t: &String| !t.is_empty());
        it.bundle_id = Some(bundle.into());
        it
    }

    #[test]
    fn categories_use_word_boundaries() {
        let cat = |n, t, b| {
            let mut it = item(n, t, b);
            rate(&mut it);
            it.category
        };
        assert_eq!(cat("Google Chrome", "google-chrome", "com.google.Chrome"), Some("Browser"));
        assert_eq!(cat("Brave Browser", "brave-browser", "com.brave.Browser"), Some("Browser"));
        assert_eq!(cat("TeamViewerQS", "teamviewer-quicksupport", "com.teamviewer.TeamViewerQS"), Some("Fernzugriff"));
        assert_eq!(cat("Knowledge Base", "", "com.example.knowledge"), None); // „edge“ nicht als Wortteil
        assert_eq!(cat("Digital Color Meter", "", "com.example.digital"), None); // „git“ nicht als Wortteil
        assert_eq!(cat("PDFgear", "", "com.pdfeditor.pdfeditormac"), Some("Office & PDF"));
        assert_eq!(cat("Pinta", "pinta", "com.github.pinta"), None); // „git“ ≠ „github“
        assert_eq!(cat("Microsoft Word", "microsoft-word", "com.microsoft.Word"), Some("Office & PDF"));
    }

    #[test]
    fn severity() {
        let mut it = item("AnyDesk", "anydesk", "com.philandro.anydesk");
        it.status = "update";
        it.installed = "9.8.0".into();
        it.latest = Some("9.8.1".into());
        rate(&mut it);
        assert_eq!(it.severity, Some("wichtig"));

        let mut it = item("Rectangle", "rectangle", "com.knollsoft.Rectangle");
        it.status = "update";
        it.installed = "0.95".into();
        it.latest = Some("2.0.3".into());
        rate(&mut it);
        assert_eq!(it.severity, Some("empfohlen"));

        let mut it = item("Pinta", "pinta", "com.github.pinta");
        it.status = "update";
        it.installed = "3.1.1".into();
        it.latest = Some("3.1.2".into());
        rate(&mut it);
        assert_eq!(it.severity, Some("optional"));
    }
}
