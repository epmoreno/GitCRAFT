#!/usr/bin/env bash
# Sandbox to try GitCRAFT's remote features (fetch, pull, push) without
# any real server: everything is local.
#
#   ~/gitcraft-remoto.git        bare repository acting as the remote ("origin")
#   ~/gitcraft-remoto-prueba     your working repository (open it in GitCRAFT)
#   ~/gitcraft-remoto-otra       a second clone: "someone else" who pushes
#
# Usage:
#   scripts/make-remote-sandbox.sh                create the three repositories
#   scripts/make-remote-sandbox.sh --replace      same, moving existing ones to the Trash first
#   scripts/make-remote-sandbox.sh otra-push [file] [text]
#                                                 "someone else" commits and pushes to main
#
# Base directory: $GITCRAFT_SANDBOX_DIR (default: $HOME). Nothing is ever
# deleted: with --replace, existing directories go to the macOS Trash, after
# asking.

set -euo pipefail

base="${GITCRAFT_SANDBOX_DIR:-$HOME}"
remote="$base/gitcraft-remoto.git"
work="$base/gitcraft-remoto-prueba"
other="$base/gitcraft-remoto-otra"

# Commits made by this script use fixed identities; your own commits in the
# working repository keep using your git config.
script_git() {
	GIT_AUTHOR_NAME="Script de prueba" GIT_AUTHOR_EMAIL="script@gitcraft.invalid" \
		GIT_COMMITTER_NAME="Script de prueba" GIT_COMMITTER_EMAIL="script@gitcraft.invalid" \
		git "$@"
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

other_push() {
	local file="${1:-de-otra-persona.txt}"
	local text="${2:-Cambio de otra persona ($(date '+%H:%M:%S'))}"
	if [[ ! -d "$other/.git" ]]; then
		echo "Missing $other: run this script without arguments first." >&2
		exit 1
	fi
	git -C "$other" pull --quiet --ff-only origin main
	printf '%s\n' "$text" >>"$other/$file"
	git -C "$other" add -- "$file"
	git -C "$other" commit --quiet -m "Otra persona: cambia $file"
	git -C "$other" push --quiet origin main
	echo "\"Someone else\" pushed to origin/main:"
	git -C "$other" log --oneline -1
}

create() {
	local replace="${1:-}"
	local existing=()
	for path in "$remote" "$work" "$other"; do
		[[ -e "$path" ]] && existing+=("$path")
	done
	if ((${#existing[@]} > 0)); then
		if [[ "$replace" != "--replace" ]]; then
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
	fi

	git init --quiet --bare --initial-branch=main "$remote"

	git init --quiet --initial-branch=main "$work"
	printf '# Repo de prueba de remotos\n' >"$work/README.md"
	printf 'uno\ndos\ntres\ncuatro\ncinco\n' >"$work/lista.txt"
	git -C "$work" add -- README.md lista.txt
	script_git -C "$work" commit --quiet -m "Primer commit"
	printf 'seis\n' >>"$work/lista.txt"
	git -C "$work" add -- lista.txt
	script_git -C "$work" commit --quiet -m "Añade seis"
	git -C "$work" remote add origin "$remote"
	git -C "$work" push --quiet --set-upstream origin main

	# A published branch, and a local one without upstream.
	git -C "$work" switch --quiet -c feature
	printf 'feature\n' >"$work/feature.txt"
	git -C "$work" add -- feature.txt
	script_git -C "$work" commit --quiet -m "Empieza feature"
	git -C "$work" push --quiet --set-upstream origin feature
	git -C "$work" switch --quiet -c sin-publicar
	printf 'local\n' >"$work/local.txt"
	git -C "$work" add -- local.txt
	script_git -C "$work" commit --quiet -m "Commit sin publicar"
	git -C "$work" switch --quiet main

	git clone --quiet "$remote" "$other"
	git -C "$other" config user.name "Otra persona"
	git -C "$other" config user.email "otra@gitcraft.invalid"

	cat <<EOF
Sandbox ready:
  working repo (open it in GitCRAFT): $work
  remote (bare):                     $remote
  "someone else" (second clone):     $other

Make "someone else" push a commit to main:
  $0 otra-push [file] [text]
EOF
}

case "${1:-}" in
otra-push)
	shift
	other_push "$@"
	;;
"" | --replace)
	create "${1:-}"
	;;
*)
	echo "Unknown option: $1" >&2
	sed -n '2,17p' "$0" >&2
	exit 1
	;;
esac
