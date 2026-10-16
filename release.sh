#!/bin/sh
# Neue Version veröffentlichen: ./release.sh 0.2.0 "Was ist neu"
# Setzt die Version, ergänzt den CHANGELOG, committet, taggt und pusht.
# GitHub Actions baut dann das dmg – die installierten Apps holen sich das Update selbst.
set -e
V="$1"; NOTE="$2"
[ -n "$V" ] && [ -n "$NOTE" ] || { echo "Aufruf: ./release.sh 0.2.0 \"Was ist neu\""; exit 1; }
echo "$V" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || { echo "Version muss X.Y.Z sein"; exit 1; }
git rev-parse "v$V" >/dev/null 2>&1 && { echo "v$V gibt es schon"; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "Erst alle Änderungen committen"; exit 1; }
cd "$(dirname "$0")"
sed -i '' "s/^  \"version\": \".*\"/  \"version\": \"$V\"/" package.json src-tauri/tauri.conf.json
sed -i '' "3s/^version = \".*\"/version = \"$V\"/" src-tauri/Cargo.toml
npm install --package-lock-only --silent
(cd src-tauri && cargo metadata --format-version 1 >/dev/null)  # Cargo.lock nachziehen
printf '# Changelog\n\n## %s\n- %s\n%s\n' "$V" "$NOTE" "$(tail -n +2 CHANGELOG.md)" > CHANGELOG.md
git add -A
git commit -qm "Version $V"
git tag "v$V"
git push -q origin HEAD "v$V"
echo "v$V gepusht – Build: https://github.com/klopsnic-cyber/apptodate/actions"
