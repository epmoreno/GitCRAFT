#!/usr/bin/env bash
# Two throwaway repositories stopped in the middle of a conflict, to try
# GitCRAFT's conflict view (task 9). Nothing else is touched.
#
#   ~/gitcraft-conflictos-merge    a merge of "feature" into "main", stopped:
#                                  text with several blocks, CRLF, a binary,
#                                  a path with spaces, a file deleted on each
#                                  side, and one change merged cleanly
#   ~/gitcraft-conflictos-rebase   a rebase of "topic" (3 commits) onto
#                                  "main", stopped at the first one; the
#                                  third conflicts again
#
# Usage:
#   scripts/make-conflict-examples.sh             create both repositories
#   scripts/make-conflict-examples.sh --replace   same, moving existing ones to the Trash first
#
# Base directory: $GITCRAFT_SANDBOX_DIR (default: $HOME). Nothing is ever
# deleted: with --replace, existing directories go to the macOS Trash, after
# asking.

set -euo pipefail

base="${GITCRAFT_SANDBOX_DIR:-$HOME}"
merge_repo="$base/gitcraft-conflictos-merge"
rebase_repo="$base/gitcraft-conflictos-rebase"

# Commits made by this script use a fixed identity and never run your hooks
# or sign; what you do in the app afterwards uses your own git config.
script_git() {
	GIT_AUTHOR_NAME="Script de prueba" GIT_AUTHOR_EMAIL="script@gitcraft.invalid" \
		GIT_COMMITTER_NAME="Script de prueba" GIT_COMMITTER_EMAIL="script@gitcraft.invalid" \
		git -c core.hooksPath=/dev/null -c commit.gpgSign=false "$@"
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

# Runs a command that must stop on a conflict (git exits with 1).
expect_conflict() {
	if script_git "$@" >/dev/null 2>&1; then
		echo "Expected a conflict from: git $*" >&2
		exit 1
	fi
}

commit_all() {
	local repo="$1" message="$2"
	git -C "$repo" add --all
	script_git -C "$repo" commit --quiet -m "$message"
}

make_merge() {
	local r="$merge_repo"
	git init --quiet --initial-branch=main "$r"
	printf 'windows.txt -text\n' >"$r/.gitattributes"
	cat >"$r/conflicto.txt" <<'EOF'
fn main() {
    let saludo = "hola";
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;
    let f = 6;
    let veces = 1;
    let color = "azul";
    println!("{saludo} x{veces} en {color} ({a} {b} {c} {d} {e} {f})");
}

// Notas
// - primera nota
// - última nota
EOF
	printf 'linea uno\r\nlinea dos\r\nlinea tres\r\n' >"$r/windows.txt"
	printf 'PNG\0base\0' >"$r/logo.bin"
	printf 'un archivo\n' >"$r/con espacio.txt"
	printf 'se borra en main\n' >"$r/borrado-en-main.txt"
	printf 'se borra en feature\n' >"$r/borrado-en-feature.txt"
	printf 'solo cambia en feature\n' >"$r/sin-conflicto.txt"
	commit_all "$r" "Base"

	git -C "$r" switch --quiet -c feature
	sed -i.bak -e 's/"hola"/"buenas"/' -e 's/veces = 1/veces = 2/' \
		-e 's/última nota/última nota (feature)/' "$r/conflicto.txt"
	rm "$r/conflicto.txt.bak"
	printf 'linea uno\r\nlinea dos (feature)\r\nlinea tres\r\n' >"$r/windows.txt"
	printf 'PNG\0feature\0' >"$r/logo.bin"
	printf 'un archivo (feature)\n' >"$r/con espacio.txt"
	printf 'se borra en main, pero feature lo cambia\n' >"$r/borrado-en-main.txt"
	rm "$r/borrado-en-feature.txt"
	printf 'solo cambia en feature: hecho\n' >"$r/sin-conflicto.txt"
	commit_all "$r" "Cambios de feature"

	git -C "$r" switch --quiet main
	sed -i.bak -e 's/"hola"/"hello"/' -e 's/"azul"/"verde"/' \
		-e 's/veces = 1/veces = 3/' -e 's/última nota/última nota (main)/' \
		"$r/conflicto.txt"
	rm "$r/conflicto.txt.bak"
	printf 'linea uno\r\nlinea dos (main)\r\nlinea tres\r\n' >"$r/windows.txt"
	printf 'PNG\0main\0' >"$r/logo.bin"
	printf 'un archivo (main)\n' >"$r/con espacio.txt"
	rm "$r/borrado-en-main.txt"
	printf 'se borra en feature, pero main lo cambia\n' >"$r/borrado-en-feature.txt"
	commit_all "$r" "Cambios de main"

	expect_conflict -C "$r" merge feature
}

make_rebase() {
	local r="$rebase_repo"
	git init --quiet --initial-branch=main "$r"
	printf 'uno\ndos\ntres\n' >"$r/lista.txt"
	commit_all "$r" "Base"
	git -C "$r" branch topic

	printf 'uno (main)\ndos\ntres\n' >"$r/lista.txt"
	commit_all "$r" "Main cambia la primera línea"

	git -C "$r" switch --quiet topic
	printf 'uno (topic)\ndos\ntres\n' >"$r/lista.txt"
	commit_all "$r" "Topic 1: cambia la primera línea"
	printf 'algo nuevo\n' >"$r/nuevo.txt"
	commit_all "$r" "Topic 2: añade nuevo.txt"
	printf 'uno (topic, otra vez)\ndos\ntres\n' >"$r/lista.txt"
	commit_all "$r" "Topic 3: vuelve a cambiar la primera línea"

	expect_conflict -C "$r" rebase main
}

existing=()
for path in "$merge_repo" "$rebase_repo"; do
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
elif [[ -n "${1:-}" && "${1:-}" != "--replace" ]]; then
	echo "Unknown option: $1" >&2
	sed -n '2,19p' "$0" >&2
	exit 1
fi

make_merge
make_rebase

cat <<EOF
Ready (open them in GitCRAFT):
  merge stopped on conflicts:  $merge_repo
  rebase stopped on conflicts: $rebase_repo

Start again at any time with: $0 --replace
EOF
