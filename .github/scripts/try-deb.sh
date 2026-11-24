#!/usr/bin/env bash
# CI only: installs the .deb, starts the installed app with --smoke, removes
# it and checks that the program is gone.
#   try-deb.sh <file.deb> <version> <architecture>
set -euo pipefail

if [ "${GITHUB_ACTIONS:-}" != "true" ]; then
  echo "this installs and removes GitCRAFT: it only runs in the CI" >&2
  exit 1
fi
deb=$(realpath "$1")
version=$2
arch=$3
package=git-craft

fail() {
  echo "installer: $*" >&2
  exit 1
}
as_root() {
  if [ "$(id -u)" = 0 ]; then "$@"; else sudo "$@"; fi
}
# In the container there is only root, which is no one's desktop session.
as_user() {
  if [ "$(id -u)" = 0 ]; then runuser -u tester -- env HOME=/home/tester "$@"; else "$@"; fi
}

. /etc/os-release
echo "installer: $PRETTY_NAME, $(dpkg --print-architecture)"

dpkg-deb --info "$deb"
dpkg-deb --contents "$deb"
[ "$(dpkg-deb -f "$deb" Package)" = "$package" ] || fail "the package name"
[ "$(dpkg-deb -f "$deb" Version)" = "$version" ] || fail "the version"
[ "$(dpkg-deb -f "$deb" Architecture)" = "$arch" ] || fail "the architecture"
[ "$(dpkg-deb -f "$deb" Maintainer)" = "GitCRAFT" ] || fail "the maintainer"
files=$(dpkg-deb --fsys-tarfile "$deb" | tar -t | sed 's|^\./||; /\/$/d')

if dpkg -s "$package" >/dev/null 2>&1; then fail "already installed"; fi
export DEBIAN_FRONTEND=noninteractive
as_root apt-get update -q
if [ "$(id -u)" = 0 ]; then
  apt-get install -y -q xvfb xauth
  useradd --create-home tester
fi
# A copy apt can read whoever owns the folder.
copy="/tmp/$(basename "$deb")"
cp "$deb" "$copy"
as_root apt-get install -y -q "$copy"
dpkg -s "$package" | grep -qx "Status: install ok installed" ||
  fail "not installed"
[ -x /usr/bin/gitcraft ] || fail "no /usr/bin/gitcraft"
if ldd /usr/bin/gitcraft | grep "not found"; then fail "a library is missing"; fi
grep -qx "Exec=gitcraft" /usr/share/applications/GitCRAFT.desktop ||
  fail "the menu entry"
git --version
echo "installer: installed"

code=0
as_user timeout 300 xvfb-run -a /usr/bin/gitcraft --smoke || code=$?
[ "$code" = 0 ] || fail "the installed app ended with $code"
echo "installer: the installed app started and closed (0)"

# Again over itself, as an update.
as_root apt-get install -y -q --reinstall "$copy"
echo "installer: installed again over itself"

as_root apt-get remove -y -q "$package"
if dpkg -s "$package" >/dev/null 2>&1; then fail "still installed"; fi
while IFS= read -r file; do
  if [ -e "/$file" ] || [ -L "/$file" ]; then fail "left behind: /$file"; fi
done <<< "$files"
if [ -e "/usr/share/doc/$package" ]; then fail "left its documentation"; fi
command -v git >/dev/null || fail "git went away with it"
echo "installer: removed; none of its files is left"

# What stays on purpose: the user's data.
for dir in .config .local/share .cache; do
  as_user sh -c 'if [ -e "$HOME/$1/dev.gitcraft.app" ]; then
    echo "installer: user data: ~/$1/dev.gitcraft.app"; fi' sh "$dir"
done
