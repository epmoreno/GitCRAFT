#!/usr/bin/env bash
# Throwaway repositories to try GitCRAFT's visual interactive rebase
# (task 13): "Interactive rebase from here…" in a commit's menu. Nothing
# else is touched.
#
#   ~/gitcraft-rebase              the repository (open it in GitCRAFT)
#   ~/gitcraft-rebase-remoto.git   its "origin" (a local bare repository)
#   ~/gitcraft-rebase-a-medias     a merge stopped on a conflict: the entry
#                                  is disabled there
#
# Branches of ~/gitcraft-rebase, each for one case (all from "Base"):
#   main                 (current) five simple commits, each in its own file:
#                        reorder, edit messages, combine and drop freely
#   choca-al-reordenar   three commits on the same line: moving the newest
#                        below the middle one stops with a conflict
#   queda-vacio          "Revierte «Sube la versión a 2»" becomes empty
#                        (git leaves it out) if "Sube la versión a 2" is
#                        dropped
#   publicada            three commits already on origin and one that is
#                        not: rewriting them warns about a forced push
#   con-merge            has a merge: the view explains it is not offered
# The working tree is left clean, so you can switch between branches.
#
# Usage:
#   scripts/make-rebase-examples.sh             create them
#   scripts/make-rebase-examples.sh --replace   same, moving existing ones to the Trash first
#
# Base directory: $GITCRAFT_SANDBOX_DIR (default: $HOME). Nothing is ever
# deleted: with --replace, existing directories go to the macOS Trash, after
# asking.

set -euo pipefail

base="${GITCRAFT_SANDBOX_DIR:-$HOME}"
repo="$base/gitcraft-rebase"
remote="$base/gitcraft-rebase-remoto.git"
halfway="$base/gitcraft-rebase-a-medias"

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

# Commits everything in repository $1 with message $2 (and body $3).
commit_all() {
	git -C "$1" add --all
	if [[ -n "${3:-}" ]]; then
		script_git -C "$1" commit --quiet -m "$2" -m "$3"
	else
		script_git -C "$1" commit --quiet -m "$2"
	fi
}

# A new branch from $2, checked out.
branch_from() {
	git -C "$repo" switch --quiet -c "$1" "$2"
}

existing=()
for path in "$repo" "$remote" "$halfway"; do
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
	sed -n '2,29p' "$0" >&2
	exit 1
fi

git init --quiet --bare --initial-branch=main "$remote"
git init --quiet --initial-branch=main "$repo"
git -C "$repo" remote add origin "$remote"

# --- Base ---------------------------------------------------------------------
printf '# Rebase interactivo de prueba\n' >"$repo/README.md"
commit_all "$repo" "Base"
start=$(git -C "$repo" rev-parse HEAD)

# --- main: five independent commits -------------------------------------------
printf 'Guía del proyecto\n' >"$repo/guia.txt"
commit_all "$repo" "Añade la guía" "Primera versión de la guía."
printf 'borrar antes de publicar\n' >"$repo/temporal.txt"
commit_all "$repo" "Prueba temporal"
printf 'búsqueda por archivo\n' >"$repo/busqueda.txt"
commit_all "$repo" "wip"
printf 'tooltip: primera versión\n' >"$repo/tooltip.txt"
commit_all "$repo" "Ajuste del tooltip"
printf 'estilos del tooltip\n' >"$repo/tooltip.css"
commit_all "$repo" "Arregla el tooltip" "Corrige el color y el margen."

# --- a reorder that conflicts -------------------------------------------------
branch_from choca-al-reordenar "$start"
printf 'uno\ndos\ntres\n' >"$repo/lista.txt"
commit_all "$repo" "Crea la lista"
printf 'uno\nDOS\ntres\n' >"$repo/lista.txt"
commit_all "$repo" "Cambia la dos"
printf 'uno\nDos, definitiva\ntres\n' >"$repo/lista.txt"
commit_all "$repo" "Cambia la dos otra vez"

# --- a commit that becomes empty -----------------------------------------------
branch_from queda-vacio "$start"
printf 'versión 1\n' >"$repo/version.txt"
commit_all "$repo" "Añade version.txt"
printf 'versión 2\n' >"$repo/version.txt"
commit_all "$repo" "Sube la versión a 2"
printf 'versión 1\n' >"$repo/version.txt"
commit_all "$repo" "Revierte «Sube la versión a 2»"
printf 'notas de la versión\n' >"$repo/notas.txt"
commit_all "$repo" "Añade las notas"

# --- published commits ----------------------------------------------------------
branch_from publicada "$start"
printf 'publicada\n' >"$repo/publicada.txt"
commit_all "$repo" "Publicada: añade publicada.txt"
printf 'publicada\nsegunda línea\n' >"$repo/publicada.txt"
commit_all "$repo" "Publicada: amplía publicada.txt"
printf 'otra cosa\n' >"$repo/otra.txt"
commit_all "$repo" "Publicada: añade otra.txt"
git -C "$repo" push --quiet -u origin publicada
printf 'sin subir\n' >"$repo/local.txt"
commit_all "$repo" "Local: todavía sin subir"

# --- a merge in the range -------------------------------------------------------
branch_from con-merge "$start"
printf 'con merge\n' >"$repo/con-merge.txt"
commit_all "$repo" "Con merge: añade con-merge.txt"
branch_from lado "$start"
printf 'lado\n' >"$repo/lado.txt"
commit_all "$repo" "Lado: añade lado.txt"
git -C "$repo" switch --quiet con-merge
script_git -C "$repo" merge --quiet --no-ff -m "Merge lado" lado
git -C "$repo" branch --quiet -D lado
printf 'después del merge\n' >"$repo/despues.txt"
commit_all "$repo" "Con merge: después del merge"

git -C "$repo" switch --quiet main

# --- a merge left halfway -------------------------------------------------------
git init --quiet --initial-branch=main "$halfway"
printf 'uno\ndos\ntres\n' >"$halfway/lista.txt"
commit_all "$halfway" "Base"
git -C "$halfway" switch --quiet -c otra
printf 'uno\ndos (otra)\ntres\n' >"$halfway/lista.txt"
commit_all "$halfway" "Otra: cambia la dos"
git -C "$halfway" switch --quiet main
printf 'uno\ndos (main)\ntres\n' >"$halfway/lista.txt"
commit_all "$halfway" "Main: cambia la dos"
printf 'más\n' >"$halfway/mas.txt"
commit_all "$halfway" "Main: añade mas.txt"
# Stops on the conflict in lista.txt, as intended.
script_git -C "$halfway" merge --quiet otra >/dev/null 2>&1 || true

cat <<EOF
Ready (open them in GitCRAFT):
  repository:             $repo
  its remote:             $remote
  a merge left halfway:   $halfway

Start again at any time with: $0 --replace
EOF
