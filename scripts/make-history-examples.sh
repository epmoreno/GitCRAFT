#!/usr/bin/env bash
# A throwaway repository with its own local "remote", to try GitCRAFT's
# actions on commits (task 10): cherry-pick, revert, reset and tags.
# Nothing else is touched.
#
#   ~/gitcraft-historial             the repository (open it in GitCRAFT)
#   ~/gitcraft-historial-remoto.git  its "origin" (a local bare repository)
#
# main, oldest first:
#   Base · Añade README (tag v1.0, also on origin) · Merge old-feature (a
#   merge) · Cambia la línea dos · Quita notas.txt (origin/main is here) ·
#   Cambia la línea dos otra vez (only local)
# feature (from "Añade README"):
#   Feature: añade feature.txt (cherry-picks cleanly) ·
#   Feature: cambia la línea dos (conflicts with main)
# Tag v0.9 on "Base" (only local). Left uncommitted: a change to README.md
# and an untracked notas.txt (which reset --hard to before "Quita
# notas.txt" would overwrite).
#
# Usage:
#   scripts/make-history-examples.sh             create both
#   scripts/make-history-examples.sh --replace   same, moving existing ones to the Trash first
#
# Base directory: $GITCRAFT_SANDBOX_DIR (default: $HOME). Nothing is ever
# deleted: with --replace, existing directories go to the macOS Trash, after
# asking.

set -euo pipefail

base="${GITCRAFT_SANDBOX_DIR:-$HOME}"
repo="$base/gitcraft-historial"
remote="$base/gitcraft-historial-remoto.git"

# Commits made by this script use a fixed identity and never run your hooks
# or sign; what you do in the app afterwards uses your own git config.
script_git() {
	GIT_AUTHOR_NAME="Script de prueba" GIT_AUTHOR_EMAIL="script@gitcraft.invalid" \
		GIT_COMMITTER_NAME="Script de prueba" GIT_COMMITTER_EMAIL="script@gitcraft.invalid" \
		git -c core.hooksPath=/dev/null -c commit.gpgSign=false -c tag.gpgSign=false "$@"
}

to_trash() {
	local path="$1"
	if [[ "$(uname)" != "Darwin" ]]; then
		echo "Moving to the Trash is only supported on macOS. Move or remove $path yourself." >&2
		exit 1
	fi
	osascript -e 'on run argv' -e 'tell application "Finder" to delete (POSIX file (item 1 of argv) as alias)' -e 'end run' "$path" >/dev/null
	echo "Moved to the Trash: $path"
}

commit_all() {
	git -C "$repo" add --all
	script_git -C "$repo" commit --quiet -m "$1"
}

existing=()
for path in "$repo" "$remote"; do
	[[ -e "$path" ]] && existing+=("$path")
done
if ((${#existing[@]} > 0)); then
	if [[ "${1:-}" != "--replace" ]]; then
		echo "These already exist; nothing was changed:" >&2
		printf '  %s\n' "${existing[@]}" >&2
		echo "Run with --replace to move them to the Trash and start again." >&2
		exit 1
	fi
	echo "These will be moved to the Trash (recoverable from Finder):"
	printf '  %s\n' "${existing[@]}"
	read -r -p "Continue? [y/N] " answer
	[[ "$answer" == "y" || "$answer" == "Y" ]] || {
		echo "Cancelled."
		exit 1
	}
	for path in "${existing[@]}"; do to_trash "$path"; done
elif [[ -n "${1:-}" ]]; then
	echo "Unknown option: $1" >&2
	sed -n '2,25p' "$0" >&2
	exit 1
fi

git init --quiet --bare --initial-branch=main "$remote"
git init --quiet --initial-branch=main "$repo"
git -C "$repo" remote add origin "$remote"

printf 'uno\ndos\ntres\ncuatro\ncinco\n' >"$repo/lista.txt"
printf 'notas del proyecto\n' >"$repo/notas.txt"
commit_all "Base"
script_git -C "$repo" tag v0.9

printf '# Historial de prueba\n' >"$repo/README.md"
commit_all "Añade README"
script_git -C "$repo" tag -a -m "Versión 1.0" v1.0

# A merge commit (cherry-pick and revert refuse it).
git -C "$repo" switch --quiet -c old-feature
printf 'ayuda\n' >"$repo/ayuda.txt"
commit_all "Añade ayuda.txt"
git -C "$repo" switch --quiet main
script_git -C "$repo" merge --quiet --no-ff -m "Merge old-feature" old-feature
git -C "$repo" branch --quiet -D old-feature

# feature, from "Añade README".
git -C "$repo" switch --quiet -c feature v1.0
printf 'feature\n' >"$repo/feature.txt"
commit_all "Feature: añade feature.txt"
sed -i.bak -e 's/^dos$/dos (feature)/' "$repo/lista.txt"
rm "$repo/lista.txt.bak"
commit_all "Feature: cambia la línea dos"
git -C "$repo" switch --quiet main

sed -i.bak -e 's/^dos$/dos (main)/' "$repo/lista.txt"
rm "$repo/lista.txt.bak"
commit_all "Cambia la línea dos"
git -C "$repo" rm --quiet notas.txt
commit_all "Quita notas.txt"

# origin has main up to here, and v1.0.
git -C "$repo" push --quiet origin main feature refs/tags/v1.0
git -C "$repo" branch --quiet --set-upstream-to=origin/main main

sed -i.bak -e 's/^dos (main)$/dos (main, otra vez)/' "$repo/lista.txt"
rm "$repo/lista.txt.bak"
commit_all "Cambia la línea dos otra vez"

# Uncommitted work.
printf '# Historial de prueba\n\nUn cambio sin confirmar.\n' >"$repo/README.md"
printf 'mis notas, sin seguimiento\n' >"$repo/notas.txt"

cat <<EOF
Ready (open the first one in GitCRAFT):
  repository:       $repo
  its remote:       $remote

Start again at any time with: $0 --replace
EOF
