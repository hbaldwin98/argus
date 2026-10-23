# Adopting reliab

argus checks its changes with [reliab](https://github.com/hbaldwin98/reliab):
`reliability-profile.yaml` says which tools fill each testing role and what
each tier runs, and `AGENTS.md` §"Testing rules (reliab)" holds the rules an
agent follows. The profile is the source of truth for what runs; this file
records why it is shaped the way it is, what each adoption stage found, and
what is still open. It began as argus's records in reliab's own knowledge
base, moved here on 2026-10-18 when reliab stopped naming the codebases it
runs on (reliab commit 135c2de has the originals).

## The profile

argus was surveyed on 2026-10-17: a Rust workspace of about 146k lines in
four crates, about eight weeks old. Its API is a local socket (a Unix socket
or a Windows named pipe) carrying length-prefixed msgpack, with about 87
`ClientMsg` dispatch arms, and its state is SQLite. It had about 3.2k tests,
mostly inline unit tests, proptest in four files, nightly cargo-fuzz,
store-upgrade fixtures and 3-OS CI with clippy `-D warnings`; no snapshots and
no mutation tooling. Its starting condition is **maintained**, so adoption ran
S0, S1, a mutation baseline over the existing suite, then S2 to S4.

| Role | Filling | State |
|---|---|---|
| T1 unit | cargo-nextest, JUnit through `.config/nextest.toml` profiles | verified 2026-10-17 |
| T2 properties | proptest, in a `mod properties` beside the code | verified 2026-10-17 |
| T3 snapshots | insta, over request/response transcripts on the socket (`crates/argusd/src/test_host/tests`) | verified 2026-10-18 |
| T4 mutation | cargo-mutants, changed lines; `reliab-adapter-cargo-mutants` turns survivors into findings | verified 2026-10-17 |
| T5 in-process host | `test_host::Host`: the real accept loop and dispatch on a private endpoint in a temp dir | verified 2026-10-18 |
| T6 typed client | the argus-protocol client | |
| T7 API fuzzing | proptest over arbitrary `ClientMsg` sequences (no OpenAPI spec exists) | verified 2026-10-18 |
| T10 store faults | SQLite hooks on the store's connection (`store::faults`) | verified 2026-10-18 |
| T12 coverage | cargo-llvm-cov | named only: no `coverage` command is wired, so every tier skips it |
| T14 static analysis | clippy | verified 2026-10-17 |
| T8, T9, T11, T13 | not applicable, each with its reason in the profile | |

**Protected paths** (R2: they change only with a person's approval): the
saved failing cases under `**/proptest-regressions/**`, the store-upgrade
fixtures `crates/argusd/tests/fixtures/stores/**`, the transcript snapshots
`**/snapshots/**`, and `mutants.toml`. insta's `cargo insta accept` and
`review`, and an `INSTA_UPDATE` that writes snapshots, are protected commands,
because they approve a snapshot without naming its file. The ts-rs bindings
are generated code CI checks, not oracles, and are not protected.

**Impacted tests:** one map entry per crate directory, so a change runs the
crates it touched.

**Budgets:** the inner tier is held to 300 seconds and runs everything rather
than only impacted crates, because S0 measured the full inner tier at 86
seconds. Each layer added since names its cost in the profile; inner now takes
about 165 seconds warm. The budget is not raised to fit a layer.

## Stages

| Stage | What argus gained | Commits |
|---|---|---|
| S0, S1 | profile, nextest JUnit profiles, agent settings and testing rules | e20b03c |
| Baseline | mutation baseline for argusd's store, 91.5% | b4b0b8e |
| S2 | the in-process host and five pinned conversations as snapshots, labelled additive-only or values-changed | c07ef4d, 0625b75, c8dacef |
| S3 | a generator for every `ClientMsg`, held by exhaustive matches, and a request-sequence fuzz | 0d006e3, e4b0903, 4d18dac, 80ce3c7 |
| S4 | every store write faulted at each step and its commit, held to all-or-nothing | 0d42c2a, 95f08a5, 86d593c |

### S0 (2026-10-17)

- The client package is `argus`, not `argus-client`, and argus already kept a
  regression corpus, proptest's saved failing cases, which AGENTS.md says never
  to delete; so `**/proptest-regressions/**` is protected.
- nextest needs profiles in `.config/nextest.toml` to write JUnit. Properties
  run as their own command at the tier's case count.
- Warm, the inner tier took 86s (3,306 tests, 10 properties at 100 cases)
  and prepr 45s; a cold build of a worktree took 138s.
- Tests regenerated the ts-rs bindings with LF on a checkout with CRLF, so
  `git status` listed them after every run. d4da249 checks every file out
  with LF, which ended it.

### The mutation baseline (2026-10-17)

`cargo mutants -p argusd -f 'crates/argusd/src/store/**' --copy-target=false
-j 4 -- --bin argusd`: 389 mutants in 44 minutes; 324 caught, 9 timeouts, 31
missed, 25 unviable, a score of **91.5%** (333 of 364 viable). The misses
cluster in `Store::migrate` (8, schema-version comparisons at off-by-one
boundaries), `insert_task` (7, Task fields dropped without any test noticing),
`bounded`, and pairs in the feature and task rename and append paths.

On Windows the mutation commands skip functions compiled only under
`cfg(unix)`, which a Windows build never compiles; that code needs a Linux
mutation run. The profile names what is skipped.

### S2: characterization

argus is maintained, not legacy, so it pins a lean set of its most-used
conversations rather than every response: each response type appears in a few
snapshots, so a deliberate new field touches a handful of files. reliab labels
a snapshot diff additive-only or values-changed from insta's pending `.snap.new`
files, so an addition is a quick approval and anything else gets real review.
Characterization's main use is on demand around refactors: `reliab
characterize capture`, the change, `compare`, then `discard`, with nothing
committed. Ordinary feature work is carried by properties and the fuzz, which a
deliberate change does not churn.

- `INSTA_UPDATE=new` writes each mismatch as a pending file and never touches
  an accepted snapshot; `INSTA_PENDING_DIR` keeps those files under
  `.reliab/out`, out of the protected directories. It must be absolute and
  native to the test process: `pwd -W` gives `C:/...` under Git Bash.
- The protected commands deny an agent typing `INSTA_UPDATE=new`; reliab runs
  the profile's command itself, which no hook sees.
- With `RELIAB_CAPTURE_DIR` set, each transcript test writes its text there
  instead of asserting it (`test_host::transcript`), which is what `reliab
  characterize` runs.

### S3: the request fuzz

The acceptance plant, a crash on a zero-row resize, was caught in 10 of 14
inner runs at 100 cases and 9 of 10 at 300, so inner runs the fuzz at 300
cases (about 20s more) while its other properties keep 100. In acceptance, a
fresh agent session whose change carried the planted crash reproduced it from
its seed and fixed it in product code.

### S4: store faults

`store::faults` stops a write at any step (a writing statement prepared or a
row written) or at its commit, fills the disk with `max_page_count`, and holds
another writer's lock; `store::tests::faults` holds every store write to
leaving all of itself or none, and the daemon to refusing a stopped write
without hanging up. It found two writes made in more than one transaction:
forgetting a pane (95f08a5) and an agent opening a feature and pointing its
checkout at it (86d593c). A write that fails after its commit is a known gap,
recorded in `store::tests::faults`.

## The agentic change flow

The aim is an agent taking a change from edit to PR with evidence a person can
check: impacted tests, properties from intent, failures reproduced and fixed in
product code, characterization, mutation on changed code, the fuzz, reported
skips, a reviewer's verdict and a PR summary. CI and nightly work come last
(the owner's call, 2026-10-18), and every step ends with real use, a fresh
agent session doing the work, not only green tests.

1. **On-demand testing**, `reliab verify --only <command> --scope <paths>`:
   done; `mutation_full` takes a scope (cec14aa).
2. **argus's evidence layers**, S2 to S4: done, each accepted with a session
   whose change tripped the new layer.
3. **A legible change**, the reviewer agent and `reliab report summary`:
   accepted on 2026-10-18 in a scratch clone with `require_reviewer_verdict`.
   The author never recorded a verdict, the reviewer re-ran prepr in a clean
   worktree every time, the summary matched the report, and the push gate
   allowed, went stale and allowed again as it should. reliab v0.1.1 since
   added person-only `reliab approve` for a protected change.
4. **Proof the rules hold**, reliab's eval suite: reliab's own work.
5. **Last:** CI gates (`reliab verify --tier prepr`, `reliab gate protected`)
   and the nightly tier, which argus does not run yet.

argus ran the plugin ahead of S2 to S4, earlier than reliab's stage order;
until those layers existed an agent's change was checked by unit tests,
impacted properties and mutation on changed lines only. The paired replay
pilot of 2026-10-18 loaded the operator's user-level plugins in both arms,
so it measured less cleanly than an isolated run would.

## Where reliab used argus as an example

Before 135c2de, reliab's decisions cited argus for: the maintained starting
condition, the one-entry-per-crate impacted map, insta's approval commands as
the first protected commands, the comma that moves when a field is appended to
a transcript's JSON (counted as additive), the draft profile in its schema
tests, and v0's definition of done, a real change taken edit to PR on argus.
They now use neutral examples. The draft argus profile there
(`schemas/testdata/argus.profile.yaml`) is superseded by
`reliability-profile.yaml`.
