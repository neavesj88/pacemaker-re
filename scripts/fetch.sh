#!/usr/bin/env bash
# Fetch the original Tonium/community artifacts into vendor/ (gitignored: we link to these, never redistribute them).
# Idempotent: skips files already present, then records SHA-256s so analysis notes can cite exact inputs.
set -euo pipefail
cd "$(dirname "$0")/.."
UA="Mozilla/5.0 (Windows NT 10.0; Win64; x64)"
PM=https://pacemakerdevice.org/downloads
SF=https://sourceforge.net/projects/pmosc/files
is_html() { head -c 15 "$1" | grep -qi '<!doctype html'; }
fetch() { # dir name url
  mkdir -p "vendor/$1"
  if [[ -s "vendor/$1/$2" ]] && ! is_html "vendor/$1/$2"; then echo "have $1/$2"; return; fi
  echo "get  $1/$2"
  # SourceForge serves an HTML interstitial to browser UAs and a real 302 to tool UAs
  local ua="$UA"; [[ $3 == *sourceforge.net* ]] && ua="Wget/1.21.4"
  curl -fL --retry 3 -A "$ua" -o "vendor/$1/$2.part" "$3"
  if is_html "vendor/$1/$2.part"; then echo "error: got an HTML page for $1/$2" >&2; rm -f "vendor/$1/$2.part"; return 1; fi
  mv "vendor/$1/$2.part" "vendor/$1/$2"
}
for fw in 13557 13606814 14108 14142 16219 146061537 180096069; do
  fetch firmware "dfw_$fw.pfw" "$PM/firmware/dfw_$fw.pfw"
done
fetch editor-win "Pacemaker Setup.exe" "$PM/editor/win/PME-Win-2.0.2.14170/Pacemaker%20Setup.exe"
fetch editor-mac PME-Mac-2.0.2.14170.dmg "$PM/editor/mac/PME-Mac-2.0.2.14170/PME-Mac-2.0.2.14170.dmg"
fetch pmupdater "PMUpdater v1.04 Setup.exe" "$PM/PMUpdater/PMUpdater%20v1.04%20Setup.exe"
fetch docs Eng_Pacemaker_Manual_2.0.pdf "$PM/Documents/Eng_Pacemaker_Manual_2.0.pdf"
fetch restore PMD-Restore-14142.exe "$SF/tools/PMD-Restore-14142.exe/download"
fetch restore PMD-Restore-14142.dmg "$SF/tools/PMD-Restore-14142.dmg/download"
fetch gpl pmib.tar.bz2 "$SF/system/pmib.tar.bz2/download"
fetch gpl imx31_ads_20070511-rel4-ltib-tonium.tar.bz2 "$SF/system/imx31_ads_20070511-rel4-ltib-tonium.tar.bz2/download"
(cd vendor && find . -type f ! -name '*.part' ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
cat vendor/SHA256SUMS
