#!/bin/sh
#
# primum installer.
#
#   curl -fsSL https://raw.githubusercontent.com/aegea-dev/primum/main/install.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/aegea-dev/primum/main/install.sh | sh -s -- --version 0.1.0
#   curl -fsSL https://raw.githubusercontent.com/aegea-dev/primum/main/install.sh | sh -s -- --uninstall
#
# POSIX sh on purpose, so `| sh` and `| bash` both work. Nothing here reads
# stdin, because stdin is the script itself when this is piped.
#
# The asset names below must match what .github/workflows/release.yml
# publishes: primum-<tag>-<target>.tar.gz and primum-<tag>-<target>.sha256.

set -eu

REPO="aegea-dev/primum"

# Overridable only so the test suite can serve a deliberately corrupted archive
# and prove the checksum check actually fires. A security check that has never
# been executed is not a security check. This is a test seam, not a feature:
# do not document it, and do not fold it into the flag parser.
BASE_URL="${PRIMUM_INSTALL_BASE_URL:-https://github.com/$REPO/releases/download}"

DEST="$HOME/.local/bin"
WANT_VERSION=""
DO_UNINSTALL=0

say() { printf '%s\n' "$*"; }
warn() { printf 'warning: %s\n' "$*" >&2; }
err() { printf 'error: %s\n' "$*" >&2; exit 1; }
have() { command -v "$1" >/dev/null 2>&1; }

usage() {
	cat <<'EOF'
primum installer, from
https://raw.githubusercontent.com/aegea-dev/primum/main/install.sh

USAGE
    curl -fsSL https://raw.githubusercontent.com/aegea-dev/primum/main/install.sh | sh
    curl -fsSL https://raw.githubusercontent.com/aegea-dev/primum/main/install.sh | sh -s -- [OPTIONS]

OPTIONS
    --version X.Y.Z   Install a specific release instead of the latest
    --to DIR          Install into DIR instead of ~/.local/bin
    --uninstall       Remove the binary this script installed
    --help            Show this message

The binary is verified against the .sha256 published with each release before
it is unpacked. Nothing is downloaded from anywhere but GitHub Releases, and
your shell profile is never modified.
EOF
}

# ---------------------------------------------------------------- target

# Prints a Rust target triple, or exits with a message naming the reason.
detect_target() {
	os="$(uname -s)"
	arch="$(uname -m)"

	case "$os" in
	Darwin)
		# A shell running under Rosetta 2 on Apple Silicon reports x86_64;
		# give it the native binary. The sysctl key is absent on Macs that
		# never had it, so anything other than a literal 1 counts as native.
		if [ "$arch" = "x86_64" ] &&
			[ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
			arch="arm64"
		fi

		case "$arch" in
		arm64 | aarch64) printf 'aarch64-apple-darwin\n' ;;
		x86_64) printf 'x86_64-apple-darwin\n' ;;
		*) err "unsupported architecture on macOS: $arch" ;;
		esac
		;;
	Linux)
		case "$arch" in
		x86_64 | amd64) printf 'x86_64-unknown-linux-gnu\n' ;;
		aarch64 | arm64) printf 'aarch64-unknown-linux-gnu\n' ;;
		*) err "unsupported architecture on Linux: $arch (primum publishes x86_64 and aarch64)" ;;
		esac
		;;
	*)
		err "unsupported operating system: $os (primum publishes macOS and Linux binaries only)"
		;;
	esac
}

# ---------------------------------------------------------------- download

# Sets DL to the downloader to use, or exits.
pick_downloader() {
	if have curl; then
		DL="curl"
	elif have wget; then
		DL="wget"
	else
		err "neither curl nor wget is available; install one and try again"
	fi
}

# Sets HTTP_CODE so a caller can tell "that release does not exist" from "the
# network is broken". Without it every failure surfaces as a raw curl error,
# and a mistyped --version reads like an outage.
HTTP_CODE=""
fetch() {
	# $1 url, $2 destination path
	if [ "$DL" = "curl" ]; then
		HTTP_CODE="$(curl -sL -o "$2" -w '%{http_code}' "$1" 2>/dev/null || printf '000\n')"
		case "$HTTP_CODE" in
		200) return 0 ;;
		# file:// reports 000 and is only ever used by the test seam above.
		000)
			[ -s "$2" ] && return 0
			return 1
			;;
		*) return 1 ;;
		esac
	else
		HTTP_CODE=""
		wget -q -O "$2" "$1" 2>/dev/null
	fi
}

# Resolves the newest tag by following the /releases/latest redirect.
#
# Deliberately not api.github.com: that is rate limited to 60 requests an hour
# per IP unauthenticated, which one office behind a NAT can exhaust, and
# parsing its JSON in POSIX sh means either a jq dependency or a fragile sed.
latest_tag() {
	url="https://github.com/$REPO/releases/latest"
	if [ "$DL" = "curl" ]; then
		final="$(curl -sSfL -o /dev/null -w '%{url_effective}' "$url")"
	else
		final="$(wget -q -S --spider "$url" 2>&1 |
			awk '/[Ll]ocation:/ { print $2 }' | tail -n 1)"
	fi
	tag="${final##*/}"
	case "$tag" in
	v[0-9]*) printf '%s\n' "$tag" ;;
	*) err "could not work out the latest version from $url" ;;
	esac
}

# ---------------------------------------------------------------- verify

# Checked before anything is downloaded, so a machine that cannot verify is
# told immediately rather than after fetching the archive. Missing tools are a reason
# to stop, never a reason to install something unverified.
require_checksum_tool() {
	if have sha256sum; then
		SUM="sha256sum"
	elif have shasum; then
		SUM="shasum"
	else
		err "no sha256sum or shasum found, so the download cannot be verified.

Install one (coreutils on Linux, or use the system shasum on macOS) and run
this again. primum will not install an unverified binary."
	fi
}

sha256_of() {
	if [ "$SUM" = "sha256sum" ]; then
		sha256sum "$1" | awk '{print $1}'
	else
		shasum -a 256 "$1" | awk '{print $1}'
	fi
}

# Compares hex fields rather than running `sha256sum -c`, which insists the
# filename recorded in the file matches a file in the current directory and so
# couples verification to the working directory for no benefit.
verify() {
	# $1 archive, $2 .sha256 file
	expected="$(awk '{print $1}' "$2" | head -n 1)"
	actual="$(sha256_of "$1")"
	if [ -z "$expected" ]; then
		err "the published checksum file was empty or malformed"
	fi
	if [ "$expected" != "$actual" ]; then
		err "checksum mismatch, refusing to install.

  expected  $expected
  actual    $actual

The download was corrupted in transit, or it is not the file the release
published. Nothing has been installed."
	fi
}

# ---------------------------------------------------------------- provenance

# Answers "who owns this copy of primum": cargo, brew, or none.
#
# Shared by the shadowing warning and by --uninstall. One needs it to warn and
# the other to refuse, and two implementations of the same question drift.
owner_of() {
	p="$1"

	# Resolve one level of symlink, because Homebrew puts a link in its bin
	# directory pointing into the Cellar, and the link is what is on PATH.
	if [ -L "$p" ] && have readlink; then
		t="$(readlink "$p" 2>/dev/null || true)"
		case "$t" in
		/*) p="$t" ;;
		?*) p="$(dirname "$p")/$t" ;;
		esac
	fi

	case "$p" in
	"$HOME"/.cargo/bin/*)
		printf 'cargo\n'
		return
		;;
	/opt/homebrew/* | /usr/local/Cellar/* | /home/linuxbrew/.linuxbrew/*)
		printf 'brew\n'
		return
		;;
	esac

	if have brew; then
		prefix="$(brew --prefix 2>/dev/null || true)"
		if [ -n "$prefix" ] && [ "${p#"$prefix"/}" != "$p" ]; then
			printf 'brew\n'
			return
		fi
	fi

	printf 'none\n'
}

remove_command_for() {
	case "$1" in
	cargo) printf 'cargo uninstall primum\n' ;;
	brew) printf 'brew uninstall primum\n' ;;
	*) printf '\n' ;;
	esac
}

# Every executable primum on PATH, in PATH order.
copies_on_path() (
	IFS=:
	for d in $PATH; do
		[ -n "$d" ] || d="."
		[ -x "$d/primum" ] && printf '%s\n' "$d/primum"
	done
	true
)

# The shadowing problem: an older copy earlier on PATH keeps winning, and
# `primum --version` quietly reports it, so nothing looks wrong.
check_shadowing() {
	installed="$1"
	first="$(copies_on_path | head -n 1)"
	[ -n "$first" ] || return 0
	[ "$first" != "$installed" ] || return 0

	other_owner="$(owner_of "$first")"
	other_version="$("$first" --version 2>/dev/null || printf 'unknown\n')"
	warn "another primum comes first on your PATH and will keep winning:

  $first  ($other_version)
  $installed  (just installed)

Running 'primum' will use the first one."
	cmd="$(remove_command_for "$other_owner")"
	if [ -n "$cmd" ]; then
		say "  To remove it:  $cmd"
	else
		say "  Remove it, or put $DEST earlier on your PATH."
	fi
}

# ---------------------------------------------------------------- PATH advice

profile_for_shell() {
	case "${SHELL:-}" in
	*/fish) printf '%s\n' "~/.config/fish/config.fish" ;;
	*/zsh) printf '%s\n' "~/.zshrc" ;;
	*/bash) printf '%s\n' "~/.bashrc" ;;
	*) printf '%s\n' "your shell profile" ;;
	esac
}

path_line_for_shell() {
	case "${SHELL:-}" in
	*/fish) printf '%s\n' "fish_add_path $DEST" ;;
	*) printf '%s\n' "export PATH=\"$DEST:\$PATH\"" ;;
	esac
}

# Prints advice, never edits a dotfile. An installer that silently rewrites
# shell profiles is the behaviour people cite when they say they do not trust
# piping a script into a shell.
check_on_path() {
	case ":$PATH:" in
	*":$DEST:"*) return 0 ;;
	esac
	say ""
	say "$DEST is not on your PATH. Add it to $(profile_for_shell):"
	say ""
	say "    $(path_line_for_shell)"
}

# ---------------------------------------------------------------- uninstall

human_size() {
	du -sh "$1" 2>/dev/null | awk '{print $1}'
}

# Reports what is left behind. Never deletes any of it.
report_leftovers() {
	conf="${XDG_CONFIG_HOME:-$HOME/.config}/primum"
	[ -d "$conf" ] || return 0
	say ""
	say "Left in place: $conf, this machine's list of projects."
	say "Your projects themselves (primum.toml and items/) are never touched."
	say "    To delete the list:  rm -rf $conf"
}

uninstall() {
	target="$DEST/primum"

	if [ ! -e "$target" ]; then
		say "No primum installed at $target."
		other="$(copies_on_path | head -n 1)"
		if [ -n "$other" ]; then
			owner="$(owner_of "$other")"
			cmd="$(remove_command_for "$owner")"
			say ""
			say "There is one elsewhere on your PATH:"
			say ""
			say "  $other"
			[ -n "$cmd" ] && say "" && say "  To remove it:  $cmd"
		fi
		exit 1
	fi

	owner="$(owner_of "$target")"
	if [ "$owner" != "none" ]; then
		# Deleting it directly leaves the package manager believing primum is
		# still installed, which is a worse state than before the attempt.
		err "$target belongs to $owner, not to this installer.

Removing it directly would leave $owner thinking primum is still installed.
Use its own command instead:

    $(remove_command_for "$owner")"
	fi

	rm -f "$target"
	say "Removed $target"
	report_leftovers
	exit 0
}

# ---------------------------------------------------------------- main

main() {
	while [ $# -gt 0 ]; do
		case "$1" in
		--help | -h)
			usage
			exit 0
			;;
		--uninstall)
			DO_UNINSTALL=1
			shift
			;;
		--version)
			[ $# -ge 2 ] || err "--version needs a value, for example --version 0.1.0"
			WANT_VERSION="$2"
			shift 2
			;;
		--to)
			[ $# -ge 2 ] || err "--to needs a directory"
			DEST="$2"
			shift 2
			;;
		*) err "unknown option: $1 (try --help)" ;;
		esac
	done

	if [ "$DO_UNINSTALL" = "1" ]; then
		uninstall
	fi

	pick_downloader
	require_checksum_tool

	target="$(detect_target)"

	if [ -n "$WANT_VERSION" ]; then
		# Users type both forms and the tag carries the v, so normalise once
		# here rather than in each URL below.
		case "$WANT_VERSION" in
		v*) tag="$WANT_VERSION" ;;
		*) tag="v$WANT_VERSION" ;;
		esac
	else
		tag="$(latest_tag)"
	fi

	archive="primum-$tag-$target.tar.gz"
	# The checksum asset is NOT "<archive>.sha256". The release workflow names
	# it after the archive stem, so the .tar.gz is replaced rather than
	# extended. Getting this wrong 404s only on the checksum, which reads like
	# a missing release rather than a naming mistake.
	sums="primum-$tag-$target.sha256"
	url="$BASE_URL/$tag/$archive"
	sums_url="$BASE_URL/$tag/$sums"

	tmp="$(mktemp -d)"
	# shellcheck disable=SC2064
	trap "rm -rf '$tmp'" EXIT INT TERM

	say "Installing primum $tag ($target)"

	if ! fetch "$url" "$tmp/$archive"; then
		if [ "$HTTP_CODE" = "404" ]; then
			err "no such release: $tag

Check the version you asked for against the published releases:
  https://github.com/$REPO/releases"
		fi
		err "could not download $url ${HTTP_CODE:+(HTTP $HTTP_CODE)}

Check your network connection and try again."
	fi
	if ! fetch "$sums_url" "$tmp/$sums"; then
		err "downloaded the archive but not its checksum, so it cannot be verified"
	fi

	# Verify before unpacking. Extracting first and checking afterwards means
	# an attacker-controlled archive has already been written somewhere the
	# user is about to run it. The order is the security property.
	verify "$tmp/$archive" "$tmp/$sums"

	tar xzf "$tmp/$archive" -C "$tmp"
	[ -f "$tmp/primum" ] || err "the archive did not contain a primum binary"

	mkdir -p "$DEST" || err "could not create $DEST"
	if ! mv "$tmp/primum" "$DEST/primum" 2>/dev/null; then
		err "could not write to $DEST. Choose somewhere else with --to DIR"
	fi
	chmod 755 "$DEST/primum"

	say "Installed $("$DEST/primum" --version 2>/dev/null || echo primum) to $DEST/primum"

	check_on_path
	check_shadowing "$DEST/primum"

	say ""
	say "Next:  cd into a project folder, then  primum init"
	say "Docs:  https://github.com/aegea-dev/primum"
}

main "$@"
