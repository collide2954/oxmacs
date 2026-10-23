#!/usr/bin/env bash
set -euo pipefail

channel="$1"
built="$2"
site="$3"
url="$4"
keep_nightly=3

shopt -s nullglob

for rpm in "$built"/*/*/*.rpm "$built"/*/*.rpm; do
  release=$(rpm -qp --qf '%{RELEASE}' "$rpm")
  fedora=$(sed -E 's/.*\.fc([0-9]+).*/\1/' <<<"$release")
  arch=$(rpm -qp --qf '%{ARCH}' "$rpm")
  dir="$site/rpm/$channel/fedora/$fedora/$arch"
  mkdir -p "$dir"
  cp "$rpm" "$dir/"
done

if [ "$channel" = nightly ]; then
  for dir in "$site"/rpm/nightly/fedora/*/*/; do
    for rpm in "$dir"*.rpm; do
      printf '%s %s\n' "$(rpm -qp --qf '%{BUILDTIME}' "$rpm")" "$rpm"
    done | sort -rn | tail -n +$((keep_nightly + 1)) | cut -d' ' -f2- | xargs -r rm -f
  done
fi

for dir in "$site"/rpm/"$channel"/fedora/*/*/; do
  createrepo_c --update "$dir"
done

repo() {
  local id="$1" name="$2" path="$3"
  cat <<EOF
[$id]
name=$name for Fedora \$releasever
baseurl=$url/rpm/$path/fedora/\$releasever/\$basearch/
enabled=1
gpgcheck=0
repo_gpgcheck=0
EOF
}

repo oxmacs "oxmacs" stable >"$site/rpm/oxmacs.repo"
repo oxmacs-nightly "oxmacs nightly" nightly >"$site/rpm/oxmacs-nightly.repo"

cat >"$site/index.html" <<EOF
<!doctype html>
<meta charset="utf-8">
<title>oxmacs packages</title>
<h1>oxmacs packages</h1>
<p>Fedora 44 and 45:</p>
<pre>sudo dnf config-manager addrepo --from-repofile=$url/rpm/oxmacs.repo
sudo dnf install oxmacs</pre>
<p>Builds of every change on master: <code>$url/rpm/oxmacs-nightly.repo</code>.</p>
EOF
touch "$site/.nojekyll"
