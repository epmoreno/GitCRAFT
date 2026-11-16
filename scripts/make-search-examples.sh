#!/usr/bin/env bash
# A throwaway repository to try GitCRAFT's history search, file history and
# blame (task 12). Nothing else is touched.
#
#   ~/gitcraft-busqueda   the repository (open it in GitCRAFT)
#
# What it has:
# - 3,000 filler commits ("Ajuste 1" … "Ajuste 3000") by four authors (Ana
#   Pérez, Luis, Marta, Pere), so the search has something to go through.
# - notas.txt: created, changed, renamed with a change to
#   "docs/guía de uso.txt", changed on the branch "rama" (merged), deleted
#   and created again. Left with a staged and an unstaged change.
# - A commit whose message has regular-expression characters:
#   "fix(parser): a+b [beta]", and one whose body says "palabra clave".
# - grande.txt: 25,000 lines (the blame asks first) and imagen.bin
#   (binary: no blame).
# - "Commit perdido": taken out of main by a reset --hard; only the branch
#   history has it (search its hash).
#
# Usage:
#   scripts/make-search-examples.sh             create it
#   scripts/make-search-examples.sh --replace   same, moving the existing one to the Trash first
#
# Base directory: $GITCRAFT_SANDBOX_DIR (default: $HOME). Nothing is ever
# deleted: with --replace, the existing directory goes to the macOS Trash,
# after asking.

set -euo pipefail

base="${GITCRAFT_SANDBOX_DIR:-$HOME}"
repo="$base/gitcraft-busqueda"

# Commits made by this script use fixed identities and never run your hooks
# or sign; what you do in the app afterwards uses your own git config.
script_git() {
	GIT_AUTHOR_NAME="${AUTHOR:-Ana Pérez}" GIT_AUTHOR_EMAIL="${EMAIL:-ana@gitcraft.invalid}" \
		GIT_COMMITTER_NAME="Script de prueba" GIT_COMMITTER_EMAIL="script@gitcraft.invalid" \
		git -C "$repo" -c core.hooksPath=/dev/null -c commit.gpgSign=false "$@"
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
	script_git commit --quiet -m "$@"
}

if [[ -e "$repo" ]]; then
	if [[ "${1:-}" != "--replace" ]]; then
		echo "This already exists; nothing was changed:" >&2
		echo "  $repo" >&2
		echo "Run with --replace to move it to the Trash and start again." >&2
		exit 1
	fi
	echo "This will be moved to the Trash (recoverable from Finder):"
	echo "  $repo"
	read -r -p "Continue? [y/N] " answer
	[[ "$answer" == "y" || "$answer" == "Y" ]] || {
		echo "Cancelled."
		exit 1
	}
	to_trash "$repo"
elif [[ -n "${1:-}" ]]; then
	echo "Unknown option: $1" >&2
	sed -n '2,26p' "$0" >&2
	exit 1
fi

git init --quiet --initial-branch=main "$repo"

# --- filler history (fast-import: a few seconds) ------------------------------
authors=("Ana Pérez:ana" "Luis:luis" "Marta:marta" "Pere:pere")
{
	for i in $(seq 1 3000); do
		entry="${authors[$((i % 4))]}"
		name="${entry%%:*}"
		mail="${entry##*:}@gitcraft.invalid"
		when=$((1600000000 + i * 3600))
		message="Ajuste $i"
		content="ajuste $i"
		echo "commit refs/heads/main"
		echo "mark :$i"
		echo "author $name <$mail> $when +0000"
		echo "committer Script de prueba <script@gitcraft.invalid> $when +0000"
		echo "data ${#message}"
		echo "$message"
		if ((i > 1)); then echo "from :$((i - 1))"; fi
		echo "M 100644 inline relleno/archivo-$((i % 50)).txt"
		echo "data $((${#content} + 1))"
		echo "$content"
		echo
	done
} | LC_ALL=C git -C "$repo" fast-import --quiet
git -C "$repo" checkout --quiet main

# --- a file with a story ------------------------------------------------------
printf 'uno\ndos\ntres\ncuatro\ncinco\n' >"$repo/notas.txt"
commit_all "Crea las notas"
printf 'uno\ndos\ntres\ncuatro\ncinco\nseis\n' >"$repo/notas.txt"
AUTHOR="Luis" EMAIL="luis@gitcraft.invalid" commit_all "Amplía las notas"
mkdir -p "$repo/docs"
git -C "$repo" mv notas.txt "docs/guía de uso.txt"
printf 'uno\ndos\ntres\ncuatro\ncinco\nseis\nsiete\n' >"$repo/docs/guía de uso.txt"
commit_all "Mueve las notas a docs"

git -C "$repo" switch --quiet -c rama
printf 'uno\nDOS\ntres\ncuatro\ncinco\nseis\nsiete\n' >"$repo/docs/guía de uso.txt"
AUTHOR="Marta" EMAIL="marta@gitcraft.invalid" commit_all "En la rama: cambia la dos"
git -C "$repo" switch --quiet main
printf 'otro\n' >"$repo/otro.txt"
commit_all "Otro archivo"
script_git merge --quiet --no-ff -m "Merge rama" rama

# --- search cases -------------------------------------------------------------
printf 'parser\n' >"$repo/parser.txt"
AUTHOR="Pere" EMAIL="pere@gitcraft.invalid" commit_all "fix(parser): a+b [beta]"
printf 'cuerpo\n' >"$repo/cuerpo.txt"
commit_all "Un commit con cuerpo" -m "El cuerpo menciona la palabra clave."

# --- blame cases --------------------------------------------------------------
seq -f 'línea %g' 1 25000 >"$repo/grande.txt"
printf 'PNG\0\001\002' >"$repo/imagen.bin"
commit_all "Archivo grande y uno binario"

# --- a commit only the branch history keeps -----------------------------------
printf 'se perderá\n' >"$repo/perdido.txt"
commit_all "Commit perdido"
lost=$(git -C "$repo" rev-parse --short HEAD)
script_git reset --quiet --hard HEAD~1

# --- deleted and created again, then uncommitted changes ----------------------
git -C "$repo" rm --quiet "docs/guía de uso.txt"
commit_all "Borra la guía"
mkdir -p "$repo/docs"
printf 'nueva guía\n' >"$repo/docs/guía de uso.txt"
commit_all "La vuelve a crear"
printf 'nueva guía\npreparado\n' >"$repo/docs/guía de uso.txt"
git -C "$repo" add "docs/guía de uso.txt"
printf 'nueva guía\npreparado\nsin preparar\n' >"$repo/docs/guía de uso.txt"

cat <<EOF
Ready (open it in GitCRAFT):
  $repo
The commit only in the branch history ("Commit perdido"): $lost

Start again at any time with: $0 --replace
EOF
