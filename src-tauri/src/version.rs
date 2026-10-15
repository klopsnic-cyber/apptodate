//! Versionsvergleich für sehr unterschiedliche Versionsschemata der Hersteller.
//! Beispiele aus der Praxis: „8.0.1 (build 6301)“, „9.5.4,45528“, „0.11.13-dev.2“, „Version 0.9.5“,
//! Chromium-Präfixe wie Brave „154.1.96.61“ gegenüber „1.97.57.0“.

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq)]
pub struct Ver {
    pub nums: Vec<u64>,
    /// Vorabversion (alpha/beta/rc/dev): (Rang, Nummer). Keine Vorabversion ist größer als jede Vorabversion.
    pub pre: Option<(u8, u64)>,
}

fn pre_rank(s: &str) -> Option<u8> {
    match s.to_ascii_lowercase().as_str() {
        "dev" | "nightly" | "snapshot" => Some(0),
        "a" | "alpha" => Some(1),
        "b" | "beta" => Some(2),
        "pre" | "preview" | "rc" => Some(3),
        _ => None,
    }
}

pub fn parse(raw: &str) -> Option<Ver> {
    let s = raw.split(',').next()?.trim();
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let s: String = s[start..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
        .collect();
    let s = s.split('+').next()?; // Build-Metadaten ignorieren
    let (main, pre_part) = match s.find(['-', '_']) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let mut nums = Vec::new();
    let mut pre = None;
    for part in main.split('.') {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            if let Some(r) = pre_rank(part.trim_end_matches(|c: char| c.is_ascii_digit())) {
                let n = part.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
                pre = Some((r, n));
            }
            break;
        }
        nums.push(digits.parse().ok()?);
        let rest = &part[digits.len()..];
        if !rest.is_empty() {
            // „3b“, „2rc1“ → Vorabversion; sonstige Buchstaben („227k“) ignorieren
            let letters: String = rest.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            if let Some(r) = pre_rank(&letters) {
                let n = rest[letters.len()..].parse().unwrap_or(0);
                pre = Some((r, n));
            }
            break;
        }
    }
    if let (None, Some(p)) = (pre, pre_part) {
        let letters: String = p.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        if let Some(r) = pre_rank(&letters) {
            let n = p.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
            pre = Some((r, n));
        }
    }
    (!nums.is_empty()).then_some(Ver { nums, pre })
}

/// Vergleich, ob `latest` neuer als `installed` ist.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Cmp {
    Newer,
    Same,
    Older,
    /// Versionsschemata passen offensichtlich nicht zusammen – keine Aussage möglich.
    Unknown,
}

pub fn compare(installed: &str, latest: &str) -> Cmp {
    let (Some(mut i), Some(l)) = (parse(installed), parse(latest)) else { return Cmp::Unknown };
    // Chromium-Präfix (z. B. Brave „154.1.96.61“ vs. „1.97.57.0“): erste Stelle verwerfen
    if i.nums.len() >= 4 && i.nums[0] >= 10 * l.nums[0].max(1) && i.nums.len() > 1 {
        let tail = Ver { nums: i.nums[1..].to_vec(), pre: i.pre };
        if tail.nums[0] <= l.nums[0] * 2 + 1 {
            i = tail;
        }
    }
    // Plausibilität: Hauptversionen um mehr als Faktor 10 auseinander → anderes Schema
    let (a, b) = (i.nums[0].max(1), l.nums[0].max(1));
    if a > b * 10 + 10 || b > a * 10 + 10 {
        return Cmp::Unknown;
    }
    let n = i.nums.len().min(l.nums.len());
    for k in 0..n {
        match l.nums[k].cmp(&i.nums[k]) {
            Ordering::Greater => return Cmp::Newer,
            Ordering::Less => return Cmp::Older,
            Ordering::Equal => {}
        }
    }
    // „1.6“ → „1.6.1“ ist neuer; ein angehängtes Build-Segment („26.173.0906“ → „26.173.0906.0008“) nicht
    if l.nums.len() > n && n < 3 && l.nums[n..].iter().any(|&x| x > 0) {
        return Cmp::Newer;
    }
    // weniger genaue Angabe beim Katalog („15“ vs. „15.80.4“) gilt als gleich
    match (i.pre, l.pre) {
        (Some(_), None) if l.nums.len() >= i.nums.len() => Cmp::Newer,
        (None, Some(_)) => Cmp::Same,
        (Some(x), Some(y)) => match y.cmp(&x) {
            Ordering::Greater => Cmp::Newer,
            Ordering::Less => Cmp::Older,
            Ordering::Equal => Cmp::Same,
        },
        _ => Cmp::Same,
    }
}

/// Neue Hauptversion (erste Stelle größer, bei 0.x die zweite Stelle)?
pub fn is_major_jump(installed: &str, latest: &str) -> bool {
    let (Some(i), Some(l)) = (parse(installed), parse(latest)) else { return false };
    if compare(installed, latest) != Cmp::Newer {
        return false;
    }
    if l.nums[0] > i.nums[0] {
        // Chromium-Präfix-Fall nicht als Hauptversion werten
        return i.nums.len() < 4 || i.nums[0] < 10 * l.nums[0].max(1);
    }
    l.nums[0] == 0 && i.nums[0] == 0 && l.nums.get(1) > i.nums.get(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Cmp::*;

    #[test]
    fn real_world_cases() {
        let cases = [
            ("155.0.8059.40", "155.0.8059.80", Newer), // Chrome
            ("154.1.96.61", "1.97.57.0", Newer),       // Brave mit Chromium-Präfix
            ("8.0.1 (build 6301)", "9.0.1,6491", Newer), // Tunnelblick
            ("9.4.1", "9.5.4,45528", Newer),           // Cyberduck
            ("2.37337.1", "2.37337.1,ba51ab", Same),   // Claude mit Hash
            ("0.11.13-dev.2", "0.11.12", Older),       // Ice
            ("1.6.4", "1.1.4", Older),                 // Caffeine (anderes Projekt)
            ("15.80.4", "15", Same),                   // TeamViewer QS, ungenaue Katalogangabe
            ("1.9.3", "1.9.3b", Same),                 // Shottr
            ("3.20-beta.9", "4.0.10", Newer),          // SuperDuper
            ("3.20-beta.9", "3.20", Newer),
            ("1.6", "1.6.1", Newer),
            ("1.6.0", "1.6", Same),
            ("26.173.0906", "26.173.0906.0008", Same), // OneDrive
            ("16.114", "16.114.26101123", Newer),      // Office: Kurzversion → Build entscheidet (scan.rs)
            ("Version 0.9.5", "0.9.6", Newer),
            ("227k", "227", Same),
            ("8.0.3", "4.0.3,17892", Older),           // Synology: Katalog nutzt anderes Schema
            ("", "1.0", Unknown),
            ("1.0", "latest", Unknown),
            ("2024.1", "24.2", Unknown),
        ];
        for (i, l, want) in cases {
            assert_eq!(compare(i, l), want, "{i} → {l}");
        }
    }

    #[test]
    fn major() {
        assert!(is_major_jump("0.95", "2.0.3"));
        assert!(is_major_jump("0.23.3", "0.40.4"));
        assert!(!is_major_jump("155.0.8059.40", "155.0.8059.80"));
        assert!(!is_major_jump("154.1.96.61", "1.97.57.0"));
        assert!(!is_major_jump("2.0", "1.0"));
    }
}
