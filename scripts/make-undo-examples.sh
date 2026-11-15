#!/usr/bin/env bash
# A throwaway repository with its own local "remote", to try GitCRAFT's
# undo and redo and the history of a branch (task 11). Nothing else is
# touched.
#
#   ~/gitcraft-deshacer             the repository (open it in GitCRAFT)
#   ~/gitcraft-deshacer-remoto.git  its "origin" (a local bare repository)
#
# main, oldest first: Base · Añade la guía · Cambia la línea dos
# (origin/main is here) · Solo en local (not pushed). Before the last one
# there was a "Commit perdido" that a reset --hard took out of main: the
# history of main still has it, marked "solo en este historial".
# Branches: feature (from "Añade la guía", two commits: to merge or
# rebase), vieja (to delete and get back). Tag v1.0 on "Añade la guía".
# Left uncommitted on main: a change to README.md and an untracked
# notas.txt.
#
# Usage:
#   scripts/make-undo-examples.sh             create both
#   scripts/make-undo-examples.sh --replace   same, moving existing ones to the Trash first
#
# Base directory: $GITCRAFT_SANDBOX_DIR (default: $HOME). Nothing is ever
# deleted: with --replace, existing directories go to the macOS Trash, after
# asking.

set -euo pipefail

base="${GITCRAFT_SANDBOX_DIR:-$HOME}"
repo="$base/gitcraft-deshacer"
remote="$base/gitcraft-deshacer-remoto.git"

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
	sed -n '2,24p' "$0" >&2
	exit 1
fi

git init --quiet --bare --initial-branch=main "$remote"
git init --quiet --initial-branch=main "$repo"
git -C "$repo" remote add origin "$remote"

# --- main --------------------------------------------------------------------
printf 'uno\ndos\ntres\n' >"$repo/lista.txt"
printf '# Deshacer y rehacer\n' >"$repo/README.md"
commit_all "Base"
printf 'Guía del proyecto\n' >"$repo/guia.txt"
commit_all "Añade la guía"
guide=$(git -C "$repo" rev-parse HEAD)
script_git -C "$repo" tag -a -m "Versión 1.0" v1.0
sed -i.bak -e 's/^dos$/dos (main)/' "$repo/lista.txt"
rm "$repo/lista.txt.bak"
commit_all "Cambia la línea dos"
git -C "$repo" push --quiet --set-upstream origin main

# A commit that a reset --hard takes out of main: only the branch history
# (reflog) keeps it.
printf 'se perderá\n' >"$repo/perdido.txt"
commit_all "Commit perdido"
script_git -C "$repo" reset --quiet --hard HEAD~1
printf 'solo aquí\n' >"$repo/local.txt"
commit_all "Solo en local"

# --- branches -----------------------------------------------------------------
git -C "$repo" switch --quiet -c feature "$guide"
printf 'función nueva\n' >"$repo/funcion.txt"
commit_all "Función: añade funcion.txt"
printf 'función nueva\ncon pruebas\n' >"$repo/funcion.txt"
commit_all "Función: añade pruebas"

git -C "$repo" switch --quiet -c vieja "$guide"
printf 'rama vieja\n' >"$repo/vieja.txt"
commit_all "Vieja: añade vieja.txt"

git -C "$repo" switch --quiet main

# --- uncommitted work ---------------------------------------------------------
printf '# Deshacer y rehacer\n\nUn cambio sin confirmar.\n' >"$repo/README.md"
printf 'mis notas, sin seguimiento\n' >"$repo/notas.txt"

cat <<EOF
Ready (open the first one in GitCRAFT):
  repository:       $repo
  its remote:       $remote

Start again at any time with: $0 --replace
EOF
