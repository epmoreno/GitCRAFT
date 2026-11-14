#!/usr/bin/env bash
# A throwaway repository with its own local "remote", to try GitCRAFT's
# merge and rebase of branches (task 8): from the menus and dragging one
# branch onto another. Nothing else is touched.
#
#   ~/gitcraft-ramas             the repository (open it in GitCRAFT)
#   ~/gitcraft-ramas-remoto.git  its "origin" (a local bare repository)
#
# main, oldest first: Base · Añade la guía · Cambia la línea dos ·
# Actualiza la guía. origin/main has one more commit, "Otra persona: añade
# contribuir.txt", that was not fetched yet ("Fetch primero" brings it).
#
# Branches, each for one case:
#   adelantada      (from main)            fast-forward: main has nothing of its own
#   trae-notas      (from main)            fast-forward that would overwrite the
#                                          untracked notas.txt
#   funcion-limpia  (from "Añade la guía") merge with a merge commit, no conflicts
#   funcion-choca   (from "Añade la guía") merge that conflicts in lista.txt
#   rebase-limpio   (from "Añade la guía") rebase onto main, no conflicts
#   rebase-choca    (from "Añade la guía") rebase that stops at commit 2 of 3
#   publicada       (from "Añade la guía") also on origin: rebasing it rewrites
#                                          published commits
#   con-merge       (from "Añade la guía") has a merge: rebase is not offered
# Left uncommitted on main: a change to README.md (a rebase needs it out of
# the way) and an untracked notas.txt.
#
# Usage:
#   scripts/make-merge-examples.sh             create both
#   scripts/make-merge-examples.sh --replace   same, moving existing ones to the Trash first
#
# Base directory: $GITCRAFT_SANDBOX_DIR (default: $HOME). Nothing is ever
# deleted: with --replace, existing directories go to the macOS Trash, after
# asking.

set -euo pipefail

base="${GITCRAFT_SANDBOX_DIR:-$HOME}"
repo="$base/gitcraft-ramas"
remote="$base/gitcraft-ramas-remoto.git"

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

# A new branch from $2, checked out.
branch_from() {
	git -C "$repo" switch --quiet -c "$1" "$2"
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
	sed -n '2,32p' "$0" >&2
	exit 1
fi

git init --quiet --bare --initial-branch=main "$remote"
git init --quiet --initial-branch=main "$repo"
git -C "$repo" remote add origin "$remote"

# --- main --------------------------------------------------------------------
printf 'uno\ndos\ntres\ncuatro\ncinco\n' >"$repo/lista.txt"
printf '# Ramas de prueba\n' >"$repo/README.md"
commit_all "Base"
printf 'Guía del proyecto\n' >"$repo/guia.txt"
commit_all "Añade la guía"
guide=$(git -C "$repo" rev-parse HEAD)
sed -i.bak -e 's/^dos$/dos (main)/' "$repo/lista.txt"
rm "$repo/lista.txt.bak"
commit_all "Cambia la línea dos"
printf 'Guía del proyecto\nCon un apartado nuevo\n' >"$repo/guia.txt"
commit_all "Actualiza la guía"

# --- fast-forwards ------------------------------------------------------------
branch_from adelantada main
printf 'paso uno\n' >"$repo/adelantada-1.txt"
commit_all "Adelantada: paso uno"
printf 'paso dos\n' >"$repo/adelantada-2.txt"
commit_all "Adelantada: paso dos"

branch_from trae-notas main
printf 'notas de la rama\n' >"$repo/notas.txt"
commit_all "Trae notas.txt"

# --- merges -------------------------------------------------------------------
branch_from funcion-limpia "$guide"
printf 'función nueva\n' >"$repo/funcion.txt"
commit_all "Función: añade funcion.txt"
printf 'función nueva\ncon pruebas\n' >"$repo/funcion.txt"
commit_all "Función: añade pruebas"

branch_from funcion-choca "$guide"
sed -i.bak -e 's/^dos$/dos (funcion)/' "$repo/lista.txt"
rm "$repo/lista.txt.bak"
commit_all "Función: cambia la línea dos"

# --- rebases ------------------------------------------------------------------
branch_from rebase-limpio "$guide"
printf 'limpio\n' >"$repo/limpio.txt"
commit_all "Limpio: añade limpio.txt"
printf 'limpio\notra línea\n' >"$repo/limpio.txt"
commit_all "Limpio: amplía limpio.txt"

branch_from rebase-choca "$guide"
printf 'primero\n' >"$repo/choca.txt"
commit_all "Choca 1: añade choca.txt"
sed -i.bak -e 's/^dos$/dos (choca)/' "$repo/lista.txt"
rm "$repo/lista.txt.bak"
commit_all "Choca 2: cambia la línea dos"
printf 'tercero\n' >"$repo/choca-3.txt"
commit_all "Choca 3: añade choca-3.txt"

branch_from publicada "$guide"
printf 'publicada\n' >"$repo/publicada.txt"
commit_all "Publicada: añade publicada.txt"
printf 'publicada\notra línea\n' >"$repo/publicada.txt"
commit_all "Publicada: amplía publicada.txt"

branch_from con-merge "$guide"
printf 'con merge\n' >"$repo/con-merge.txt"
commit_all "Con merge: añade con-merge.txt"
branch_from lado "$guide"
printf 'lado\n' >"$repo/lado.txt"
commit_all "Lado: añade lado.txt"
git -C "$repo" switch --quiet con-merge
script_git -C "$repo" merge --quiet --no-ff -m "Merge lado" lado
git -C "$repo" branch --quiet -D lado

# --- origin -------------------------------------------------------------------
git -C "$repo" switch --quiet main
git -C "$repo" push --quiet origin main publicada
git -C "$repo" branch --quiet --set-upstream-to=origin/main main
fetched=$(git -C "$repo" rev-parse origin/main)

# Someone else pushes to origin/main; this repository has not fetched it yet.
branch_from otra-persona main
printf 'Cómo contribuir\n' >"$repo/contribuir.txt"
commit_all "Otra persona: añade contribuir.txt"
git -C "$repo" push --quiet origin otra-persona:main
git -C "$repo" switch --quiet main
git -C "$repo" branch --quiet -D otra-persona
# The push moved origin/main here too: put it back where the last fetch saw it.
git -C "$repo" update-ref refs/remotes/origin/main "$fetched"

# --- uncommitted work ---------------------------------------------------------
printf '# Ramas de prueba\n\nUn cambio sin confirmar.\n' >"$repo/README.md"
printf 'mis notas, sin seguimiento\n' >"$repo/notas.txt"

cat <<EOF
Ready (open the first one in GitCRAFT):
  repository:       $repo
  its remote:       $remote

Start again at any time with: $0 --replace
EOF
