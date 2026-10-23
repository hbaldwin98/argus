# Contributing

[`AGENTS.md`](AGENTS.md) holds the conventions code is held to (where modules go, style, testing
rules) and [`ARCHITECTURE.md`](ARCHITECTURE.md) the rules the code is built to.

## Prerequisites

- A current stable Rust toolchain through [rustup](https://rustup.rs/). No minimum version is
  pinned; CI lints with the newest stable, so `rustup update stable` first.
- Git on `PATH`, and a native C/C++ toolchain.

**Ubuntu or Debian**

```sh
sudo apt install build-essential pkg-config git curl
# the Tauri window (argus-desktop):
sudo apt install libwebkit2gtk-4.1-dev libdbus-1-dev
# the GPUI window (argus-gpui):
sudo apt install libfontconfig-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libx11-xcb-dev libxcb1-dev
```

**macOS**: `xcode-select --install`, then `brew install git pkg-config`.

**Windows**: Git for Windows, and Visual Studio 2022 Build Tools with **Desktop development with
C++**, with Rust's default MSVC toolchain.

libgit2 is built from vendored source, and fetching and pulling go through the `git` on `PATH`, so
neither OpenSSL nor a system libgit2 is needed.

## Building

```sh
cargo build --workspace --release --locked
```

Build the whole workspace: `argus` looks for `argusd` beside itself, and `argusd` for `argus-hook`.
A plain `cargo build` or `cargo test` leaves out `argus-desktop` and `argus-gpui`, so the terminal
client and daemon build without the window's system libraries.

To install from source, give every package the same root, so the executables sit together:

```sh
cargo install --locked --path crates/argusd
cargo install --locked --path crates/argus-client
cargo install --locked --path crates/argus-gpui      # optional: the desktop window
cargo install --locked --path crates/argus-desktop   # optional: the Tauri window it replaced
```

## Running a development build beside an installed one

```sh
cargo build -q -p argus -p argusd                    # add -p argus-gpui for the window
ARGUS_INSTANCE=dev target/debug/argus                # this checkout's Argus
ARGUS_INSTANCE=dev target/debug/argus desktop        # its desktop window
ARGUS_INSTANCE=dev target/debug/argus server stop    # end its daemon
```

On Windows, `.\scripts\dev.ps1` builds and opens it, and `.\scripts\dev.ps1 -Stop` ends it.

`ARGUS_INSTANCE=dev` gives the daemon its own endpoint and its own slice of the config directory, so
the installed Argus keeps running untouched. The daemon outlives the client, so stop it when done.

`scripts/reload.sh` puts this checkout on the installed daemon instead: it backs up the store and
binaries first, and detaches the restart when run inside an Argus pane.

To watch the daemon start, run it in the foreground before opening a client:

```sh
RUST_LOG=argusd=debug cargo run -p argusd --bin argusd
```

## Checks

```sh
cargo check --workspace --all-targets --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

GitHub Actions runs tests and clippy on Linux x86_64, macOS arm64 and Windows x86_64, and checks the
desktop page. The sources are hand-formatted; rustfmt is disabled. Some tests launch real PTYs and
Git processes, so a test run can pause briefly while their processes are torn down.
`scripts/ci-failure.sh [run-id]` prints only the failure lines of a CI run.

### The desktop page

The Tauri window's page is TypeScript in `crates/argus-desktop/page`, compiled into
`crates/argus-desktop/assets/js` and committed, so building the crate needs no Node. From the page
directory:

```sh
npm ci            # once
npm run build     # compile src/ into ../assets/js; commit what it writes
npm run fmt       # format with oxfmt
npm run check     # compile, oxlint and oxfmt --check, as CI does
```

`cargo test -p argus-desktop-core` writes the types Rust sends the page into `page/src/bindings`;
commit them with the change to `connection/page.rs`.

### Reliability evidence (reliab)

[reliab](https://github.com/hbaldwin98/reliab) runs the test layers in `reliability-profile.yaml`
(unit tests, properties, the daemon's transcripts, request fuzz, store faults, mutation of changed
code) in tiers, and holds agents to the testing rules in `AGENTS.md`. The version is pinned in
`.reliab-version`. Claude Code sessions get it from the plugin in `.claude/settings.json`; for your
own shell:

```sh
curl -fsSL https://raw.githubusercontent.com/hbaldwin98/reliab/main/scripts/install.sh | sh
```

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/hbaldwin98/reliab/main/scripts/install.ps1)))
```

The tiers also need `cargo-nextest` and `cargo-mutants` at the versions the profile lists.

```sh
reliab verify --tier inner     # a change is done (about 5 minutes)
reliab verify --tier prepr     # before a push (up to 30 minutes)
reliab report route            # what to do about each finding
reliab doctor                  # check the install against the profile
```

The `reliab` workflow runs the pre-PR tier on every push to `main` and every pull request, and lists
any protected path a pull request changes; those need a person's approval (`reliab approve <path>`),
never an agent's. To bump reliab, edit `.reliab-version`, re-run
`reliab profile init --starting-condition maintained`, and commit both.

## Publishing a release

Releases are built from version tags matching `workspace.package.version` in `Cargo.toml` (`v0.1.0`
for `0.1.0`).

1. Update `workspace.package.version` and commit it, with `Cargo.lock`, as
   `Bump version to X for release`.
2. If the release raises `SCHEMA_VERSION` in `crates/argusd/src/store/mod.rs`, commit a store it
   wrote: populate one through `Store`'s own methods in a throwaway test, save it as
   `crates/argusd/tests/fixtures/stores/runtime-vX.Y.Z.db`, and add it to `RELEASES` in
   `crates/argusd/src/store/tests/releases.rs` with a test of what it holds.
3. Ensure CI passes on `main`.
4. Tag and push:

```sh
git tag -a v0.1.0 -m "Argus 0.1.0"
git push origin v0.1.0
```

The release workflow runs CI's full matrix against the tag, then builds the native packages,
checksums them and creates a GitHub Release. A mismatched tag or any failure stops it before
anything is published.
