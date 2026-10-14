#!/usr/bin/env bash
# Put this checkout's build on the installed daemon.
#
# The install commands and `argus server restart` are in the README; what is
# not written anywhere else is the two things that go wrong around them:
#
#   - A build that bumps the store's schema migrates runtime.db on first start
#     and there is no way back, so the store and the binaries being replaced
#     are copied aside first. Old and new builds can report the same version,
#     so the copy is taken every time rather than when it looks needed.
#
#   - The restart relaunches every pane, including the one that asked for it.
#     An agent running this from inside Argus is killed before it can report,
#     so under ARGUS_PANE the restart is scheduled detached, after a delay
#     long enough to say where its result will be. Outside a pane it runs
#     directly and is confirmed here.
#
# Nothing is installed but Argus itself: a missing prerequisite is reported
# and the script stops.
#
# Environment:
#   ARGUS_CONFIG_DIR, ARGUS_INSTANCE   as the product reads them (paths.rs)
#   CARGO_INSTALL_ROOT, CARGO_HOME     as `cargo install` reads them
#   ARGUS_RELOAD_DELAY                 seconds before a detached restart (20)
set -euo pipefail

die() {
    echo "reload: $*" >&2
    exit 1
}

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
delay=${ARGUS_RELOAD_DELAY:-20}
exe=""

# Mirrors argus_protocol::config_dir: ARGUS_CONFIG_DIR wins outright, and only
# the platform default gains an `instances/<name>` slice. A second copy of a
# rule is a liability, so when paths.rs changes this must change with it.
platform_config_dir() {
    case "$(uname -s)" in
        Linux)
            # directories ignores a relative XDG_CONFIG_HOME, as the spec says to.
            case "${XDG_CONFIG_HOME:-}" in
                /*) echo "$XDG_CONFIG_HOME/argus" ;;
                *) echo "$HOME/.config/argus" ;;
            esac
            ;;
        Darwin)
            echo "$HOME/Library/Application Support/argus"
            ;;
        MINGW* | MSYS* | CYGWIN*)
            [ -n "${APPDATA:-}" ] || die "APPDATA is not set; set ARGUS_CONFIG_DIR"
            echo "$APPDATA/argus/config"
            ;;
        *)
            die "unknown platform $(uname -s); set ARGUS_CONFIG_DIR"
            ;;
    esac
}

if [ "${ARGUS_CONFIG_DIR+set}" = set ]; then
    config_dir=$ARGUS_CONFIG_DIR
else
    config_dir=$(platform_config_dir)
    instance=$(printf '%s' "${ARGUS_INSTANCE:-}" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
    if [ -n "$instance" ]; then
        config_dir="$config_dir/instances/$instance"
    fi
fi

case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) exe=".exe" ;;
esac

# `cargo install` also honours `install.root` in a cargo config file, which
# is not read here; the newer-than-stamp check after the install catches that
# case, where older binaries may already sit in this directory.
install_root=${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}
bin_dir="$install_root/bin"
argus_bin="$bin_dir/argus$exe"

# Everything that can be refused is refused before anything is touched.
command -v cargo >/dev/null 2>&1 || die "cargo is not on PATH"
[ -d "$config_dir" ] || die "no config directory at $config_dir; has Argus run here?"
[ -f "$repo_root/crates/argusd/Cargo.toml" ] || die "$repo_root is not an Argus checkout"
case "$delay" in
    '' | *[!0-9]*) die "ARGUS_RELOAD_DELAY must be whole seconds, got '$delay'" ;;
esac

detach=""
if [ -n "${ARGUS_PANE:-}" ]; then
    command -v nohup >/dev/null 2>&1 || die "nohup is not on PATH; it is needed to restart from inside a pane"
    if command -v setsid >/dev/null 2>&1; then
        detach="setsid"
    elif command -v perl >/dev/null 2>&1; then
        # macOS ships no setsid(1), but it does ship perl.
        detach="perl"
    else
        die "neither setsid nor perl is on PATH; one is needed to restart from inside a pane"
    fi
fi

config_dir=$(cd "$config_dir" && pwd)
backup_dir="$config_dir/backup-reload-$(date +%Y%m%d-%H%M%S)"
mkdir "$backup_dir"
echo "reload: config directory $config_dir"
echo "reload: backing up to $backup_dir"

copy_store_files() {
    # Without the -wal file a copy of a live WAL database is missing whatever
    # has not been checkpointed. The -shm file is rebuilt on open, so copying
    # it only adds a way for the set to disagree.
    for suffix in "" "-wal"; do
        if [ -f "$config_dir/runtime.db$suffix" ]; then
            cp -p "$config_dir/runtime.db$suffix" "$backup_dir/"
        fi
    done
    echo "reload: store copied file by file from a running daemon: best effort," >&2
    echo "reload: a write between the two copies leaves them out of step. Install sqlite3 for a consistent backup." >&2
}

backup_store() {
    if [ ! -f "$config_dir/runtime.db" ]; then
        echo "reload: no runtime.db yet, nothing to back up"
    elif ! command -v sqlite3 >/dev/null 2>&1; then
        copy_store_files
    # Run from the backup directory so no path has to survive sqlite3's own
    # quoting ("Application Support" has a space in it).
    elif (cd "$backup_dir" && sqlite3 "$config_dir/runtime.db" ".backup runtime.db"); then
        echo "reload: store backed up with sqlite3 .backup"
    else
        echo "reload: sqlite3 .backup failed, falling back to a copy" >&2
        rm -f "$backup_dir/runtime.db"
        copy_store_files
    fi
}

for name in argus argusd argus-hook; do
    if [ -f "$bin_dir/$name$exe" ]; then
        cp -p "$bin_dir/$name$exe" "$backup_dir/"
    else
        echo "reload: no installed $name in $bin_dir to back up"
    fi
done

# --force so an up-to-date install is still rewritten: the stamp check below
# reads a binary's age as proof that this run put it there.
stamp="$backup_dir/.stamp"
touch "$stamp"
for crate in argusd argus-client; do
    cargo install --locked --force --path "$repo_root/crates/$crate" ||
        die "installing $crate failed; binaries may be half replaced, the old ones are in $backup_dir"
done

for name in argus argusd argus-hook; do
    [ "$bin_dir/$name$exe" -nt "$stamp" ] ||
        die "cargo did not install $name into $bin_dir; set CARGO_INSTALL_ROOT to where it did"
done

# Taken after the build, which the daemon keeps writing through, so a restore
# loses as little as possible.
backup_store

log="$config_dir/argusd.log"

# Only for the reader: `argus server restart` waits for the old daemon to stop
# and connects to its replacement, so its exit status is the verdict. pgrep is
# absent on Git-Bash and matches other instances' daemons everywhere.
show_daemons() {
    pgrep -l argusd 2>/dev/null || echo "  (pgrep found no argusd)"
}
before=$(show_daemons)

if [ -z "$detach" ]; then
    "$argus_bin" server restart || die "argus server restart failed; see $log"
    echo "reload: restarted; argusd before:"
    echo "$before"
    echo "reload: argusd now:"
    show_daemons
    echo "reload: daemon log is $log"
    exit 0
fi

# The job writes its own verdict: an empty output file after the fact would
# say nothing about whether the daemon came back.
restart_out="$backup_dir/restart.out"
job='sleep "$1"
"$2" server restart
echo "argus server restart exited $?"
pgrep -l argusd 2>/dev/null || true'

if [ "$detach" = "setsid" ]; then
    setsid nohup sh -c "$job" reload "$delay" "$argus_bin" >"$restart_out" 2>&1 </dev/null &
else
    perl -MPOSIX -e 'POSIX::setsid() != -1 or die "setsid: $!"; exec @ARGV' \
        nohup sh -c "$job" reload "$delay" "$argus_bin" >"$restart_out" 2>&1 </dev/null &
fi

cat <<EOF
reload: running inside pane $ARGUS_PANE, so the restart is detached and fires
reload: in ${delay}s. This pane is relaunched by it; nothing after this line is
reload: proof that the daemon came back. Once resumed, confirm with:

  cat "$restart_out"    # "argus server restart exited 0" is the verdict
  tail -n 20 "$log"

reload: argusd before the restart:
$before
EOF
