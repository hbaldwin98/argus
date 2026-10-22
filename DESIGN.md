# Argus: Current Design

Argus is a terminal workspace for running shells and AI command-line agents against Git
checkouts. This document describes the code as it exists today. Desired behavior lives in
[`TARGET.md`](TARGET.md); unfinished work lives in [`ROADMAP.md`](ROADMAP.md).

## Process model

Argus has four binaries and one library:

- `argus`: the ratatui/crossterm client.
- `argusd`: the daemon that owns PTYs, terminal state, Git state, and runtime persistence.
- `argus-hook`: the helper managed hooks run, and the command an agent runs to report on itself.
- `argus-desktop`: a desktop window, a second view of the same daemon, drawn in a webview.
- `argus-desktop-core` (the library): what that window holds and says, with no window in it, for the webview window and the GPUI one to share.

The client starts the daemon lazily when it cannot connect. On Unix the daemon process calls
`setsid`; on Windows it starts in a detached process group. Closing the client does not stop the
daemon or its child processes. Before restoring panes or starting its hook server, the daemon holds
an OS-level instance lock and binds the endpoint without replacing a live socket. A second start
exits without restoring panes; a Unix socket pathname is removed only after a bind failure proves
that no daemon can connect to it.

`argus server restart` is the explicit clean replacement path. The client sends a protocol control
message, waits for the daemon to flush its acknowledgement and release the endpoint, then starts
the replacement through the same lazy-start path. The daemon removes managed hooks before exit;
the replacement restores non-exited panes from the runtime session store.

`argus server stop` uses a separate control message. The daemon acknowledges the request, tells
every connected client to exit so their reconnect loops cannot launch a replacement, removes
managed hooks, and releases the endpoint. The next ordinary `argus` launch starts the daemon and
restores non-exited panes from the runtime session store.

The desktop window (`argus-desktop`, opened by `argus desktop`) never opens the daemon's endpoint
and never starts `argusd` itself. It runs `argus bridge` beside its own executable, or from `PATH`,
and speaks the protocol over the child's stdin and stdout, as `argus --host` does over ssh, so
connect-or-start lives only in the terminal client. It greets like any client, keeps the last tree
on screen while it tries again every two seconds, and shows what the bridge printed when a try
fails. After `Stopping` it does not reconnect until the person presses Reconnect: a client never
starts again a daemon somebody stopped. The window can reach another machine's daemon instead: the
saved `host` (or `ARGUS_DESKTOP_HOST` when none is saved) makes the same child `ssh -T -o BatchMode=yes
-- <host> argus bridge`, with the arguments and the reading of ssh's failures shared with `argus --host`
through the protocol's `connection::ssh_bridge`. Switching host drops the old bridge, empties the tree and the rail
back to connecting, and names the host in the sidebar's foot and in each notification; a host that
cannot be reached leaves a Use this machine button on the connecting screen, since no rail is loaded
to open the switcher from. An editor set to open in a window of its own is not asked for over ssh,
where that window would open on the remote machine. The person's choice and the hosts used are kept in
`desktop.json`. The system's title bar is put away: a sidebar runs to the window's top edge with Search (the palette), New, and the Workspace, Features and Checkouts stages above the project's rail, the connection, the chime and the settings at its foot, and a strip across the stage names what it shows beside the window's own buttons (macOS keeps its traffic lights; on Windows 11 resting on maximize opens snap layouts). The project heading above the
rail opens the switcher: each project with how many of its agents need you or are working, then
Add a project…, Switch workspace… (the daemon's, so every client follows), Connect to a host…
(the hosts in `~/.ssh/config` and the ones used before, unfolded under it), and Remove, which asks first and leaves the folders alone.
Add a project is a dialog over a folder browsed on the daemon's machine (`ListDirectories`): a folder
of repositories, one repository, or a new one made with `git init`, with what the daemon's scan would
add (`PreviewProject`) beside the folders, then a name and folders to skip; it stays up until the
daemon has made the project and a tree holds it, and a refusal is said in it. The rail is a tree: a heading per
repository, a branch row per checkout with its + on hover, and the checkout's panes nested under it
as a status dot and a title, saying only what needs a look or has ended. Every repository is listed,
the quiet ones after the active as the daemon orders them, and the rail only ever scrolls down. Its
right edge drags it wider or narrower, between 200 and 560 pixels; the width is kept in `desktop.json`,
and a double-click on the edge gives back the usual one, which narrows in a small window. New (`Ctrl-N`, in a pane the leader and `n`) is the composer, a stage
of its own: the chosen repository, a checkout and an agent picked beside a message that becomes
the agent's first prompt, or a shell there. It stays up until the daemon makes the pane, so a
refused start keeps what was typed. It shows one pane at a time; the daemon sizes a pane to the
client the person was last at, so a window and a terminal client on one pane each fill while in use.
A drag across the pane selects and letting go copies; the text is read from the window's grid in Rust,
and the clipboard is written and read there too (Ctrl-V, Ctrl-Shift-V, Alt-V), not by the page. A
picture on it crosses as a file (`PasteFile`), and text has its line endings made newlines, as the
terminal client does (`argus_protocol::panes::paste`).
The wheel and Shift-PageUp/PageDown act as the terminal client's do: a child that asked for the
mouse gets its wheel reports (`argus_protocol::panes::mouse`), the normal screen parks the view in the
pane's history (the view held in `argus_protocol::panes::view`, asked further by line, typing back to live), and a
full-screen child that asked for nothing gets a cursor key a notch. A press, release, drag or move over a child that
asked for the mouse goes to it in the mode it asked for, and only Shift-drag selects there; the frame
says whether it asked, so the page can tell a press for the child from the start of a selection. The stage header leads its size
with how far back a parked view is. It
reports its view only while it has focus, so a window in the background never takes the focus from
a terminal client being typed in. Its keys are one table, `keys/keymap`, of bindings each with its keys,
where it works (Anywhere, In a pane, Sidebar, Feature, Board card, a dialog, a text field — the
shortcuts sheet's groups), what it does and what it is called. A key in a pane goes to Rust, which
dispatches it from the table; the table crosses to the page once it loads, and the page dispatches
every other key from it, a text field's Enter and Escape included, so a key cannot be run by one and
missing from what the sheet and the palette are drawn from; a test fails a page module that reads a
key's name itself. Who has a key outside a pane is the table's too: an ordered list of layers — a
text field, each dialog, the sidebar's pick, a menu over the stage, each stage — each with the places
it hears in order and whether it takes the keys from a pane it opens over. The first layer up has
the key, Escape puts away the first thing it finds in the same order, and the page (`keys/focus`) holds a
handler per layer but no order of its own, so a dialog is added in one place and a test checks every
layer's places for a key bound twice. A binding of the Window place, the zoom keys', is heard by
the page before anything else, a pane and a text field included, and so is bound nowhere else. The
zoom is the webview's own rather than the page's CSS, so a pane's cells grow with the rest and its
grid refits to the new size; it steps as a browser's does and is kept in `desktop.json`.
In a pane every other
key is the program's except the leader, `Ctrl-Space` as in the terminal client: the key after it is
the window's, one nothing binds is dropped, and a second leader sends one through. `Ctrl-K` (in a
pane, the leader and `k`) opens the palette, one fuzzy search over the table's commands and the
panes, checkouts and features Rust lists while it is open (`keys/palette`), `>` narrowing it to commands
and `#` to features; a command chosen is a call that `session` runs where its key would, the
checkout ones on the Checkouts stage's row or else the pane on screen's checkout. A stage hides the
pane, so from one the pane commands find nothing on screen, and a held or archived feature chosen
unfolds the Feature index to it. `?` (in a pane, the leader and `?`) opens the shortcuts sheet,
the table's groups as a list of sections beside the keys of the one chosen, generated from the table
(`keys/sections` pairs next and previous, and the four arrows, into a row each). It opens on the
section for where the person is, filters across all of them, and says the leader once in the pane's
header rather than on every row. Both are drawn over everything, a review included, and a pane's leader key reaches them as a
`Run` event, since only the page draws them. So does the Settings dialog, `Ctrl-,` (in a pane, the
leader and `,`), also in the palette and behind the gear at the sidebar's foot: the theme (Mocha,
Macchiato, Frappé, Latte or Argus Night — Argus's own, a cool navy lit by one periwinkle accent — or
one of seven named for the systems they follow: Ledger, Carbon, Flexoki, Kanagawa, Rosé Pine, Nord
and Radix, or following the system as Mocha in the dark and Latte in the light),
the background's opacity and what shows through it, the two fonts, where a file opens and the editor
command, the system notification an agent coming to need you raises, and the layout a repository's
Feature index opens as before it remembers its own. Each choice is a call that
`settings/preferences` holds and saves as it is made, and the page draws the look from what comes
back. A theme may also set the shape tokens, the radii, control heights and heading weights each
system draws with, and brings its own interface and code faces as the default, with the Settings
choice still in front of them. Each font is the family of one
installed on the machine, typed or picked from a list under the field that draws each family in its
own face: the families `settings/installed_fonts` reads at start, those the webview can find under that name,
with a note when nothing is called what the field holds. A half-typed name is not chosen until it is
picked, entered or left. The code font, offered from the monospaced families, goes in front of the
bundled code face (JetBrains Mono unless the theme bundles another) and Symbols Nerd Font Mono, so the panes draw a prompt's or an editor's Nerd
Font icons whichever text face is chosen; a pane re-measures its cells when it changes. The
interface font, offered from every family, goes in front of the theme's bundled face (Geist unless the theme bundles another) and then the system's
own, and draws the rest. The window is always built transparent, since that can only
be set as it is made, so the opacity is only the page's alpha on its large fills — the sidebar, the
strip, the stages and the terminal's background — and never on text or borders, and moving it never
rebuilds the window. What shows through is Clear, or on Windows Mica (Windows 11 only, greyed out
with the reason elsewhere) or Acrylic, which `settings/vibrancy` puts on and takes off the live
window through `window-vibrancy`; elsewhere only Clear is offered. A notification names the pane,
what came of it and where, for an agent turning waiting, needs-review or failed, never for the pane
watched in a focused window, and by default only while the window is in the background: the window's
version of the terminal client's bell, beside its chime. The leader and `f` fill the window with the
pane, putting the sidebar away until the same keys again. The console (`panes/console`) is a fourth
place in the nav, keyed 4 (in a pane, the leader and 4): the daemon's consoles
(`ServerMsg::Consoles`) as tabs, `+` asking for one (`SpawnConsole`) that appears once the daemon's
consoles hold it, and a tab's close ending it. It is docked beside the Workspace, Feature and
Checkouts stages — at the bottom until it is moved, by its head's dock menu or the palette's Dock
the console on the left, on the right or at the bottom — with a sash to resize it, fills the stage
as the Console, fills all but the sidebar's projects fullscreen (the
leader and `z`; the nav and the sidebar's foot are put away, the strip stays for the window's
buttons), or is hidden, a `›_` in the sidebar's foot and `` ` `` (the leader and `` ` ``) showing it
again. The Workspace's pane and the console each have a screen of their own, so a docked console
and an agent are on screen together and only the keys move between them: a console shown or picked
takes them, and the Workspace's pane takes them back when it is pressed, chosen on the rail, or
asked for by the Workspace's key, or when the console hides and lets its panes go. The console
splits in two side by side (its split button, the leader and `\`, or the palette), the second side
showing the first tab on neither side, or a console asked for when every one is shown. Each side —
the Workspace's and the console's two — is its own screen (`panes/shown` keeps a subscription, a
`Screen`, a room and a size request per side, and every frame names its side), one of them has the
keys, and the view the window reports names that one, so the daemon's sizing and the chime follow
it. The page's one text area for the keys goes into the screen that has them. A press on a screen
gives it the keys, and the leader and `o` hands them to the next one drawn; a tab picked replaces
the console's side with them; the wheel scrolls the side under it. A side's `✕`, the split button
again (keeping the side with the keys), or a side's console ending puts the split away, the other
side filling the console, and keys a console held that ended go to the console's other side, or
else to the Workspace's pane. Where it is docked, whether it shows, and the dock's size are saved
in `desktop.json`; fullscreen, the split and the tabs chosen are the run's, since a restarted
daemon's consoles are new shells under new ids. The Feature stage
has them while it is open, and another pane brings them back for good.

Which stage is up is `layout/stage`'s: one `connection::page::Stage` set through `go`, or a page
opened over one, which closing, docking or its view going takes back to the stage under it and going
anywhere else closes. The rail goes to the page whenever it differs from the last one sent, so no
move has to say it changed it. It says too whether the Workspace's panes are drawn — only while its
stage is up and no fullscreen console covers it, the composer covering them as every other stage
does — and the page hides them by the rule that takes their keys. `go`'s callers hand the keys on
and put fullscreen away themselves, since half the ways out of fullscreen move no stage.

The docks (`layout/docks`) are the stage's left, right and bottom edges; the sidebar keeps the window's
own left edge, so the left dock is between it and the stage, and the bottom dock is under the stage
alone. A panel is the console (at the bottom until moved), the views agents showed (on the right),
or one extension's view (where its extension's `place` says, the first time it opens), or a saved view
(on the right, the first time it opens). A panel the
person placed stays placed whether or not it shows or even exists, so hidden and shown again it
comes back where it was put, and an extension's view gone with its extension finds its place again
when it comes back; an extension's view may instead be a page on the stage, which the console and
the views never are. A dock shows one of the panels in it that exist now, is open or closed, and is
as wide or tall as its sash left it; holding more than one, it draws a tab per panel over them, a
tab's right-click offering the other docks. Each panel's own header has the same dock menu, drawn by
one button (`layout/docks.ts`'s `dockMenuButton`); what a panel can do there — float, take the
stage, close for good — is the docks' to say (`PresentPanel`), and the page reads it from
`layout/panels.ts`, never from the panel's name. A panel is dragged where it goes
(`layout/drag.ts`): its dock tab, its header, or an
extension's view's row in the sidebar picked up, the place it would land shown under the pointer —
an edge of the stage for that dock, the stage's foot for the bottom one, the rest for the stage
itself where the docks say the panel takes it (the Console stage, an extension's page) — and the
same move the menus send reported when it is let go (`DockGesture::Stage` for the stage); Escape
puts it back, and a press that never travels stays a click. So
several panels are on screen at once, one per dock — a console on the left, the agents' views on
the right, an extension's table at the bottom — with only the keys moving between them. The Console
stage and fullscreen take the console out of its dock without moving it, and leaving them puts it
back. A panel moved to a dock is shown there, out of fullscreen, the Console stage or the page, and
the dock it left shows the tab before it, or closes once empty. A `desktop.json` from before the
docks keeps its console docked at the bottom as tall as it was, its views panel's width as the
right dock's, and each extension view where the person had put it. Rust holds the model
(`session`); the page draws what it is sent and reports gestures. The one exception is the Code
stage's editor, whose buffer is the page's: a key must be drawn in the handler that heard it, with
no round trip to Rust (see "Editing a file"). It is TypeScript in
`crates/argus-desktop/page/src`, one module per subject, in folders named as the window's Rust ones
are, so a surface's page module sits under the same folder as its Rust module. At the root are
`main`, `state` (what the page holds) and `chrome` (the sidebar, strip and foot, which draw every
stage). `kit/` is what surfaces are built from: `dom`, `menu` (menus over the stage, and what a
right-click opens), `middle` (a middle click closing the tab or row it lands on) and `clock`.
`connection/` holds `tauri`, the page's channel to Rust. `system/` is what the page asks of the
machine: `frame`, `copy` and `chime`. `settings/` holds `preferences` and `font_picker` (a font
field and the installed families under it). `chrome/` holds `projects`, the project heading and its
switcher. `layout/` is where each surface sits: `docks`, `floats` (the dialogs a saved view floats
in, moved by the header and resized by the corner), `drag`, `panels` (what Rust says each panel
that exists is and can do) and `edge` (a side column's edge, dragged). `panes/` holds `screen`,
`input`, `split` and `split_math`, `size_label` (what the header says of the grid), `console` and `rename`. `keys/` holds `keymap`, `focus` (which
layer has a key), `chords` (how a key is written), `palette`, `sections` (the sheet's sections, filter and opening place) and
`shortcuts`. `browser/` holds `stage` (the overview tiles and the sidebar rows), `pane` (a browser
pane's grid cell), `shots` (the pictures and the canvases that paint them), `page` (the browsers as
Rust last said), `status` (who has it) and `input`. `views/` is the views
agents show and save: `agent` is the agents' views panel and `saved` the saved views', its parts in
`saved/` (`ctx`, `logic` and `link`); `blocks` draws a view's blocks and hands each to the module
of its kind in `blocks/` (`markdown`, `fields`, `table`, `list`, `stats`, `callout`, `code`,
`diagram`, `card`, `grid`, `details`, `row`, `chart`, `progress`, `timeline`, `tree`, `graph` and
`graph_layout`, `figure` (a graph or sequence diagram opened larger over the window, zoomed and
dragged) and `zoom` (where it sits there), `patch` (a diff block), `embed`, `matrix`, `checklist`, `log`, `board` and `form`),
a table's parts in `blocks/table/` (`bar`, `details`, `sort`, `memory` and `cell_text`); beneath
them `values` draws one value, with `refs` for those that point into Argus, `chip` and `views/link`;
`actions` draws the buttons an agent put in a view; and `views/ctx` and `system/copy` serve them
all. `extensions/` holds `views` and `dialogs` (an extension's question). `agents/` holds `compose`,
`sessions_dialog` and `transcript`. `checkouts/stage` and `code/stage` draw the Checkouts and Code
stages, `code/editor` the Code stage's editor, `code/sync` the editor's buffer kept in step with the
disk, `code/colours` its syntax colours, and in `code/grammar/` the tree-sitter grammars they
come from: `known` (the pinned registry), `fetch` (what to offer and downloading it), `store` (the
installed ones) and `load` (the runtime and each language, loaded once). They are loaded the first
time a file is shown, with `vendor/codemirror`, CodeMirror and tree-sitter's WebAssembly runtime
bundled by esbuild in `npm run build` (Checkouts is a slim table of Checkout, State and Last commit, State saying nothing for a
clean idle checkout, with the selected checkout in a side column that the files panel replaces
while open; `checkouts/says` words the State cell); `checkouts/files` draws a checkout's files beside the Checkouts table, a column of it, or
a drawer over it when the stage is under 900px, listed by the daemon a folder at a time (`ListDir`,
`FILES`) off the disk, untracked and ignored files included, and marked by git. Both draw their tree with `kit/file_tree` (`kit/ignored_fold` holds the top-level ignored folders in one row); Code's one field takes a leading `#` as a text search. `review/` holds `dialog`, `history` and `comment_box` (the comment box under picked lines,
the review's and the Code stage's). `board/` holds `feature_stage` and `capture`, the lenses in
`lenses/` (`work`, `record`, `flows` and `sequence`), and in `tracker/` `link`, `reconcile` and
`tickets` (a feature's ticket links). It is compiled into `assets/js` and committed, so building
the crate needs no Node. The events it reads are `connection/page.rs`'s types and the page and
gesture types each surface's module keeps beside it (`panes/console`, `layout/docks`,
`board/capture_note`, `checkouts/table`, `code/stage`, `extensions/views`, `extensions/keys`,
`review/comment_box`, …), written as TypeScript by `cargo test` into `page/src/bindings`; CI fails
when either committed output differs from what its source makes. Its webview keeps its storage
under the instance's config directory, so a dev window never shares it with the installed one, and
it logs to `argus-desktop.log` beside the config (the GPUI window to `argus-desktop-next.log`), keeping the previous run's.

The daemon is started with its stderr on the null device, since it shares no console with the
client and anything it printed would either land in the middle of the TUI or open a console window
of its own. So it logs to a file beside its config as well, keeping the previous run's log — a
daemon that is starting has often just stopped in a way somebody wants to read about. `RUST_LOG`
sets the level, `info` by default. Failing to open the log is never a reason not to start.

The daemon listens on a Unix socket or Windows named pipe. Messages are named MessagePack
records framed by a four-byte big-endian length. Frames larger than 64 MiB are rejected.
Several clients may connect at once and each connection may subscribe to several pane screens.
There is no transport authentication.

A message a side cannot decode — one a newer peer added — is skipped, and the connection carries
on. The whole frame is read before it is decoded, so the stream is still aligned; only a frame over
the size cap, which is never read, or an I/O error ends a connection. Every reader goes through
`read_known_msg`. Peers from before this skip unknown messages by hanging up, so nothing new may be
sent to one.

What is new is negotiated by a greeting, shaped around never sending an older peer a message it
cannot read. The client's first message is `Hello`: a protocol number, its version, and the optional
messages and encodings it can take, by name. The daemon sends its opening messages — tree,
templates, workspaces — as it always has, and answers a `Hello` with its own whenever one arrives;
it never sends one unasked, so a client that does not greet is never sent it. The client reads on
until the answer, keeping what came first for the app. A daemon from before the greeting hangs up on
it right after its opening messages, and the client connects again without greeting. One that takes
the greeting and says nothing for three seconds is kept, with nothing optional assumed. Either side
uses an optional message or encoding only when the other listed it, and the protocol number is
raised only for a change nothing can be negotiated across.

What a client is pushed comes in topics — the tree with its telemetry and consoles, the workspaces,
the feature boards, the focus, agents' views, saved views, and extensions. A client joins the tree,
the workspaces and the boards on connecting and the others by asking, or, for extensions, by
greeting with the capability. Each topic is forwarded on a task of its own, in the order its changes
were made, and says how a client that fell behind it is brought level, since a broadcast that
overflows loses what it held: the tree, the workspaces, the views and the saved views are sent
whole, each board the last time it was pushed (the daemon keeps the latest of each), and an
extension's views and commands whole with every dialog still put to that client. Order holds within
a topic and not across them, so no client relies on what one topic sent landing beside another's.

A client told of a daemon from another build says so on the status bar, with both versions and
`argus server restart`, which starts the daemon installed beside the client. A daemon on another
protocol number is an alarm rather than a note.

A client can attach to the daemon on another machine as well as its own. `argus --host <host>`
runs `ssh -T -o BatchMode=yes -- <host> argus bridge`, and the bridge on that machine carries its
daemon — started there the way any client starts one — on ssh's stdin and stdout. Everything the
daemon does stays beside the agents it runs: git, hook installs and the loopback hook server need
nothing new. ssh is in batch mode, keys or an agent only, because the terminal is the client's and
ssh has nowhere to ask; when it fails, what it printed becomes what to do — a key to add, a host
key to accept with `ssh <host>` in a terminal, argus to install there. Connecting never installs
argus on the other machine, as VS Code Server does: a client that drops binaries on another host
looks like malware to antivirus, and installing software is the user's act, so a missing or older
argus there is an error that says so. The `--` keeps a host name from passing for an ssh option,
and keepalives notice a link that died quietly within a minute.

Each daemon is a host with an app of its own — its tree, grids, selections and connection — rather
than one app with every id qualified by the daemon it came from, since hosts sit a level above
workspaces and one is on screen at a time. The host on screen is drawn and takes input; the others
keep their connections. A host that loses its daemon reconnects the way it first connected, and a
remote daemon from another build is named along with the host to update.

`W` opens the host picker, a level above the workspace picker. It lists this machine first, then
the hosts attached, then those this client has reached before — remembered in `client.toml` — and
the plain `Host` names in `~/.ssh/config` and the files it `Include`s; patterns name no host anyone
could pick and are left out. The host on screen and the attached ones are marked. Choosing an
attached host puts it on screen; choosing any other, or typing a name nobody listed, connects to it
in the background, puts it on screen once connected, and remembers it, while ssh's failure lands on
the status bar. The product mark names the host on screen when it is not this machine. A host off
screen holds no pane subscriptions — nothing draws its grids, and a remote one would stream them
for nobody — and takes them back, sizes and all, when it returns. `client.toml`'s list of hosts is
written only when a host is remembered: each host's app holds a copy of the settings, and saving a
theme from one keeps the list on disk rather than its own. On a host elsewhere an editor always opens in a floating
pane: an external editor would be launched on that machine, where nobody is looking, and this
machine's editor command is left out so the daemon there picks its own.

A request that makes something — a shell, an agent, an editor, a worktree, a project, a
repository — may name itself with a `request_id`, and the daemon answers that client alone with
`Created`: the id of what it made. The client matches what was made by that id, never by where a
new row turns up in the next tree, since the tree is broadcast for everything and the first to
arrive need not carry the row; it acts once it holds both the answer and a tree containing the
id, in either order. A named request the daemon turns down — a make, a read answered by id, a
change to a feature, a review comment — is refused by that id: to a client that listed
`request-refusals`, with one `Refused` carrying the id, the reason and whether the request only
read, so a refused read the client no longer wants is dropped like its answer and a refused
change is always said. Any other client is refused in the shape it has always read, and a request
that names none — a keystroke, a git write, anything nothing waits on — with a bare `Error`;
which shape each request gets, both ways, is `connection::request_refusal`'s alone. An unnamed request (id
zero, what a client from before this sends) is never answered, so an older client never receives
a message it cannot read, and an older daemon ignores the name — a newer client talking to it
simply does not move to what it made. The desktop reads an older daemon's refusals as the new
shape; a bare `Error` from one names nothing, so it lets go of any change still waiting on one,
as the window always did. The terminal client does not list `request-refusals` yet: it counts its
reviews apart from what it makes, and an id alone could name either.

Where the person is working is the client's to know, since the selection is client state, so the
daemon is told. A client whose daemon listed `focus` sends a `View` (the project, checkout and pane
its selection names) whenever that changes, and again when its terminal regains focus, which the
client asks the terminal to report. A view is *active* while the terminal has focus, or always for a
terminal that never says; only an active view moves the focus to its client, so a tree reshuffling
the selection of a client in another window does not. The focus is the view of the client most
recently active, falling back to the one before when that client goes. A host taken off screen
reports a view of nothing, since the person is looking at another machine. A client that sends
`WatchFocus` is sent `Focus` at once, and again whenever a tree or a view changes what it says: the
view read against the daemon's own tree, so a pane is named where it is now rather than where the
client last saw it. `argus focus` is that client. It prints the focus as JSON and exits, or with
`--watch` prints a line per change, `null` while the daemon is gone, and connects again once it is
back. It never starts a daemon, because a status line runs it every few seconds. The view decides
only what the daemon says about where the person is; every client is still sent the same trees and
boards.

## Where each responsibility lives

One rule decides which file a thing goes in: a module is named after the question it answers, and
answers only that one. Nothing is split across two files, and no file holds two subjects. Tests sit
beside the code they cover, in a `tests/` module split the same way.

`argus-protocol` is everything the three binaries have to agree on, and nothing else. Anything
written on both sides of a boundary belongs here, because three binaries share no types unless they
live in this crate and a contract written twice drifts in silence.

| module | answers |
| --- | --- |
| `connection` | how a client and the daemon talk |
| `connection/message` | what a client asks for, and what the daemon sends back |
| `connection/hello` | what each side of a connection can take, said before anything else, and how a client's greeting went |
| `connection/framing`, `connection/transport` | a frame, and the endpoint it travels over |
| `connection/request_refusal` | how a request the daemon turned down is told: by its id, why and whether it only read, to a client that listed `request-refusals`; the old shape each other client is sent; and an older daemon's shapes read back as the new one |
| `connection/awaited` | requests a client waits on the daemon to make something for, matched by the id it answers with, and which of the things made the tree now holds |
| `connection/ssh_hosts` | the hosts the user's ssh config names, for either client's host picker |
| `connection/ssh_bridge` | the ssh command that runs `argus bridge` on another machine, and what ssh's failure means |
| `tree` | what a client renders, which pane state outranks which, and what a row standing for several shows |
| `tree/held` | a client's copy of the tree, and finding things in it: every pane and checkout, where a pane or checkout sits, telemetry applied between trees, and which panes changed state from the copy before |
| `tree/ids` | the identities the daemon hands out, one type per kind of node so a pane id cannot stand where a checkout id belongs |
| `tree/focus` | where the person is working: the view a client reports, and the focus the daemon makes of it and `argus focus` prints |
| `tree/column` | the order a repository's checkouts column lists its rows: the main branch first, then the checkouts, then the branches without one while they are shown |
| `tree/column/verbs` | what a row of that column allows: the request each verb on it sends — remove, new worktree, switch, fetch, pull — or the words it is refused in, which checkout git runs in, and what a yes to the forced deletion sends |
| `hook` | the pane API's URLs, environment, headers and flags, and the largest body each endpoint takes — `argus-hook` builds what the daemon parses — an extension's endpoints among them |
| `hook/inbox` | what the daemon tells a harness's plugin over its inbox |
| `hook/call` | what each pane-API endpoint is posted and answers with: the one declaration `argus-hook` calls through and the daemon answers through |
| `extensions` | what an extension publishes for Argus to draw — its views, commands and dialogs — and what the person does with them on the way back: what taking an action on a view carries, a row or none, or the words it is refused in |
| `extensions/console` | what an extension's console view runs and where — its command, working directory and environment as the extension asks, and the pane and ending the daemon reports |
| `extensions/keys` | which keys an extension may bind, and the words a refused one is refused in: the keys Argus keeps where an extension's would be heard |
| `extensions/guide` | how to build an extension from nothing, as `argus extension guide` prints it: a walkthrough, then the reference, its examples read through the types and its limits filled from the constants |
| `panes` | a pane's terminal as a client holds and drives it |
| `panes/cell` | a screen cell on the wire, and the per-cell span an older client is sent its damage in |
| `panes/damage` | what changed on a pane's screen, as runs of cells and regions that scrolled |
| `panes/keys` | the bytes a key press is to a pane's child, decided once so every client types the same thing |
| `panes/mouse` | the bytes a mouse event is to a pane's child that asked for them, decided once so every client reports the mouse the same way |
| `panes/paste` | what a client sends as a paste: text with its line endings as a pty wants them, and how large a file may be to cross |
| `panes/view` | a pane's view as a client holds it: the live grid, the view parked in its history, how the daemon's pane messages change them, and what a scroll gesture over the pane sends — the child's bytes, a request for history, or nothing |
| `ago` | how long ago something happened, as every client says it |
| `review` | the shapes review data travel in: a checkout's uncommitted changes and a commit against its first parent, as rendered diffs |
| `review/view` | a diff flattened into the rows a viewer moves over, unified or split, and what a comment on the picked ones anchors to |
| `review/session` | one review's exchange with the daemon, as both clients hold it: which sides were asked for and answered, the side on screen and when an empty one gives way, and the picked lines a comment from it is anchored to |
| `review/agent_comment` | a comment on picked lines to an agent, from a review, a commit's review or the Code stage in either client: whether one may be written, which agent hears it as agents come and go, the words each refusal is said in, and the message that sends it |
| `review/captures` | what a tree-sitter capture name means as a highlight kind, read by its first word: the one table the daemon's read view and the window's editor colour by, so a token is the same colour in a diff and under the cursor |
| `review/history_view` | a checkout's recent commits flattened into rows, each unfolding to the files it touched once they are asked for |
| `code` | a checkout's code as the Code stage reads it: which version, a file's lines and the ones that version changed, what the file defines, and where text occurs across the checkout |
| `code/edit` | a working file edited live: the changes the daemon and a client trade, positioned in UTF-16 units of the text with CRLF read as LF, and the version each is made on |
| `files` | a checkout's folder as it is on disk, one level: each entry's kind, size and age, and what Git makes of it, with the cap on how many one listing carries |
| `paths` | where Argus keeps its files and which instance it is, shared so `ARGUS_CONFIG_DIR` points the daemon and every client at one place |
| `fuzzy` | which items a typed query matches, best first, so a picker in either client finds the same ones |
| `board` | a repository's feature board |
| `board/features`, `board/tasks`, `board/decisions`, `board/diagrams` | a feature board's parts: its scope, what is left to do, why it is built that way, and how it flows |
| `board/tracker_links` | how a repository's work reaches the team's trackers: which tickets a feature is linked to, how a ticket's tracker and key are read out of its URL, and the board's tracker map and its draft |
| `board/reconcile` | reconciling a feature with its tickets: what an agent proposes to carry between Argus and each tracker, which lines the person ticks and in what words, and what changed on Argus's side since the last one was carried out |
| `board/capture` | how one note reads as a feature — its title, brief, tasks and decisions — and what writing it down asks of the daemon |
| `board/decisions/adr` | how a decision reads as a record somebody can paste or correct |
| `board/outline` | how a feature's tasks and the decisions raised under them read as one outline: what binds a task, what carries a decision out, what an open one holds up |
| `board/diagrams/flow` | the steps a sequence diagram's source says, in the order it says them, the lifelines they run between, and the source rewritten to bind a step, retell it, move it, or move a lifeline |
| `board/follower` | how a client follows one repository's board: when it asks for the board and the feature's lists, which pushes it keeps, and where the cursor lands on a feature, task or diagram when one is replaced — on a feature only among those the client's index lists, which the client says (`FeatureListing`) |
| `board/standing` | how a feature stands on the index — Active, Queued, Done, Held or Archived — what a row of the index says, and which task comes next |
| `board/artifacts` | how shared work is bounded when read or written |
| `agents` | what Argus holds for the agents in its panes |
| `agents/launch` | what an agent asks for when it starts another agent in a pane of its own — the template, the first message, and the checkout, new worktree or other repository's checkout it works in — and what it is told it started |
| `agents/memory` | the working memory an agent receives for one request |
| `agents/roster` | which agents Argus is running and where each is, as an agent asks for them: the pane id a view links another by, its own project first |
| `agents/sessions` | a repository's agent sessions as listed: the harness and id that resume each, its checkout or the removed worktree it ran in, title, times and turns, and the pane running it |
| `agents/transcript` | what a past session said, in one shape whichever harness wrote it — its turns, replies and tool calls, its title and times — and whether its harness keeps a transcript at all |
| `agents/typed` | what may be typed into an agent's terminal: one line with no control character in it |
| `views` | the views agents show and keep |
| `views/agent_view` | what a view an agent shows is made of — its blocks and values and the change an agent sends to show, patch or close one — as the desktop and the terminal client both draw it |
| `views/check` | whether a view keeps its limits (sizes, its serialised size among them, depth, counts, bounded numbers, unique and printable ids, web links, checkout-relative paths, kinds this Argus knows), and where it does not, the path that names the fault |
| `views/saved` | what a saved view is: its body, its scope, and the metadata an agent refreshes it from (recipe, sources, the commit it was built at, stale); what an agent writes to save one, the largest body that save may arrive in, and what it reads back |
| `views/tree` | what each kind of block holds: the lists of blocks inside a container and the links, values and actions it holds itself, each by the step a refusal names it with, and each list as the desktop page names it |
| `views/links` | which saved views a view links to, in the order it names them, reached through values, `opens` and embeds at any depth: the terminal's links list and the desktop's nesting of a linked view under the view that names it |
| `views/press` | what a press of a view's action carries back, which of it the view offered, and whether it is relayed to the agent or refused in the person's words |
| `views/requests` | what an agent is typed when the person acts on one of its views — a press, a refresh of a shown view, a refresh of a saved one — each on one line with no control character in it |
| `views/patch` | how a patch replaces one block of a held view: the block path an agent names it by, and the view with that block swapped |
| `checkout_path` | whether a path an agent wrote stays inside the checkout it is read against |
| `browser` | a browser the daemon runs for an agent's pane: what an agent asks of it, what a client is shown of it, and what the person's hands do to it |

`argusd` owns PTYs, Git, and everything that outlives a client. Its state is one `Daemon` type
behind a small set of mutexes; the split below is which file a concern is *read* in, not a change
to the type or its locking.

| module | answers |
| --- | --- |
| `main` | daemon startup, and running until asked to stop |
| `state` | what the tree is, and what a client is shown of it |
| `state/build` | turning what the config declares into the tree the daemon runs on |
| `state/pushes` | what the daemon publishes for connections to push: one channel per topic, how deep each is, and the latest board of each key for a connection that fell behind |
| `state/panes` | a pane's lifecycle: spawned, restarted, written to, and retired — the one way out of the tree every removal takes |
| `state/processes` | starting a pane's process and driving it once it runs: the port every pane is started through, a pty in a running daemon and a scripted process in tests |
| `state/scripted` | a daemon over pane processes and a clock the test drives, for tests about exits and durations and for the test host |
| `state/clock` | what time it is for the durations the daemon decides on — a resume's grace, a restart loop's window, a quiet child's age |
| `state/agents` | what an agent reports about itself, and what it is told |
| `state/agents/launch` | an agent starting another: which checkout it lands in, which template it gets, and who may ask |
| `state/agents/follow` | which checkout a pane belongs to: the one its agent works in, moved to when the agent says so or a status report says its turn ended in another worktree, and the primary checkout once its worktree is removed outside Argus |
| `state/relocate` | a person moving a pane to another checkout of its project: an agent told where it works now, a shell at its prompt sent a `cd` there, and one running something refused, so a row never claims a directory its process is not in |
| `state/viewers` | what each client is showing: the one pty size reconciled out of what they all ask for, and the view of the client the person was last at |
| `state/git_ops` | the writes to Git |
| `state/git_reads` | the port the tree's reads of Git go through — status, HEAD, worktrees, a root's repositories — so a test can script what Git answers |
| `state/sync` | the polls and watchers that keep the tree level with the disk |
| `state/panel` | the rows the user adds and removes by hand |
| `state/workspaces` | which scope is open, daemon-wide |
| `state/extensions` | what the running extensions publish — views, commands, open dialogs — which client a dialog is put to, and what the person does with them on its way back |
| `state/extensions/consoles` | the consoles extensions' views run: started when a view is published, kept while it asks for the same run, ended with the view or the extension, and their ending recorded on the view |
| `state/extensions/running` | running the configured extensions: each started with its own URL and token, started again when it exits, stopped with the daemon |
| `state/session` | what survives a daemon restart |
| `state/agents/sessions` | a repository's agent sessions: its harnesses' transcripts for its checkouts joined to the agent panes running them, what one session said, and reopening one by its id |
| `state/agents/inbox` | how what Argus says to an agent reaches it: through its harness's inbox when one is open, typed into its terminal otherwise |
| `state/tree` | finding your way around the tree |
| `state/board`, `state/board/features` | a feature board and who may change it; which feature a checkout is on, and the one apply a change to a feature takes, with the board's address and who asked as parameters |
| `state/board/tasks`, `state/board/decisions`, `state/board/diagrams`, `state/board/tracker_map` | a feature board's rows: how a task or a diagram is stored, a decision added, changed and read, and the tracker map written or drafted — each change one apply, whoever asks |
| `state/board/reconcile` | a feature's reconcile with its tickets: who may ask, propose, tick and report, side by side in one apply, and what the agent is told at each turn |
| `state/board/parts` | which feature a task or diagram request lands in, which rows it may touch, and which lists its change pushes |
| `state/board/address` | which board a request addresses, and who hears of it: an agent's pane or a client's checkout resolved once into the board's store key, its project and the repositories a push of it names |
| `state/board/asker` | who is asking for a change to a board — a person at a client, or an agent with its pane and session — and how a row an agent left names it |
| `state/views` | which views agents have shown, per pane, and which go when |
| `state/browsers` | which browser panes run a browser, where each is and its latest picture, and which go when: the browser pane in its agent's checkout, owned by that agent |
| `state/views/timers` | which views are being asked to refresh, and the task that asks |
| `state/views/saved` | the views agents kept: saving, marking stale, deleting, refreshing and pressing them, which of them an agent may reach, what each client is told as the rows or the tree change, and the staleness read off the tree |
| `connection` | one client connection: which task each message runs on, and the topics it has joined |
| `connection/topics` | what a connection is pushed: each topic's first send, what each change sends, and how a client that fell behind is brought level |
| `connection/dispatch` | what each client message does, and the one way its answer or refusal reaches the client that asked |
| `connection/accept` | taking clients off the listener, each onto a connection task of its own, until the daemon is asked to stop |
| `test_host` | a whole daemon inside a test process, on a private endpoint with its own store, scripted pane processes and clock, and a fixed repository: what tests that speak to it as a client stand on |
| `test_host/transcript` | one client's conversation with a test host, written down step by step: a request, then everything said until the daemon goes quiet |
| `test_host/scrub` | what in a transcript differs between runs and machines while behavior does not — the host's directory, the build's version, the wall clock — in words that stay put |
| `test_host/render` | how a message is written in a transcript: runs folded, highlight spans on a line, and a kind seen before written as what changed |
| `test_host/requests` | every message a client can send, generated against what a host's fixture holds — the fixture's ids, which variant a message is, and the one strategy per variant that the compiler holds every variant to |
| `test_host/requests/values` | what requests are made of: ids mostly the fixture's and sometimes nobody's, short text, and paths kept under the host's directory and filled in when sent |
| `test_host/requests/panes` | generated pane requests: starting, watching, typing into, sizing, naming and ending one, and an editor pane that never starts a real editor |
| `test_host/requests/git` | generated requests about a checkout's Git and code: worktrees, branches, review, history, files, search and recorded sessions |
| `test_host/requests/board` | generated feature-board requests: features, tasks, decisions, diagrams, the tracker map and reconcile |
| `test_host/requests/panel` | generated requests about the panel's rows: workspaces, projects, repositories and the directory browser, every path under the host's directory |
| `test_host/requests/views` | generated requests about what agents and extensions show: views, saved views, presses, extension actions, commands and dialogs |
| `test_host/requests/session` | generated greetings: this build's, one missing capabilities, one on another protocol |
| `hook` | the loopback pane API: which daemon call each agent's or extension's request reaches, behind its token |
| `hook/devtools` | the pane API's DevTools socket: an agent's browser tool attached to its pane's browser, behind the pane API's token |
| `hook/http` | the pane API's HTTP: a request's line, headers and bounded body read off the socket, the answer written back, and an event stream held open with its heartbeat |
| `review`, `review/queue` | a review's diff, and when it is generated and which asks it answers: one at a time across the daemon, each client holding one place in that turn and read in the order it asked, an ask repeated while its twin waits answered by that one diff, asks for different checkouts, sides or commits each read on their own, and every ask answered |
| `review/diff` | a review's stable snapshot and its diff, and a commit's |
| `pty`, `pty/job`, `pty/console_host`, `pty/vt` | a pane's child process; launching it, bounding what it starts and ending all of it; which process is its Windows console server; and its terminal emulator with the translation of its screen |
| `pty/pace` | when a busy pane's next frame may go, and the fine system timer on Windows that keeps it to the interval |
| `pty/cd` | moving a shell to another directory by typing into it: which shell it is, whether it waits at its prompt with nothing of its own running, and the keys that clear its line and `cd` it in its own syntax |
| `browser` | a browser run for an agent's pane: started, followed while it runs, and ended when its handle is dropped |
| `browser/process` | a browser pane's process: what a terminal's keys, sizes and scrollback come to for a pane whose content is a browser, which is nothing |
| `browser/chrome` | which browser runs, and starting it: headless, on a profile of its own that goes with it and that the next daemon clears if this one is killed, handed its pipe |
| `browser/pipe` | the DevTools pipe to a browser: two anonymous pipes it is started holding, in place of a port any process could reach, and the NUL-ended messages on them |
| `browser/mux` | one DevTools connection shared by the daemon's view of a browser and agents' tools: ids rewritten so answers find their way back, events sent to who can use them, the tab an agent acts on told to the daemon, and an agent's input refused while the person holds the browser |
| `browser/cdp` | the daemon's own DevTools client of a browser, on a line of its mux: commands sent and their answers matched back by id, and everything else read as an event |
| `browser/screen` | which of a browser's tabs is shown, and showing it: laid out at the size the person sees it, its pictures read off as the page changes, and the person's input put on it |
| `browser/act` | what an agent's built-in browser commands do to the tab the person watches: each a few DevTools calls on the daemon's own session, a load waited for, a picture saved |
| `browser/input` | what the person's hand on a browser becomes: the DevTools commands, in order, that do it to the page shown |
| `harness`, `harness/install`, `harness/hooks` | what a CLI is, what gets written into a checkout for it, and the command lines in it |
| `harness/skill` | the skill package an agent receives and the short message that leads it there |
| `harness/plugin_dir` | the plugin folder a harness loads from outside the checkout — Claude Code's mod — and the variable that points a pane at it |
| `harness/transcript` | reading a session's transcript file, in its harness's format, into the protocol's transcript |
| `harness/transcript/locate` | where transcripts are on disk, from a harness's path templates: the one a session's id names, and every one kept for a set of checkouts |
| `harness/transcript/listing` | which sessions the harnesses kept for a repository's checkouts, newest first, and whether a directory one ran in was a worktree of the repository since removed |
| `harness/transcript/glance` | how much of a transcript a listing reads: a short one whole, a long one at its two ends |
| `harness/transcript/claude`, `harness/transcript/codex`, `harness/transcript/cursor`, `harness/transcript/pi`, `harness/transcript/neutral` | one transcript format each: which of its records are a turn, a reply or a tool call, and what they say of the session |
| `store`, `store/schema`, `store/legacy` | `runtime.db`, its tables, and the files it replaced |
| `store/saved_views` | the saved views as stored: one row per view, the features it is attached to, and what removing a feature does to them |
| `store/boards`, `store/reviews` | the feature boards and the review comments, as stored |
| `store/reconcile` | a feature's reconcile with its tickets as stored: the one under way and the last one carried out |
| `store/capture` | a whole note written down as a feature, in one transaction |
| `store/panel`, `store/session` | runtime changes to the panel and when each repository was last used, and the panes to bring back after a restart |
| `store/seen_sessions` | the sessions Argus saw agents run under a harness that keeps no transcript — title, checkout, first and last seen — so one still lists once its pane is gone |
| `store/faults` | a fault put in the store's connection by a test: a write stopped at any step or at its commit, a full disk, another writer's lock |
| `git`, `git/browse` | the read-only questions asked of a repository: its status and worktrees, and what a checkout contains for the pickers |
| `git/watch` | when a repository's Git metadata changes, as it happens rather than on the next poll |
| `git/edit_watch` | when a file is edited in a checkout being worked in, and which checkouts an edit could show up in the status of |
| `git/gitignore` | the repository-local excludes for generated Argus files |
| `syntax`, `syntax/grammar` | which tree-sitter grammar reads a file, chosen by its extension, so highlighting and outlines agree on its language |
| `syntax/highlight` | what each token of a diff's or a file's lines is, for highlighting |
| `code`, `code/search`, `code/symbols` | what the Code stage reads: a version's files and how much each changed, one file's lines with the ones it changed, where text occurs in the working tree, and what a file defines |
| `code/edit` | a working file held open for one connection to edit: its text and version, a client's change written only onto the version it was made on, and any other write read back, settled, and sent as a change as it lands |
| `code/edit/text` | a text's changes in the editor's positions: applying a client's change set whole or refusing it, and the change set between two texts, a line diff narrowed to the characters |
| `files` | one folder of a checkout read off disk, never past its root, with each entry marked changed, untracked or ignored |
| `config` | `projects.toml`, which is read and never written |
| `system`, `system/daemon_lock`, `system/logging`, `system/command`, `system/paths`, `system/editor` | what the daemon asks of the machine it runs on: the one daemon allowed to own an instance, where its diagnostics go, a command run without a console window, whether two spellings name one place, and the person's editor |
| `system/file_watch` | watching one file, `projects.toml`, through an editor's rename-over save and without waking on the log and store the daemon writes beside it |
| `system/pastes` | where a file pasted into a pane is kept on the pane's host, and how its path is spelled into the pane |
| `system/dll_search` | where the daemon loads libraries from on Windows: its own directory and System32, never the directory it was started in |
| `system/ctrl_c` | whether a pane's child can be interrupted on Windows: the daemon clears the ignore-Ctrl-C flag it was started with, so no child inherits it |
| `bin/argus-hook/main` | `argus-hook` startup: which command the arguments name, and naming the feature a board command is about |
| `bin/argus-hook/transport` | how a request reaches the daemon: the loopback post, the typed call each endpoint answers, which URL a pane's reports go to, and the credentials the environment carries |
| `bin/argus-hook/installed` | the form Argus writes into a harness's own hook config: the event the harness pipes in, and only the JSON its runner needs back |
| `bin/argus-hook/telemetry` | what model an agent runs, how full its context is, what it has spent and which tool it is in, for harnesses that report through hook commands |
| `bin/argus-hook/context` | what an agent reads before it starts: the pane's review comments and the feature the checkout is on, sized to sit in its context |
| `bin/argus-hook/comments` | `argus-hook comments`: the review comments addressed to the pane, one a line |
| `bin/argus-hook/board` | the commands that read and write a feature's board, one to a module |
| `bin/argus-hook/board/features` | `argus-hook feature`: the feature a checkout is on, read, listed or exported, and every write that opens, moves or links one |
| `bin/argus-hook/board/tasks` | `argus-hook task`: the feature's task tree, its writes parsed from arguments and the tree drawn for the model |
| `bin/argus-hook/board/diagrams` | `argus-hook diagram`: the feature's sequence diagrams, read and written as Mermaid source |
| `bin/argus-hook/board/decisions` | `argus-hook decisions` and `decide`: the decision board drawn as its tree, and the writes that add, answer, withdraw or rewrite a decision |
| `bin/argus-hook/board/tracker_map` | `argus-hook tracker-map`: how the repository maps to its trackers, and an agent's draft of one |
| `bin/argus-hook/board/reconcile` | `argus-hook reconcile`: reading the reconcile asked for, proposing its lines, and reporting what was carried out |
| `bin/argus-hook/view` | `argus-hook view`: showing the person a view of what the agent read, taking it down, and keeping one in the store |
| `bin/argus-hook/agents` | `argus-hook agents`: the other agents in every project of the open workspace, one a line with the pane id a view links them by |
| `bin/argus-hook/browser` | `argus-hook browser`: the pane's browser opened, answered with the DevTools URL a browser tool attaches to, or closed |
| `bin/argus-hook/launch` | `argus-hook agent`: an agent starting another in a pane of its own, here, in a new worktree or in another repository's checkout |
| `bin/argus-hook/inbox` | relaying the pane's inbox, one item a line, for a plugin that cannot hold the stream open itself |
| `bin/argus-hook/transcript` | appending a turn to a transcript in the neutral format, for a harness that keeps none of its own |

`argus` is a replaceable renderer over one model. `app` holds the state and never predicts the
result of a request; `ui` is a pure function of it.

| module | answers |
| --- | --- |
| `main` | the event loop, and which command the command line asks for |
| `system` | what this client asks of the machine it runs on and of the programs beside it |
| `system/sibling` | starting another of Argus's executables, found beside this one, so it outlives this process |
| `system/browser` | opening a ticket's URL in the person's browser, or copying it where no browser is at hand |
| `system/clipboard` | text crossing between Argus and the clipboard |
| `system/herdr` | what is reported outward to herdr: how many agents are working, and the most urgent |
| `commands` | the `argus` commands that are not the terminal UI (the daemon lifecycle commands are `connection/launch`'s) |
| `commands/bridge` | this machine's daemon on stdin and stdout, for a client on another machine to reach over ssh |
| `commands/desktop` | the `argus desktop` and `argus desktop-next` commands: opening a desktop window beside this terminal client |
| `commands/focus` | the `argus focus` command: where the person is working, as JSON, once or on every change |
| `commands/extension` | the `argus extension` command: the guide to building one, and the running extensions listed or one shown whole, as the daemon here or under `--host` describes them |
| `commands/update` | the `argus update` command: this installation's executables replaced with the newest release's and the daemon restarted onto them, or with `--check` only whether there is one |
| `commands/update/release` | which release is the newest, and which of its archives this machine runs, named as the release workflow names them |
| `commands/update/download` | fetching a release's files over HTTPS in this process, against the operating system's certificates, and an archive held to its published checksum |
| `commands/update/archive` | the executables in a release archive, read out of it under Argus's own names and nothing else |
| `commands/update/swap` | replacing the installed executables with a release's, all or none, the old ones renamed aside since Windows will not overwrite a running one |
| `commands/update/progress` | how an update shows what it is doing: a bar redrawn while the archive downloads, then a line per step, and plain lines when not on a terminal |
| `connection` | how this client reaches its daemons and talks to them |
| `connection/launch` | reaching this machine's daemon, starting one if nothing is listening, and the daemon lifecycle command |
| `connection/wire` | the socket a connection runs over: the two tasks between the app and the daemon, and a frame's subscription churn collapsed between them |
| `connection/remote` | reaching a daemon on another machine through ssh and its bridge, and saying why when ssh cannot |
| `connection/hosts` | the daemons this client is attached to, each with an app of its own, and which one is on screen |
| `terminal` | the screen the event loop runs over: owning the terminal for a run and presenting one frame at a time |
| `terminal/backend` | the ratatui backend, wrapped so the cursor does not thrash |
| `terminal/redraw` | which of the event loop's wake-ups is worth a frame |
| `terminal/profile` | an opt-in measurement of where a keystroke's time goes: painting the frame, or the round trip to the pty |
| `app` | the model: the tree, the selection, and which modal is up |
| `app/tree` | where the person is in the tree |
| `app/tree/nav` | where you are in the tree, and what moving does |
| `app/tree/rows` | naming a row independently of its index |
| `app/tree/rail` | what the rail lists, in what order, which repositories are open and which checkouts folded, and what choosing a row does |
| `app/gestures` | what the operator's gestures mean |
| `app/gestures/input`, `app/gestures/mouse`, `app/gestures/scroll` | a key, mode by mode; the mouse against the last frame; which pane a scroll moves |
| `app/gestures/layout` | where the last frame put things, and which of them has focus |
| `app/gestures/mode` | which mode has the keys — the one answer key dispatch, the status bar and `?` all match on |
| `app/gestures/mode/keymap` | what each mode's keys are, said once: the keys, what `?` and the bar call them, and what they run |
| `app/layers` | the layers that float over the stages |
| `app/layers/modal` | which layer is up — the picker, the floating window, the settings rows, the prompt — and what each holds |
| `app/layers/pickers` | the modal layers that ask the daemon, each opened with the rows it offers resolved |
| `app/layers/brief`, `app/layers/dirpicker` | the view state behind each overlay |
| `app/layers/diagram` | the Mermaid sequence-diagram view and its responsive overlay layout |
| `app/daemon` | the model's exchange with its daemon |
| `app/daemon/actions` | what is asked of the daemon |
| `app/daemon/server` | what arrives back, and what it does to the selection |
| `app/daemon/awaited` | what the terminal client does with what the daemon made for it — focus the pane, select the row, start what was asked for in the new checkout — once `awaited` says the tree, or the consoles beside it, holds it |
| `app/daemon/showing` | what the daemon is told this client shows |
| `app/stages` | what each stage holds as this client sees it |
| `app/stages/views` | which surface the content area holds, and the fetches opening one makes |
| `app/stages/feature` | where the Feature stage is — the index, a feature on a lens, the row under the cursor — and how the keys move between them |
| `app/stages/board_writes` | what the Feature stage asks the daemon to change: the line or note being typed, and the message each key sends |
| `app/stages/agent_views` | the views agents showed as this client holds them, newest first: which is open, how far it is scrolled, and the close and refresh requests that change nothing until the daemon answers |
| `app/stages/saved_views` | the saved views as the daemon sends them, which one the stage shows, the links offered from it and the trail of links followed to it, and the embeds drawn in place |
| `app/stages/consoles` | the shells that belong to no checkout: the daemon's list of them, which one the stage shows, and asking for another |
| `ui` | the frame, and where the cursor goes on it |
| `ui/card` | a card and the rows in it: its frame, one row, and how a list longer than the card scrolls |
| `ui/command_center` | the HTML-specified shell: where the rail and stage go, the tab strip above them, the stage heading, the first-run state, and the status vocabulary they share |
| `ui/command_center/views` | the tab strip naming the four stages, and which tab a click lands on |
| `ui/command_center/rail` | drawing the rail — its heading, the rows `app/tree/rail` lists, the divider between the active repositories and the quiet ones, and the consoles at its foot — and which row a click lands on |
| `ui/command_center/workspace`, `ui/command_center/feature`, `ui/command_center/checkouts`, `ui/command_center/agent_views` | one stage each, with the hit-test that shares its layout |
| `ui/command_center/feature/index`, `ui/command_center/feature/work`, `ui/command_center/feature/record`, `ui/command_center/feature/flows`, `ui/command_center/feature/capture` | what fills the Feature stage's frame: the index of features, each of the three lenses, and the note a feature is written as |
| `ui/command_center/agent_views/blocks` | an agent view's blocks as styled lines of a width, and which kinds are left to the desktop |
| `ui/command_center/agent_views/tone` | how an agent view's tones read as theme roles, and the chip drawn through them |
| `ui/command_center/agent_views/values` | what each kind of agent view value reads as in a terminal: styled text on a line, or wrapped lines for a long text |
| `ui/text` | fitting text to a width: ellipsis, wrapping a field or prose, one-lining |
| `ui/theme` | the palette: every colour the UI draws, by the role it plays |
| `ui/motion` | animation arithmetic: how far along a transition is at a given instant |
| `ui/prose` | markdown styled where it stands |
| `ui/status` | the command band: where you are, what just happened, and the keys that apply here |
| `ui/term` | a pane's live screen, drawn cell by cell, and where its cursor goes |
| `ui/layers` | what the renderer floats over the command center |
| `ui/layers/help` | the keymap window |
| `ui/layers/overlay`, `ui/layers/modals`, `ui/layers/review`, `ui/layers/history` | one drawn surface each: the floating window, the modal input layers, the diff viewer, and the commit list |
| `input` | what crosses between the person's terminal and a pane beyond a key the app acts on |
| `input/paste` | a burst of typed characters told back into a paste |
| `input/selection` | a pane's selected text |
| `input/dropped` | a file dropped on the terminal, recognized in what it pasted |
| `input/pty` | a terminal event as the bytes a pane's child expects to read, once the app has sent it to the pane |
| `input/pty/keys` | a terminal key event as the protocol's key, and the leader chord that is never sent |
| `input/pty/mouse` | a terminal mouse event as the protocol's, relative to the pane's area |
| `settings` | preferences, and where they live on disk |
| `fixtures` | the builders every test module shares: the tree, and a pane's view as the daemon's messages leave it |

`argus-desktop-core` is what the desktop window holds and says with no window in it: the session
over the daemon's tree, each surface's state, the events and calls the page and the daemon trade, and
what is remembered between runs. It names no windowing or platform crate, so the Tauri window and the
GPUI one both stand on it.

| module | answers |
| --- | --- |
| `session` | what this window holds of the daemon and routes between its parts — the tree, the project and repository chosen on the rail — and which part each message, refusal and page call is for |
| `connection` | how the window trades with the daemon and the page |
| `connection/daemon` | reaching this machine's daemon through `argus bridge`: starting the bridge, greeting, and connecting again when it goes; the state that holds the session is reached through `SessionOwner` |
| `connection/effects` | the order a session change's effects leave in — what it asks of the daemon, the link opened and text copied, the events, the settings kept, the alerts — and what the paste key reads off the clipboard, for every window that owns a session; where each goes, and the system calls themselves (the opener, the settings file, the clipboard through `Clipboard`), are the owner's |
| `connection/page` | what the page and the window's Rust say to each other: the events down its channel, and every call it makes as one `Call`; a surface that keeps what it last sent defines its own page and gestures, which these name |
| `connection/sent` | what the page was last sent of something it draws only while open, so it hears each change once and a closing once, or of something always on screen, so it hears only what differs |
| `system` | what the window asks of the machine it runs on |
| `system/chime` | the chime as numbers: two sine notes, 660 Hz then 880 Hz 0.12 s later, a 10 ms rise and an exponential fall, as samples and as a WAV, for any window to play |
| `system/clock` | times the daemon sends said in the person's own clock: the time and its day, how long ago, a time alone and a span, with the moment and the zone given rather than read so a test pins both |
| `system/logging` | where the window's own diagnostics go |
| `system/notifications` | the system notification an agent coming to need you raises: whether a tree's changes raise one, and what it says |
| `system/opener` | handing something to the system's opener: a link in a view, only an `http` or `https` one, and a file or folder of a checkout the file manager shows, only one inside it |
| `settings` | what the person chose for this window, what it remembers between runs, and what this machine offers to choose from |
| `settings/backdrop` | which backdrops this machine can draw behind a see-through window, and whether one may be chosen |
| `settings/flavours` | the themes the Settings cards offer, in order and by name, and what "follow the system" asks for when pressed and draws when it is on |
| `settings/font_picker` | what a font field's list does with no widget in it: the rows, the narrowing as it is typed, the active row, and when a name is chosen (picked, on Enter, or when the field is left) |
| `settings/fonts` | which font families to offer in the Settings dialog from the faces a machine has: each once, by every name the webview might know it by, monospaced if any of its faces is, and the note a name nothing is called gets |
| `settings/frame_border` | the colour of the border Windows 11 draws round a frameless window, per theme: the sidebar's rule as an opaque colour |
| `settings/preferences` | the Settings dialog's choices as the window holds them: each change the page makes, held to what this machine can draw, and whether the page has heard them and the file has kept them |
| `settings/remembered` | what the window remembers between runs: each repository's index layout, the agent it last started, whether it chimes, the side and layout a review opens on, where each panel is docked and how big each dock is, and the Settings dialog's choices |
| `chrome` | the rail and the status bar around the stage |
| `browser` | the browsers agents' panes run, as this window holds them: where each is, its owner and latest picture, and which the window draws |
| `browser/address` | what the address field turns typed text into: a bare host made `https://`, a server on this machine `http://`, a typed scheme kept, blank text none |
| `browser/key_input` | a key on a browser's focused picture as the browser takes it: a named key or text typed, and none for a Ctrl or Meta chord, which stays the window's |
| `browser/keys` | which pane has the keys and whether it is a browser, and the call that gives a cell the keys when a press lands in it |
| `browser/picture` | a browser's latest picture as bytes a window draws, decoded from the daemon's base64 once and shared, newest by a serial |
| `browser/status` | who has a browser, said once for a tile, a cell's strip and a nav sub-row: held, else the agent driving, else the person driving, else nothing |
| `browser/viewport` | which of a cell's reports of its room are told to the daemon: nothing under 50 px, nothing already told, a fast run held to its newest, and what a reconnect tells again |
| `browser/within` | a click's point in the box a picture is fitted into, as the page's own pixels, and none in the bands either side |
| `chrome/add_outcome` | what the add-a-project dialog says of what adding would do and what its Add button reads: the folder list's states, the outcome beside it per kind of add, the footer, and the pause before typing is sent |
| `chrome/add_project` | what the add-a-project dialog holds — the kind of add, the folder browsed and the daemon's listing of it, the repositories the daemon says it would add, the name and skips typed, and the add waiting on the daemon — what its gestures ask of the daemon, and the page and gestures it trades with the page, last sent while open |
| `chrome/empty` | what the stage says with no pane on screen: waiting on the daemon (and the one way out of a connection that does not answer), no project, or a project with nothing shown (the checkout a shell is offered on, the panes to go back to) |
| `chrome/projects` | what the project switcher says: the entries of the menu under the brand (projects with their tallies, add, workspaces, hosts, remove), what choosing a workspace or a host asks, and the words of the question before a project is removed |
| `chrome/cursor` | which rail row holds the keys and which checkout's + menu is open: the keys of a picked row and of the menu answered once for both windows, the pick following the tree, the menu's items |
| `chrome/rail` | what the rail lists from the tree, and what a row says |
| `chrome/status` | what the status bar says — the connection, its last failed try, an alarm about the daemon reached, whether the window chimes — and whether the page has heard it since it changed |
| `chrome/strip` | what the strip across the top of the stage says, as data: the crumb over the title, the status chip, the resumed note, whether the pane's menu is there, and the words each stage names itself by, with the words a stage's own view adds |
| `chrome/switcher` | what the project switcher offers beyond the tree's projects — the daemon's workspaces — and the project waiting on a yes to be removed |
| `layout` | where each surface sits in the window |
| `layout/docks` | where each panel sits: which dock — left, right or bottom — holds it among its tabs, whether it is a page, or a dialog floating over the stage with where it sits, how big, and which is in front; which tab each dock shows of the panels that exist, whether it is open and how big, whether that changed since it was saved; what each panel that exists can do — float, take the stage, close for good — and which kind it is, told to the page so it never reads them from a panel's name; the page and gestures of the docks, and what the page last heard |
| `layout/dragged_width` | how wide the person dragged a side column — the rail, an open feature's outline, the Code stage's tree and outline — held to its bounds, and whether that changed since it was saved |
| `layout/floats` | where a dialog may sit and how small it may get while a pointer holds it: the box moved or resized by the pointer's travel, no smaller than 280 by 160, kept with 120 px of its width and 40 px of its header in reach of the work area, so what a window draws live is what it reports on release |
| `layout/stage` | what fills the stage — one of the stages, or a page (an agent's view or an extension's) opened over one — where closing, docking, Escape or its view going takes the person back to, the Code stage's way back included, and whether the Workspace's panes are drawn under it |
| `panes` | the panes on screen and how each is drawn |
| `panes/console` | where the console is — docked, the Console stage, fullscreen or hidden — the daemon's consoles and the tab chosen for each side of a split, the page it draws and the gestures it takes, and what the page last heard |
| `panes/pointer` | what a pointer over a pane's screen is, with no window in it: the cell under it held inside the grid, a press that is the child's or the start of a selection, the move not worth a report, the release that copies, a press lost, and a wheel counted in notches |
| `panes/screen` | a pane's screen as the page draws it: the rows of its view changed since the last frame, as styled segments |
| `panes/shown` | the panes the window shows — the Workspace's grid, one per side of the console, and one per extension console view on screen — each side's subscription, size and view on this connection, which side has the keys, whether the Workspace's pane fills the window, and whether the leader waits on the next key |
| `panes/shown/workspace_grid` | the Workspace's grid: which panes it holds — the Workspace's own first, then those split beside it — where each sits and how big, which pane may join it and the refusal for one that may not, and the page the rail carries of it, its sizes in thousandths |
| `panes/shown/workspace_grid/gutter` | what dragging a gutter does to the two panes it parts: only they change, neither goes below the least a pane may be (its columns or rows and the room round them), a pair with no room for two stays, and the sizes come back as whole thousandths summing to exactly 1000 |
| `panes/shown/workspace_grid/rects` | where each cell of the grid falls, as fractions of the grid, from the rows' heights and each row's widths in thousandths as the rail carries them |
| `panes/shown/workspace_grid/split_grid` | where the Workspace's panes sit in relation to each other — rows, column fractions, row heights — and what adding, closing, swapping and moving a pane does to them, as arithmetic |
| `panes/shown/workspace_grid/zones` | which zone of which cell a carried pane would be dropped on: the nearest edge within a quarter of the cell, else the centre, the area each zone lights, and the cell under a point that is never the pane's own |
| `keys` | the window's keys and the command palette they open |
| `keys/chords` | how a key is written for a sheet, a palette row or a menu: the press with its modifiers, the leader's presses before a chorded key, a label in sentence case |
| `keys/keymap` | the window's keys: each binding's keys, where it works, what it does and what it is called, read by dispatch in Rust and on the page, the commands the palette runs, and the order of the layers that hold the keys outside a pane |
| `keys/page_key` | a key the page reports, as the protocol's key |
| `keys/palette` | what the command palette can go to: the project's panes and checkouts, and the chosen repository's features |
| `keys/rank` | how the command palette searches: the scope a leading `>` or `#` gives, the subsequence score, the rows a query keeps in each group and the characters to mark |
| `keys/results` | what the command palette lists: a row for each command, extension command and view, pane, feature and checkout with its mark, detail, aside and what choosing it asks, and where the highlight goes when the lists change |
| `keys/sheet` | what the shortcuts sheet lists and opens on: a section per place and per extension with the pairs of bindings written once, the search over the rows, and the section for where the person is |
| `views` | the views agents show and save |
| `views/agent` | which views agents showed, as the window holds them, which one their panel shows, which name a feature, and which wait on a refresh |
| `views/blocks` | the rules a view's blocks are laid out and read by, which both windows share |
| `views/blocks/chart` | where a chart block's axes, bars, lines and dots go, and what is said of them |
| `views/blocks/chart/layout` | everything of a line, bar or area chart in a box 480 by 160: the grid and its ticks, the thinned labels, each series as bars or runs of line with their dots, the dash and fill that tell series of one tone apart, and what hovering a mark reads |
| `views/blocks/chart/scale` | a chart's y axis: round ticks from at most zero to at least the data's top, three to five of them, whatever the data, and the extent of a long array found in a loop |
| `views/blocks/chart/spark` | a spark's points in a box 100 by 32, spread evenly, a flat series at mid height, and what a screen reader hears of it |
| `views/blocks/chart/words` | what a chart says in words: the compact numbers on its axis, what a reader that cannot see is told it shows, and what hovering a bar or a dot reads |
| `views/blocks/checklist` | a checklist's tally, "2 of 5 done", and what a reader that cannot see the box is told |
| `views/blocks/diagram` | where a sequence diagram's lifelines, arrows, self-calls and branch bands go, from the participants and rows Rust read out of the source: lifelines spaced until the longest message between neighbours fits, an undeclared participant given a lifeline of its own |
| `views/blocks/embed` | whether a saved view embedded in another is drawn, a cycle, too deep or missing, by the protocol's depth cap, and the line said instead |
| `views/blocks/flow` | how many equal columns fit a width, as the page's auto-fit and auto-fill grids count them |
| `views/blocks/form` | what a form block's field shows before it is touched, which typed text still stands (a choice no longer offered does not), the answers a press sends with a number that is not one left out, and the arrows' step within a number's bounds |
| `views/blocks/graph` | where a graph block's boxes and arrows go, and what is said of it |
| `views/blocks/graph/describe` | what a screen reader is told of a graph, its size and first six links, and the title its larger view carries |
| `views/blocks/graph/layout` | a graph's layered layout as numbers: cycles broken by reversing the edges a walk finds going back, longest-path layers, stand-ins so a long edge can bend, barycenter sweeps that cut crossings with ties kept in the agent's order, right-angled routes and loops, and the drawing started at its padding on both axes |
| `views/blocks/label` | how long an agent's label is let to be in a drawing and how wide it is taken to be, so a box is sized from its text before anything is measured |
| `views/blocks/log` | where a log is read from: whether it is at its end, within four pixels |
| `views/blocks/matrix` | how strongly a matrix cell is tinted, from a tenth to nine tenths, and its tooltip |
| `views/blocks/patch` | an agent's unified diff read into the review viewer's own rows: counts in a hunk header say where it ends, so a removed line reading `-- x` is told from the next file's `--- a/x`; text with no hunk is not a diff |
| `views/blocks/table` | the rules an agent's table block is sorted, filtered, picked from, copied, narrowed and sized by |
| `views/blocks/table/columns` | which columns of a table give way as its box narrows: an optional one at 560, and an extension's last four one at a time |
| `views/blocks/table/filter` | which rows a filter lets through, matched against the lowercased text of their cells, and the words a table says about how many it shows |
| `views/blocks/table/pick` | picking rows of a table: the keys that name rows, what the select-all box shows, ticking it leaving the rows a filter hides as they were, and the count and the actions that follow the pick |
| `views/blocks/table/sort` | how rows sort when a header is pressed: what each kind of cell sorts by, an unreadable date last going up and first coming down as the page's does, ties kept in the order drawn, and which way a press turns a column |
| `views/blocks/table/collate` | how a table's text orders as the page's `localeCompare` does, by the root collation with its data compiled in, and how a figure reads as text when it is compared against text, as `String(n)` writes it |
| `views/blocks/table/tsv` | a table as tab-separated text, a tab or newline in a cell made one space |
| `views/blocks/table/width` | how wide each column is taken to be from what its cells say, a long text cell cut at 280 |
| `views/blocks/timeline` | the lanes of a timeline in the order they first appear, and the lane each event is in |
| `views/blocks/zoom` | where a drawing sits in its larger view: how it opens, fit, zoom about a point, the wheel's factor and the percent it says |
| `views/changes` | what changed in an agent's view since the person last had it in front of them, named by the paths the page draws the blocks at, and whether each change flashes now or waits unseen |
| `views/head` | what the head of an agent's view says: the status pill's look for a tone, the meta line with the pane that read it and its age on the person's clock, a tab's tooltip, which view the panel and the page show, whether a dock shows the panel, and what the launcher's press asks |
| `views/refs` | where a reference in an agent's view points — the checkout, repository or pane it names — found from the pane that showed the view, which paths it refuses, and a feature it named waiting on its board |
| `views/saved` | the views agents saved as the window holds them, grouped by scope and told apart as here or elsewhere; each saved panel's trail of columns — a link followed keeps the columns up to it, a column closed or popped out into a panel of its own — and which saved views wait on a refresh or a press |
| `views/nav` | how the sidebar's Views fold lists the saved views: grouped by scope, filtered to here or all, nested under the view that links to them, lit while a panel in a dock shows them |
| `views/column` | what a saved column says of itself: `built <ago>` with the commit cut at 7 and the exact time, the Details rows with the commit cut at 12, and whether the panel has a head of its own (not a lone view under a dock's tabs) or floats |
| `views/offered` | which saved views the Feature stage offers for its open feature: those scoped or attached to it |
| `views/markdown` | the small Markdown a ticket or a CI log is written in, read into blocks both windows draw |
| `views/markdown/blocks` | Markdown read into blocks, the one entry `parse`: line ends and tabs made plain, then each line read in priority (blank, fence, heading, rule, quote to a depth of five, list, table, paragraph) |
| `views/markdown/inline` | the marks inside a run of text — code spans, emphasis, strikethrough, links, bare addresses — every string kept as text, every scan capped or remembered so it stays linear, nesting stopped at five |
| `views/markdown/leaves` | the text of a document in reading order, one leaf for each paragraph, heading, item, code block and table cell, a place in it as a leaf and a byte, and the text between two places as a copy joins it |
| `views/markdown/line` | what one line of Markdown is — blank, a fence, a heading, a rule, a quote, an item, a table separator — each the page's regular expression written out |
| `views/markdown/link` | which links are links at all (an http or https address with a host, normalised, and nothing else), where `[label](url "title")` ends within its limits, and where a bare address ends |
| `views/markdown/list` | a list one nested level deep: the kind a marker says, where a sub-item begins, what continues an item, and what ends the list |
| `views/markdown/model` | what Markdown is read into — blocks and inline marks — and the limits (depth, label, address) that bound it |
| `views/markdown/runs` | a run of inline marks flattened into one string and the stretches that are marked, the link a byte is in, and the cuts a selection splits it into |
| `views/markdown/table` | a pipe table: the cells of a line (an escaped pipe kept), where one begins, and every row padded or cut to the header |
| `views/values` | what a value in an agent's view says as words — a number's thousands, how long ago a time was, a duration in two units, initials and the tint a name keeps, how far along a meter is — the text a cell filters and copies by, and which references are drawn as buttons |
| `extensions` | what the running extensions put in this window |
| `extensions/dialog` | the questions extensions put to this window: the one open, those waiting behind it, the answer or cancellation each sends back, and the page last sent of them |
| `extensions/ask` | what a question an extension puts to this window says and how it is answered: the asker's line with the count waiting, the word on the answering button and whether it is danger, the answer a form builds from its fields, the lit choice of a pick held at both ends, and what a key does in the card |
| `extensions/echo` | an extension's table as the agent table's renderer draws it: a row's status leading as a "Status" column, each cell that only repeats the status blanked and each column left with nothing to say dropped |
| `extensions/keys` | the extensions' commands and keys as the window's: where each key is heard, what the palette lists, and a heading of the shortcuts sheet per extension, as the page draws them |
| `extensions/route` | what a key is to an extension's view that has the keys: the keymap's own step or mode for a view, an action of the view by its key, or a command heard in that view or anywhere, in that order |
| `extensions/row_actions` | which of a view's actions are drawn where: on the row while it is selected or pointed at, in the view's bar, in its menu with the keys beside them and the reason one is refused, and the width under which a row's actions are one button |
| `extensions/views` | which extension views the window holds, which are listed under the sidebar's Extensions where the person is, each one's selected row, and the question an action waits on; what a gesture in one sends; and the page of them all, drawn where the window says each one is, with the commands `extensions/keys` lists beside them, and what the page last heard |
| `agents` | the agents the window can start and the sessions they leave |
| `agents/composer` | what the composer shows and does: which repository, checkout and agent are picked from the rail and the draft, the line saying where a start will be, the call a start sends, and when a start the daemon made ends the message |
| `agents/said` | how the sessions dialog and the transcript reader put a session in words: a row's meta, the Ended group's summary, the filter menu's choices, the start time, the reader's span and match count, and the verb a tool call is told with |
| `agents/sessions` | what the window has asked of a repository's agent sessions — the list, what one said, and how each harness resumes one — and what it holds of the answers, or why the daemon refused one, among them what each pane it resumed a session in was resumed from |
| `agents/sessions_dialog` | what the recent-sessions dialog holds — whose sessions, the search and the harness and checkout filters, the session under the cursor, whether its full transcript is open in place of the list — and what each session offers: its pane, a resume by the exact command named, or only its transcript; the page it draws, last sent while open |
| `agents/templates` | which agents the window offers to start: the daemon's templates, the one last started first while the daemon still offers it |
| `agents/transcript_reader` | the full-transcript reader: the person's turns as a jump list and the one it is on, what find looks for and the match it is on, and the page it draws |
| `checkouts` | a repository's checkouts as the Checkouts stage lists them |
| `checkouts/stage` | what the Checkouts stage holds — whether branch rows show, the filter, the selected row, the question waiting on the person — which of its keys and buttons are the `tree::column::verbs` it sends, and the table the page last heard |
| `checkouts/files` | the files panel beside the Checkouts table as the window holds it — which checkout's folder it reads, the folders open, what each listing said, the selection, mode and filter — what its keys and clicks ask of the daemon and leave to the window, and the listings it takes, dropping a stale one |
| `checkouts/files_page` | what that panel draws from the folders read: the tree's rows in order with each entry's mark, the breadcrumb, and the footer's size and age |
| `checkouts/table` | what the Checkouts stage lists from the chosen repository: its rows, what each says, and the selected checkout's detail, as the table the page draws |
| `checkouts/says` | what a Checkouts row says about itself in words: a checkout's State cell in its fixed order (a clean, in-step, idle one says nothing), a branch row's cell, the branch set beside a name only when the folder does not say it, and a pane count |
| `checkouts/partition` | how the table's rows fall into its blocks: the checkouts, the fold row for the branches without one (present while any exist, even when the filter hides every branch row), then the branches; and the row the selection key names |
| `checkouts/words` | the sentences the stage says around its table: loading, no repository, nothing to list and why, the strip's unfiltered counts, and the key bar with Files left out until the daemon offers them |
| `checkouts/ways` | what the stage's gestures come to: the items of the strip's and a row's menu per row kind, and for a button, a menu choice or a key the calls it sends (picking the row first when it is not the selected one) or the branch name it asks for before a new worktree |
| `checkouts/side` | what the column beside the table says of the selected row: its name, the path cut at the first `/.argus/`, the buttons, the facts, the running panes' heading and the foot |
| `checkouts/fold` | which rows of a file tree fold into the one "N ignored" row at its foot — an ignored top-level entry and what is opened under it — and what that row says, for the Files panel and the Code stage alike |
| `checkouts/marks` | the mark a file tree's row carries and says on hover: `M` for changed, `?` for untracked, and on a Code folder how many changed files are under it |
| `checkouts/query` | what the one field over a checkout's files means by its text: a name, or after a leading `#` a search of the text, which filters no names and is run on Enter |
| `code` | the Code stage and the files it reads in a checkout |
| `code/edit` | the page's editor and the daemon joined: its asks put to the daemon and the answers handed back as they are, past the session, which keeps none of it |
| `code/change` | what an edit to a text is, as a value: a set of changes positioned in the text before it, in the rope's bytes, applied, composed, mapped over another set made on the same text, inverted, and a position carried through it |
| `code/wire` | the edge where the rope's bytes meet the protocol's UTF-16 units: a change set leaving as the daemon counts it and arriving the same way, a position inside a surrogate pair or past the text refused |
| `code/clash` | whether a disk change and the person's touch the same text, and a set split into what clears the other and what does not |
| `code/sync` | the editor's buffer kept in step with the file on disk: what is the person's and unwritten, what is in flight, how a write by anything else is folded in or held aside as a clash until the person says whose stands |
| `code/live` | one file open for editing as the window holds it: its sync, what the bar says of it, what each word of the daemon's asks in answer, and every file the connection took opened again when it says it is back |
| `code/typed` | what the person typed, found from the buffer before and after: the one change between two ropes by what they share at each end, on character boundaries |
| `code/pace` | when what is typed is written, when the file is read again and how long a refused write waits: the editor's timing as numbers and a function of the clock reading |
| `code/steps` | the change marks of the file and how `]c` and Alt-] step between them: the marked lines, and the first line of the next or previous run |
| `code/bar` | what the bar over the editor says of the file and in what tone: one set of words for each status |
| `code/history` | what an editor can take back: each unit of typing as the set that undoes it, and the disk's changes folded past them so an undo never reaches an agent's write |
| `code/vim` | Vim as a state machine over the editor's rope: keys in, the changes they make and the effects the host must do out, with what every editor shares passed in; the library's modes, motions, operators, objects, search, marks, macros and ex commands |
| `code/vim/shared` | what every editor shares and the window holds: the registers, the last `f`, the search, the macros, the options, the mappings and the lines the prompts remember |
| `code/vim/host` | what the host asks of Vim and tells it between keys: the text, cursor, selection and the line being typed to show, and the view, wrap width, comment token and read-only flag to set |
| `code/vim/said` | what the editor's bar says of Vim: the mode named, the keys of a command not finished as Vim writes them, and the line being typed after `:`, `/` or `?`, or what the last key left, or a recording |
| `code/vim/dispatch` | how a key in normal, visual or operator-pending state is read: `langmap` first, then a user mapping, then the command parser |
| `code/vim/key` | what a key is to Vim, and the `<C-r>` notation that writes one down and reads it back |
| `code/vim/page` | a key the page reports as Vim's key: the character with Shift in it, Ctrl chords by their unshifted character, Shift told only for Space and Backspace, and nothing for Alt, the command key, Ctrl-Shift and a letter, or a key Vim is never sent |
| `code/vim/keymap` | which keys name a motion, read from exactly the keys given so a half typed motion is known as one |
| `code/vim/text` | the questions Vim asks of the rope: which character is where, where a line starts and ends, a character's class, a line and column |
| `code/vim/word` | where `w`, `e`, `b` and `ge` go, one step of each, with an empty line a word of its own |
| `code/vim/sentence` | where `(` and `)` go and what `is` and `as` take, by the library's walk over sentences |
| `code/vim/symbol` | where `]` and `[` with a symbol go: an unmatched bracket, a section, a comment's end, a method, a preprocessor line |
| `code/vim/tag` | the element around a position, read from the text as markup, for `it` and `at` |
| `code/vim/screen` | what the window shows as far as Vim needs it: the lines in view, which line `H M L` pick, how far a page goes, and the scroll asked of the window |
| `code/vim/display` | rows of wrapped lines: where `gj`, `gk`, `g0`, `g^` and `g$` go when the window tells Vim how many characters a row holds |
| `code/vim/motion` | where a motion goes from a position and how an operator takes it: charwise exclusive or inclusive, or whole lines; none when it cannot go |
| `code/vim/motion/aim` | what a motion needs of the editor beyond the text: the column `j` and `k` aim for, the last find, the window, the marks and the search |
| `code/vim/motion/find` | the searches of a motion over the text alone: the bracket `%` matches, the character `f` and `t` look for, where a paragraph ends, the first character `^` lands on |
| `code/vim/object` | the span an `iw`, `a"`, `i(`, `ap`, `is` or `it` stands for around a position, a pair looked for ahead when none encloses |
| `code/vim/command` | what the keys typed so far say: a register, a count, an operator and its target, or a command of its own; read purely so half a command is just keys kept |
| `code/vim/command/table` | the commands that are not a motion or an operator with a target: `r`, `m`, `q`, `@`, the `z` family, the search and `:` lines, and those of normal and visual mode alone |
| `code/vim/region` | the text an operator acts on, made from a motion's reach, an object or the selection as one span of the rope, characters or whole lines |
| `code/vim/registers` | what yanks and deletes leave and a paste reads, shared by every editor; the blocks, the last command and search, macros as text, the clipboard offered and written out by the host |
| `code/vim/marks` | the places Vim remembers and keeps with the text: marks, the jump list as the library rings it, each following every edit |
| `code/vim/jump` | what `m`, `Ctrl-o`, `Ctrl-i` and a selection's end do with the marks and the jump list |
| `code/vim/buffer` | the text Vim edits with its undo and its marks: each edit lands at once, is gathered into the open unit and told to the host as one change set per key |
| `code/vim/operate` | what each operator does to a region: delete and yank into the registers, change into insert, indent, the cases and `g?`, wrap, comment and `=`, the cursor left where Vim leaves it |
| `code/vim/wrap` | `gq` and `gw`: lines wrapped to the text width by the library's hard wrap |
| `code/vim/comment` | `gc`: line comments toggled with the token the file's language has |
| `code/vim/normal` | running a command that has been read: a motion moves, an operator is given its target's text, the rest are commands of their own, a change kept for `.` |
| `code/vim/number` | `Ctrl-a` and `Ctrl-x`: the decimal, hex, binary or octal number at or after the cursor changed by the count |
| `code/vim/insert` | insert and replace mode: the keys that type, the few that edit what was typed, `Ctrl-o` for one command, and Escape repeating the text for a count and into a block's rows |
| `code/vim/visual` | what visual mode does with the selection beyond moving it: starting, switching and leaving it, `gv`, `o`, an operator on it and an object taken into it |
| `code/vim/block` | visual block mode: the rectangle between the anchor and the cursor as rows, what an operator does to it, and what is typed after `I`, `A` and `c` going into every row |
| `code/vim/rewrite` | the commands that rewrite text at the cursor without an operator's target: `r`, `~` and `J` |
| `code/vim/paste` | `p`, `P`, `]p` and `[p`: a register put after or before the cursor, as lines or as a block, fitted to the indent, and over a selection |
| `code/vim/seek` | what `/`, `?`, `*`, `n` and `gn` do to the cursor: the line typed run, the word under the cursor made the pattern, the next match taken into a selection or an operator |
| `code/vim/search` | the search every editor shares: the pattern last searched for, which way it runs and what `:s` last put in |
| `code/vim/search/pattern` | what a typed pattern is: made into a regular expression the way the library does, with a smart case and Vim's own syntax translated when `pcre` is off |
| `code/vim/search/scan` | finding matches in the text: the library's search cursor, and `n`'s walk over it that passes the match the cursor is on and wraps round the ends |
| `code/vim/prompt` | the line typed after `:`, `/` or `?`: a text with a caret that keys edit, Up and Down through its history, Enter and Escape |
| `code/vim/recall` | what the `:` and `/` lines remember: each line run and the walk through those that begin with what is typed |
| `code/vim/macros` | the macro state every editor shares: the register being recorded into, the last one played, how deep playing is and how many keys it may still run |
| `code/vim/replay` | keys that stand for other keys: recording into a register, playing one, what a mapping and `:normal` type, and an insert mapping half typed |
| `code/vim/maps` | the mappings `:map` makes: keys that stand for other keys in a mode, found by the keys typed |
| `code/vim/options` | the options `:set` reads and writes, their short names, which are an editor's own, and `langmap` read |
| `code/vim/ex` | the `:` line run: the range and the command name read off the front, the command found in the library's table and handed the rest, the line kept in the `:` register |
| `code/vim/ex/range` | the front of an ex line: the lines it names (`%`, numbers, marks, patterns, offsets) and the command's name |
| `code/vim/ex/table` | the ex commands there are and how a typed name is matched to one by its short name |
| `code/vim/ex/editor` | the ex commands on the editor and not on lines: `:w`, `:startinsert`, a bare line number |
| `code/vim/ex/lines` | the ex commands over lines: `:d`, `:y`, `:pu` and `:j` |
| `code/vim/ex/keys` | `:normal`: keys typed as normal-mode commands on each line of a range |
| `code/vim/ex/substitute` | `:s`: the matches in the lines named replaced, with groups, the flags `g`, `c` and `i` and a count, asking about each match with `c` |
| `code/vim/ex/global` | `:g`, `:v` and `:g!`: a command run on each line with a match or none, the lines held as the commands edit the text |
| `code/vim/ex/sort` | `:sort`: lines put in order, reversed, by number or by a pattern, with repeats dropped |
| `code/vim/ex/set` | `:set`, `:setlocal` and `:setglobal`: an option read, written, flipped or turned off |
| `code/vim/ex/show` | `:registers` and `:marks`: what the registers hold and where the marks are, as the message the editor shows |
| `code/vim/ex/delmarks` | `:delmarks`: marks removed by letter and by range of letters |
| `code/vim/ex/map` | the mapping commands: `:map` and its forms, `:unmap` and `:mapclear` |
| `code/stage` | the Code stage as the window holds it once opened — the checkout it reads, the version, that version's files and changes, the folders open, the file read and its cursor, the comment being written, the go-to-file field, the search, and the review it was opened from — the daemon's answers it takes, what its gestures ask of the daemon and leave to the window, and the page and gestures it trades with the page, last sent while it is up |
| `code/tree` | a version's flat file list as the tree the Code stage draws: the rows that show given the folders open, and which folders and files changed |
| `code/side` | what the Code stage's files column says and sends from its one field: the text the field shows for what Rust holds (a search as `#query`), whether typed text still means the open list, the search's options and its one summary line, the go-to list's empty line, and what a key in the field or the glob field comes to |
| `code/start` | what the Code stage's file area says while no file is read: the changed files cut at ten and how many more, what stands in for none, the files opened lately kept by the window (eight a checkout), the hint, and the search match the cursor is on that the area offers to open |
| `code/file` | what the Code stage's file area says for the file read: which state it is in (failed, none, loading, a note, the lines), the header's name, folder and facts, and the file's rows with the places lines were removed between them, as the diff's rows so one view draws them |
| `code/grammar` | the tree-sitter grammars the Code stage's editor colours with: the ones Argus knows, finding and downloading one, keeping it, and running it |
| `code/grammar/known` | the grammars Argus knows by extension or name: each an npm package's WebAssembly parser and highlight queries, pinned to a version and the SHA-256 and size of every file, found by whole name then by lower-cased extension |
| `code/grammar/net` | how a grammar's bytes come from the network: HTTPS to the registry and the two jsDelivr hosts and no other address, a refusal said as `<url> answered <status>`, and a table of answers for the flows above it to be tested with |
| `code/grammar/fetch` | what grammar to offer for a file and downloading it: the pinned one, or the first npm package found by searching that ships a top-level parser and a highlight query; each pinned file refused if its hash differs, a found one never vouched for |
| `code/grammar/store` | the grammars installed, kept as `<name>.wasm`, `<name>.scm` and a `.json` in the instance's config directory, written whole or not at all and held in memory when the disk will not take them, under one key for a file that the store and the lookup share |
| `code/grammar/load` | a grammar ready to parse with: the parser compiled by tree-sitter's WebAssembly runtime and its query accepted by it, once, and a parser of its own for each user; what is loaded held by name for a first paint |
| `code/grammar/say` | what the editor's bar says of a grammar: the offer with where it comes from and what it costs, and what became of installing it |
| `code/grammar/install` | finding a grammar when a file opens and installing it on request: the installed one colours, a pinned one installs unasked when Settings says so, every other is offered and waits; nothing is kept that did not load |
| `code/grammar/slot` | what the editor's bar holds of a file's grammar: nothing, an offer waiting on a yes, an install under way or the sentence of how one failed, with the transitions, "Not now" remembered by name for the window, nothing offered while a clash holds the bar, and which files are of one kind for a grammar |
| `code/outline` | what the outline column says: the badge of a definition's kind, the sentence for a file that defines nothing the outline knows, the heading over the changes |
| `code/ruler` | where the change ruler puts a mark at its rows, the band of the file on screen, how far a press or a drag scrolls the file, and what a mark is titled, in the measures of whatever drew the rows |
| `code/scroll` | where the file scrolls to when the cursor moves: only when the cursor is off screen, then a third of the way down, and what tells a move from a redraw |
| `code/strip` | what the strip says on the Code stage: the checkout and, when it is another word, its branch, nothing until the stage is open |
| `review` | the review and history dialogs as the window holds them: which is open or parked, the daemon's answers and refusals routed to the one that asked whether open or parked, the layout the next review opens on, and what the page last heard of each |
| `review/comment_box` | the comment box the review and the Code stage share: whether it is open, the agent it is lit on, the comment it sent and waits on the daemon to save or refuse, by its id, and what the page draws of it |
| `review/dialog` | what the review dialog holds beyond its `review::session` — the side and layout chosen, its comment box, its go-to-file field — and the page it draws |
| `review/file_jump` | the go-to-file field the review and the Code stage share: what is typed, which match is lit, and the file Enter opens |
| `review/history` | what the history dialog holds — a checkout's newest commits, the ones unfolded to their files, the filter over them, and each opened commit's diff or why it could not be read — and what its gestures ask of the daemon |
| `board` | the Feature stage and a feature opened from it |
| `board/capture_note` | a new feature being written as one note: what it reads as by `capture`'s grammar, whether the board holds the feature it was sent as yet, and the page and gestures it trades with the page, last sent while open |
| `board/feature_cards` | what a gesture on a feature's card asks of the daemon: a card dropped on a column, and the `H` and `Shift A` keys on the one under the cursor |
| `board/feature_edits` | what an edit to an open feature asks of the daemon: each gesture as the message the terminal client sends for it — a drag on a flow as its source rewritten — checked against the protocol's bounds; a new row saved blank sends nothing |
| `board/feature_stage` | what the Feature stage holds — the repository's board, the feature under the cursor and its tasks and flows, whether it is opened, the lens it is read through and each lens's cursor, each repository's layout, the filter and the fold — and what its messages and gestures change, and where each cursor steps, never onto a row the page does not draw |
| `board/feature_view` | what a feature opened from the index shows: its header — how it stands, its branch, agent, brief, progress, tickets, when it was last reconciled and how much changed since — and the lens it is read through, and the rows each lens's cursor steps over |
| `board/features` | what the Feature stage's index shows from a board: which features it lists and in what order — the filter, the fold (held and archived on the list, archived on the board), the board's columns, the opened feature kept — its rows and a card's footer, the places a person may move each, and the peek with the features it may come after |
| `board/index_words` | what the Feature index says in words for a window that draws it natively: a row's cells and note, a card's corner and work line, an empty column's lines, the strip's counts, the key bar, and what a card dropped on a column asks or is told |
| `board/lenses` | the lenses an opened feature is read through |
| `board/lenses/flows` | the Flows lens: a feature's flows, the one under the cursor as steps, and the step under the cursor with what binds it and what carries it out |
| `board/lenses/head` | the words an open feature's header says about its tickets and its reconcile, built from the counts the board holds and not unpicked from a sentence |
| `board/lenses/picture` | where each part of a flow's sequence diagram sits and how it is scaled and picked: the layout in the diagram's own units, the inline fit, the expanded fit and zoom, the nearest step or lifeline to a drag, and what scrolls into view |
| `board/lenses/record` | the Record lens: every decision in the order it was made, and a decision read as its record — what replaced it, what was turned down, and the ADR it copies as |
| `board/lenses/sequence` | a flow's source as the sequence diagram the page draws: its lifelines in order, each step as an arrow between two of them, and the frames a branch draws |
| `board/lenses/work` | the Work lens: tasks and decisions as `outline` reads them, with each task's tracker key and a mark on each row changed since the last reconcile, and the task or decision under the cursor in full |
| `board/tracker` | the dialogs that tie a feature to its tracker's tickets |
| `board/tracker/dialogs` | the link and reconcile dialogs as the window holds them: which of the two is open and on which feature, closing it when the board loses that feature, a refusal of the link the link dialog sent, and what the page last heard of each |
| `board/tracker/link_dialog` | what the link dialog holds — the URL and role being typed and what the URL reads as, the linked ticket under the cursor, the tracker map being edited, a link sent and not yet on the board — what its gestures ask of the daemon, and the page and gestures it trades with the page |
| `board/tracker/reconcile_dialog` | what the reconcile dialog holds — the line under the cursor, the lines ticked and the words each was rewritten in, the agent an ask goes to — what its gestures ask of the daemon, and the page and gestures it trades with the page |
| `board/tracker/tickets` | which view an agent showed of a linked ticket, and the words and call a press on the ticket's chip carries: the match is by the ticket's URL or by its tracker and key, the newest view winning |
| `fixtures` | the tree builders the test modules share, and the commits and diffs the daemon answers about it |
| `screens` | the scenes the screenshot harness draws: a session brought to each screen the window shows, and the events a fresh page hears there, with a stand-in daemon answering what each scene asks |
| `screens/extension_kinds` | the scenes of an extension's prompt, pick and form asks and of a view that is a console |
| `screens/live` | a scene served live to a page in a browser: its calls reach a real session, the stand-in daemon answers them, and what the page hears goes back |
| `screens/saved` | the scenes of a saved view followed into columns, stale, and a column whose view is gone |

`argus-desktop` is the Tauri desktop window. It holds what it was sent and draws it; everything it knows
of a pane, a checkout or the daemon came from the protocol, and the page only draws and reports. What is
here is only what needs Tauri or the machine; the rest is `argus-desktop-core`.

| module | answers |
| --- | --- |
| `main` | startup: the runtime, the window, and running until it closes |
| `connection` | how the Tauri window hands the page's calls to the session and the session's events to the page |
| `connection/commands` | the two commands the page invokes — `connect` for its channel, and `call` for every `connection::page::Call` — and which part of the window answers each call |
| `connection/shared` | what the page's calls, the daemon's messages and the frame tick share, handing each change to `connection::effects` with the Tauri window's outlets |
| `system` | what the Tauri window asks of the machine it runs on |
| `system/clipboard` | the desktop clipboard's text: what a dragged selection copies to, and what the paste key reads, with a picture or copied files; the Tauri window's `effects::Clipboard` |
| `system/toast` | handing an alert to the system's notifications, best effort |
| `system/window_frame` | the window without the system's title bar, and the calls the page's strip makes to move, size and close it |
| `settings` | what the Tauri window draws behind itself and which fonts it finds installed; the choices themselves are `argus-desktop-core`'s |
| `settings/vibrancy` | putting the chosen backdrop on the live window, taking off whichever was there |
| `settings/installed_fonts` | which font families this machine has installed, read from its font files by every name the webview might know each by, and which are monospaced |

`argus-gpui` is the port of the desktop window onto GPUI through `gpui-kit`. It began as a spike
that proved each piece in a frame of its own (its examples still build) and grows into the window:
the sidebar and the Workspace's live pane over `argus-desktop-core`'s session so far (`GPUI.md`).

| module | answers |
| --- | --- |
| `main` | startup of the GPUI window: the runtime built before the app, the daemon loop and the frame tick on it, ended when the last window closes, and the window; the unattended runs are `probe` and `report` |
| `board` | the Feature stage as this window draws it: the index of a repository's features as a list beside a peek or as a board, a feature opened from it and the lenses it is read through, and a new feature written as one note |
| `board/cells` | the small pieces a row, a card and the peek share: the tasks cell, the questions cell, the agent's pill and the numbers a feature comes after, worded by core's `index_words` |
| `board/columns` | the board layout: Queued, Active and Done as lanes and Held over Archived beside them, each column of cards, the lanes' own scroll kept across pushes |
| `board/columns/card` | one card: number, branch and corner tag, title, work line, the reason it is held and how it came to stand there; also lifted as the ghost and as the placeholder left behind |
| `board/columns/cue` | what says a column is cut off to the right: the fade, the chip naming the columns, how far the chip scrolls, and the redraw when what was measured is not what was drawn |
| `board/columns/drag` | a card dragged to a column by the kit's one drag helper: Escape cancels, only a column that takes the card lights up, and a drop is judged by core's verdict |
| `board/columns/lane` | one column: its head and count, its cards or the two lines it says when empty, the folded Archived row that still takes a drop, and the card's press, click and menu wiring |
| `board/ctx` | what every part of the index sends through and reads from: the window's model and the way out, nothing sent while the window is held |
| `board/feature_stage` | the Feature stage's index as the stage: the no-repository card, the loading bars, the empty repository, then head, list or board and key bar; its focus, filter field and width |
| `board/feature_stage/bar` | the keys under the index, a cap and what it does, and on the board how a card is set aside |
| `board/feature_stage/empty` | what fills the index before there are features: the no-repository card, six loading bars, and the card for a repository with none |
| `board/feature_stage/head` | the index's head: the filter field, folded into the strip's button in a narrow window, and the list's fold toggle |
| `board/feature_stage/keys` | the Feature layer of the key walk: the index and card keys sent as the action they are, the filter field taken by `/`, the menu's keys while one is open, and every other key held |
| `board/feature_stage/strip` | what the stage puts in the strip: the counts, the List and Board radio, the narrow window's filter button and New feature |
| `board/list` | the list layout: rows under group labels beside the peek, in Rust's order, the list's scroll kept across pushes |
| `board/list/row` | one row: number, title, tasks, questions, checkout and the note under them; a click peeks, a double click opens, a right click opens the Move menu |
| `board/menus` | the Move menu and the Comes-after menu, opened at the pointer, under the peek's button or beside the row, closed by a snapshot that takes the feature away |
| `board/menus/card` | the menu as it is drawn: labels, rules and choices, the highlighted one lit |
| `board/menus/hold` | the dialog that asks why a feature is held, with the field holding the keys |
| `board/menus/items` | what each menu lists as plain data, each choice with what it does, which can be chosen, and where the highlight steps, wrapping |
| `board/menus/run` | running one of a row's moves that a person sets, wherever it was chosen: a hold asks why, the rest are calls |
| `board/peek` | the peek panel beside the list: the feature under the cursor in a few lines, with Open, Go to its pane and the menu button |
| `board/peek/lines` | a section of the peek: a label over lines of what needs you, what is in flight, what is next and what was last decided |
| `board/capture` | a new feature written as one note: the field held for as long as the note is open, Create sent with the text as the field has it, the Writing keys answered before it types, and what the daemon's own grammar reads the note as |
| `board/capture/gutter` | the line numbers beside the note: one for each line, as tall as the rows the line takes once wrapped (asked of the textarea's display map), scrolled with the text, and a line the grammar could not place in danger |
| `board/capture/head` | the note's head: the repository and the way back as a crumb, the title, and Discard beside Create, which waits while the note is blocked or being made |
| `board/capture/keys` | the Capture layer of the key walk: the window's own keys while a note is open, every other held, and Escape discarding the note |
| `board/capture/legend` | the grammar as the foot of the note says it: what a first line, a paragraph and the three marks mean, in two aligned columns |
| `board/capture/reads` | what the note reads as: the verdict, the title and brief, the tasks and decisions as `CapturePage` reads them, and the line it could not place named |
| `board/feature_view` | a feature opened from the index: its header, the lens read and the key bar, with the form being written, the legend, the option chosen, the expanded diagram and every list's scroll held across the pushes that redraw it |
| `board/feature_view/act` | what each key of an open feature comes to as a value, in the Work, Record and Flows lenses and the expanded diagram: the page's `workAction`, `recordAction`, `flowsAction`, `decisionAction` and `diagramAction` without a window |
| `board/feature_view/columns` | how wide an open feature's outline is: the page's `flex: 1.25` share of the split until the edge is dragged, then the width Rust keeps, then the pointer's while the sash is held |
| `board/feature_view/diagram` | the two diagrams as the view sets them: what each is told to show, scaled to its box or fitted to the canvas, and the scroll that brings the lit arrow into view when the cursor has moved |
| `board/feature_view/expanded` | what the expanded diagram holds of its own: zoom, whether it is fitted and whether the steps panel shows, and the rules that turn one into another |
| `board/feature_view/key_bar` | the key bar under an open feature: the keys of the lens read and Esc back to features |
| `board/feature_view/keys` | the Outline and Diagram layers of the key walk: an open feature's own place before the window's, and every other key held |
| `board/feature_view/menus` | the menus an open feature opens at a point: the bind menu under the lit row or arrow or the pointer, and the flow switcher on the expanded toolbar |
| `board/feature_view/more` | the `...` that holds what is rarer than the buttons beside it, its choices being the same values as keys |
| `board/feature_view/scroll` | each list's scroll handle and the cursor's identity, which says when the cursor's row is brought into view: only when the cursor moved, not on every push |
| `board/install` | where the board's surfaces are drawn: the note and an open feature as the Feature stage's page, and the strip's title, trail and tools for an open feature |
| `board/lenses` | the lenses an opened feature is read through, and the header and forms they share |
| `board/lenses/decision` | a decision read as its record in either lens: its sections, its consequences as gains and costs, its foot of actions, and the record, answer and supersede forms that replace the body |
| `board/lenses/decision/options` | an open decision's options as cards to choose between: the choice is the window's until Take sends it |
| `board/lenses/flows` | the Flows lens: the flows, the selected one's steps and the step under the cursor on the left, the diagram on the right |
| `board/lenses/flows/bind` | the menu that binds a step to a decision: one choice for each decision with the id a binding writes, the bound ones held off |
| `board/lenses/flows/picture` | the picture beside the flows: the title, Expand, and the diagram in a box that scrolls sideways at the scale that fits it |
| `board/lenses/flows/steps` | a flow's steps and the step read whole: a row for each step and branch, what binds and carries the step out, and the step's and the flow's forms |
| `board/lenses/forms` | the one form an open feature has open: which it is about and the two fields it is typed in, held across every push, saved as one edit and dropped when its row has gone |
| `board/lenses/forms/edit` | what a form is about, what it starts out saying, which edit it sends and whether what it is about is still on screen |
| `board/lenses/forms/view` | what a form is made of on screen: a named field, the multi-line field, the primary button with its cap beside Cancel |
| `board/lenses/head` | an open feature's header: how it stands, how far it is, its tickets and brief, or the brief's form in their place |
| `board/lenses/head/menu` | the feature menu: the brief, the tickets, the reconcile with what it waits on, and the views that name the feature |
| `board/lenses/head/tools` | what the strip carries for an open feature: the lenses as a radio with a count each, and the menu with a dot while a proposal waits |
| `board/lenses/record` | the Record lens: every decision in the order made as the tree it was made in, beside the one under the cursor |
| `board/lenses/sequence` | a flow's sequence diagram drawn and picked: the click that selects a step and, expanded, the drags that move an arrow or a lifeline |
| `board/lenses/sequence/expanded` | the diagram over the whole window: toolbar, canvas and the steps beside it |
| `board/lenses/sequence/paint` | the diagram painted from core's layout: frames, lifelines, arrows with their dots, the lit step in the accent and the drop guide |
| `board/lenses/sequence/panel` | the steps beside the expanded diagram: the list, the step in full with its participants, and the forms |
| `board/lenses/sequence/toolbar` | the expanded diagram's toolbar: crumbs, flow switcher, legend, zoom, Fit, the steps panel and the way out |
| `board/lenses/work` | the Work lens: the outline of tasks and decisions, the row under the cursor in full, and the sash between them |
| `board/lenses/work/legend` | the outline's legend: the `?` that asks for it and the popover that hangs under the header |
| `board/lenses/work/marks` | what a row says in marks and words: the glyph and tone of its state, what stands at its end, how its text is set and when a changed dot is worth drawing |
| `board/lenses/work/outline` | the outline's column: its label and legend, the rows, the form that adds one and the buttons that open the root forms |
| `board/lenses/work/row` | one row of the outline: mark, label, words, ticket key, changed dot and what stands at its end |
| `board/lenses/work/task` | the task under the cursor in full: description, criteria as checkboxes, what binds it, its notes and its foot, or its form |
| `board/snapshots` | the snapshots a test reads: what the session sends in the scenes the Tauri page is photographed in, and a window that has heard one |
| `board/tracker` | the dialogs that tie a feature to its tracker's tickets, and the chip each ticket is |
| `board/tracker/install` | what the window does once for the tracker dialogs: makes both, which open themselves while the session holds them, and puts their layers in the key walk |
| `board/tracker/keys` | a tracker dialog's layer of the key walk: `?` opens the sheet, the stage switches and the window's commands are held and ignored, every other key it binds goes to the dialog's sink |
| `board/tracker/link` | the link dialog: up while the session holds it, its two fields and the map's box kept as the person types, each keystroke sent with both fields' text, the fields emptied once the board holds the link |
| `board/tracker/link/body` | what the link dialog draws: the head, the URL and role fields with the reading beneath the URL, and the foot with Cancel and Link |
| `board/tracker/link/map` | the tracker map in the link dialog: its summary with the draft pill and Edit, or the editor with its ways out |
| `board/tracker/link/rows` | the tickets already linked, as the link dialog lists them, with Open and Unlink on each and the cursor row lit |
| `board/tracker/reconcile` | the reconcile dialog: up while the session holds it, the ask and the words of the line open kept as the person types, the lines list's scroll kept across a push and the cursor line brought into view |
| `board/tracker/reconcile/ask` | the ask a reconcile starts with: what to reconcile, the agent to ask, and Send the ask |
| `board/tracker/reconcile/body` | the reconcile dialog's frame and its body by stage: the ask, the waiting line over it, or the lines beside the line open |
| `board/tracker/reconcile/detail` | the line under the cursor in full: its words word for word, the call the agent will make, and what happens afterwards |
| `board/tracker/reconcile/head` | the reconcile dialog's head and foot: what was asked, who proposed it, the count ticked and the ways out |
| `board/tracker/reconcile/lines` | the proposed lines under their headings with their ticks, the `failed` pill and what was noticed, flat in one scroll so the cursor line has a child index |
| `board/tracker/tickets` | a ticket as a chip: the tracker and key, the dot when an agent showed a view of it, and a press that opens that view or else the ticket |
| `agents` | the agents the window starts and the sessions they leave: the composer, the recent-sessions dialog and the transcript reader in it |
| `agents/ask` | how the agents' dialogs and the composer send a call: dropped while the daemon is gone |
| `agents/compose` | the composer, the stage of New: the message with the keys as it comes up, the three pickers, Shell and Start agent, and the draft held until a pane the start made is shown |
| `agents/compose/picker` | one picker of the composer: the kit's select written to only when its list or its pick changed, so an open list stays open |
| `agents/compose/view` | how the composer is drawn: the place line between two rules, the question, the card, and the note and keys under it |
| `agents/install` | what the window does once for the agents: the composer put where the stage draws it, the sessions dialog's two layers in the key walk, and the dialog brought up while the session holds it |
| `agents/offer` | what the session under the cursor offers, drawn for the sessions footer and the reader's head: its pane, a resume by the command named, or only its transcript |
| `agents/search` | the search field in the head of the sessions dialog and the reader: a plain field of the page's width |
| `agents/sessions` | the recent-sessions dialog: up while the session holds it, never dismissed by itself, with its search text, scrolls and where the keys rest kept by the window |
| `agents/sessions/filter` | the Filter button and the menu under it: core's choices, each sending the one filter changed and the other kept |
| `agents/sessions/head` | the dialog's head: its title and what it lists, the search, the Filter button and the close button |
| `agents/sessions/keys` | the dialog's two layers of the key walk, the list's and the reader's: the keymap's actions sent to the session, `/` to a field, and every key passed on while a menu is open |
| `agents/sessions/list` | the list column: the Live heading and rows, the Ended group held under one folding row, and the loading and empty words |
| `agents/sessions/preview` | the session under the cursor: its header, its transcript or the notice in its place, and the footer's keys beside what it offers |
| `agents/transcript` | the full-transcript reader in place of the list: its scroll (the end when newly read, a move's entry at the top, else where it was), the find field and the jump list |
| `agents/transcript/entry` | one line of a session, drawn alike in the preview and the reader: a turn or reply with its caption, or a tool call as a verb, its summary and whether it failed; a kind not known draws nothing |
| `agents/transcript/head` | the reader's head, with its way back, title, meta line, find, count, copy and offer, and its footer of keys |
| `agents/transcript/turns` | the reader's jump list: a row for each turn of the person's, the one it is on lit, and the note of the file it was read from |
| `browser` | the browsers agents run as this window draws them: their pictures decoded and held once, a picture in a place, the Browsers stage of tiles, a browser's cell in the grid and the words with none open |
| `browser/chip` | a browser's status drawn: the pill on a tile and the dot on a nav sub-row, coloured by core's class (agent accent, person ok, held attention), no chip when nobody has it |
| `browser/source` | where a pane's picture bytes come from: core's `Browsers::picture` (base64 undone once, shared), the session's in the running window and a `Browsers` of its own for a model with no session |
| `browser/decode` | how a browser's picture becomes an image the window draws: JPEG or PNG sniffed from core's bytes, the pixels decoded to BGRA premultiplied, with no window in it |
| `browser/decoding` | where a picture is decoded: off the UI thread, ticketed when it starts and landed in `Heard`'s pictures only if it is still its pane's newest, a failure dropped and logged |
| `browser/empty` | the Browsers stage with no browser open: `No agent has a browser open.` and the hint with `argus-hook browser` in the code face, the same for an old daemon |
| `browser/pane` | a browser pane's cell in the grid, made once for a browser and kept: its strip, toolbar and screen from what `Heard` says of the browser, and a press anywhere in it giving it the keys with core's call |
| `browser/pane/address` | the address field: the browser's url unless the person is typing in it, Enter navigating to what core makes of the text, Escape putting the url back, both giving the keys to the page |
| `browser/pane/bar` | the cell's toolbar: Back, Forward, Reload, the address and Close, each sent and none predicted, and the text button every control of the cell is drawn with |
| `browser/pane/chip` | who has a browser as a chip: core's word with a dot in the colour of whoever has it |
| `browser/pane/keys` | the keys of a cell's page: core's key input to the browser, any other key to the session as a key, a Mac's command chords left to the window |
| `browser/pane/screen` | the cell's screen: the picture letterboxed or the words for no tab with a New tab button, a press clicking in the page's pixels, the wheel scrolling the page, the keyed ring |
| `browser/pane/tabs` | the cell's tab strip: a tab for each of the browser's, picked, closed by its mark or a middle click, a plus, the status chip, Follow agent and Take control or Hand back, its scroll kept across updates |
| `browser/pane/viewport` | the room a cell tells the daemon: the screen's box as laid out and the display's density handed to core, and core asked again when it says a held size may go |
| `browser/picture` | a browser's picture drawn in one place, kept in its page's shape, repainting only for its own pane's `Pictured` and handing the atlas the images that were replaced |
| `browser/pictures` | the latest decoded picture of each browser, one shared image for every place that draws it: the newest decode wins, a browser with no tab or that closed has none, replaced images retired for the atlas |
| `browser/stage` | the Browsers stage: a tile for each browser in Rust's order in as many columns of at least 360 px as the stage holds, measured as laid out, or the empty words; one picture view kept per browser |
| `browser/tile` | one browser's tile: the agent, the page's title and address, and its picture at 16 by 10; a press asks the session to show that browser's pane and nothing else |
| `checkouts` | what the Checkouts stage shows and does: its table, its side column, its strip, its menus and its dialogs |
| `checkouts/act` | what a gesture on the stage does in the window: the calls it comes to are sent (a held window sends none), and a new worktree that wants a name opens the name dialog first; a way on a row is asked of the table as it is when the gesture lands |
| `checkouts/bar` | the keys the table answers to, quietly under it, as core's key bar lists them |
| `checkouts/dialogs` | the dialogs the stage opens: the name of a new worktree and the question before a removal |
| `checkouts/dialogs/name` | naming the branch of a new worktree: one mono field with `Off <branch>` in the foot, held by the window, Create or Enter sending the text as it stands and every other way out sending nothing |
| `checkouts/dialogs/question` | the question before a worktree is removed or a branch forced out, up while the table holds one: Keep it with the keys, one answer sent once, no second dialog until the table has cleared the first, and centred as the page's confirm dialogs are |
| `checkouts/empty` | what fills the stage before there is a table: five loading bars, the no-repository card, and the line saying why a repository lists nothing |
| `checkouts/files` | the Files panel beside the table: one checkout's folder as core lists it, drawn from the page it is sent, taking the keys when it opens on another checkout and keeping them across the tables its own keys bring back, the filter field and the ignored fold the window's memory || `checkouts/files/body` | what fills the panel between its crumb and foot: why a folder could not be read, loading bars, the reason there is nothing to list, else the tree with its rows, the two buttons on the selected file, and a click, a second click and a right-click each sent as core's gesture || `checkouts/files/crumb` | the folder the selection is in, core's crumb, and the All or Changed choice with how many paths changed || `checkouts/files/foot` | the selected entry's name, size and age, and Open in Code for a file; a line asking for a file when nothing is selected || `checkouts/files/head` | the panel's kicker and checkout, and its buttons: the file manager (only for a local daemon), a shell, read again and close || `checkouts/files/keys` | the Files layer of the key walk: up while the tree has the keys, hearing the panel's own place, sending its keys as the session's, `/` taking the filter and Escape closing through the window's ladder; any other key goes on to the table || `checkouts/files/menu` | what a right-click on a row offers and what a choice sends: open in Code or the editor, the file manager on the row's path, both copies spelled by the daemon's host, and a shell || `checkouts/files/tools` | the filter field: its edits sent as the filter except a `#`, Enter on a search sending it, Enter or Escape handing the keys back to the tree |
| `checkouts/files/body` | what fills the panel between its crumb and foot: why a folder could not be read, loading bars, the reason there is nothing to list, else the tree with its rows, the two buttons on the selected file, and a click, a second click and a right-click each sent as core's gesture |
| `checkouts/files/crumb` | the folder the selection is in, core's crumb, and the All or Changed choice with how many paths changed |
| `checkouts/files/foot` | the selected entry's name, size and age, and Open in Code for a file; a line asking for a file when nothing is selected |
| `checkouts/files/head` | the panel's kicker and checkout, and its buttons: the file manager (only for a local daemon), a shell, read again and close |
| `checkouts/files/keys` | the Files layer of the key walk: up while the tree has the keys, hearing the panel's own place, sending its keys as the session's, `/` taking the filter and Escape closing through the window's ladder; any other key goes on to the table |
| `checkouts/files/menu` | what a right-click on a row offers and what a choice sends: open in Code or the editor, the file manager on the row's path, both copies spelled by the daemon's host, and a shell |
| `checkouts/files/tools` | the filter field: its edits sent as the filter except a `#`, Enter on a search sending it, Enter or Escape handing the keys back to the tree |
| `checkouts/menus` | the strip's `⋯` menu and a row's, drawn from core's lists by the kit's popup and acted on when the menu has gone |
| `checkouts/side` | the column beside the table about the selected row: its name and path, buttons, facts, the panes running there and the foot, giving way to the Files panel and to a narrow window |
| `checkouts/side/facts` | the facts list of the side column: a faint label and what Rust says, the feature's title flowing on after its number |
| `checkouts/side/foot` | the foot of the side column: Files, the folder, New pane with its start menu opening upward, and the removal in danger |
| `checkouts/side/running` | the panes running on the selected checkout, a button each that goes to the pane |
| `checkouts/stage` | the Checkouts stage: the loading bars, the no-repository card, the table with its key bar and the side column; its strip offer, its question, taking the keys as it appears |
| `checkouts/stage/keys` | the Checkouts layer of the key walk: the table's keys through core's `ways::key`, the window's own, the extension commands of the `anywhere` and `checkout` scopes, and every other key held |
| `checkouts/strip` | what the stage puts in the strip: the counts beside the title, the filter field sending its text on every edit and never overwritten while typed in, New worktree and the `⋯` |
| `checkouts/table` | the table as one flat virtual list (the kit's table has no spanning row): its head, the checkouts, the fold row and the branches, following the selection into view |
| `checkouts/table/actions` | a row's trailing buttons, Open or Check out and the `⋯`, laid out always and seen while the row is pointed at or selected |
| `checkouts/table/columns` | how wide the three content columns are, estimated from their text since the page's automatic layout gives each column what its content needs |
| `checkouts/table/fold` | the row standing for the branches without a checkout, a press on it being the `B` key |
| `checkouts/table/row` | one row: the name with its tag and branch beside it, the State, what was last committed, the buttons, the selected mark, a click picking it and a second opening it |
| `checkouts/table/state` | the State cell: what a checkout says in core's order with a status dot per running pane, or a branch's git string, note and feature |
| `checkouts/tag` | the small `primary` pill beside a checkout's name |
| `chrome` | the window's frame around the stage: the sidebar and its foot, and the header over the stage |
| `chrome/foot` | the sidebar's foot: the tallies that need a look, a daemon's alarm, and the connection in one shape in every state |
| `chrome/ended` | what a pane whose program has ended offers over the foot of the stage: its note, a button that starts another where it was (the first template for an agent, a shell otherwise) while the pane is in the tree, and one that closes it |
| `chrome/header` | the strip across the top of the stage: the crumb over the title with the pane's status chip (where the rail is folded) and resumed note, the pane's size or parked place, the drag region that moves the window, the stage's tools and the Views launcher, and its three controls, dimmed while the daemon is gone |
| `chrome/header/crumb` | the small line over the title: the repository, then the branch or the stage's trail, between faint slashes |
| `chrome/header/launcher` | the Views launcher at the strip's end: its count of the views agents showed, gone with none, lit while the panel is out, toggling it |
| `chrome/header/meta` | what a stage says beside its title, muted and small: the Checkouts stage's counts, its branch in mono in the reference colour, with a tooltip |
| `chrome/header/more` | the pane's `...` button at the strip's end and the menu under it: its name, Rename, and Close in the danger colour |
| `chrome/header/slots` | what a stage's own view hands the strip, once, when it is made: the words it says (title and trail) and the tools it puts at the strip's end, so adding a stage adds no arm to the strip |
| `chrome/nav` | the sidebar's way between stages: which items there are, which one the stage on screen lights, and the call each press makes |
| `chrome/notice` | the line along the bottom of the stage: the newest notice with an info mark, its words and a dismiss, ended after six seconds counted from when it arrived, and replaced by the next |
| `chrome/projects` | the dialogs the project switcher opens |
| `chrome/projects/add` | the add-a-project dialog: up while the session holds it, drawing its page and sending its gestures, keeping what is typed over what was sent and sending typing after a pause |
| `chrome/projects/add/browse` | the top of the add dialog: the kind of add, the folder's path and the folders in it |
| `chrome/projects/add/outcome` | what adding would do per kind, and the name and skip fields |
| `chrome/projects/question` | the question before a project is removed: an alert up while the rail holds it, `Keep it` having the keys, answering through the page's call |
| `chrome/rail` | the sidebar: what it is made of (the brand row, the nav, the list of repositories under it and the foot) as wide as the person made it and folded to icons when narrow, over a slice of the latest `Rail`; the tree dimmed while the daemon is gone, and neither calling nor picking on a press |
| `chrome/rail/brand` | the brand row: the logo and the project the rail lists, the title bar's left end that moves the window, with the name a button opening the list of projects when there are several; the logo alone in a narrow rail |
| `chrome/rail/calls` | what the rail asks of the daemon: a call unless the sidebar is held, the close of Git that goes before a pane or another stage comes up, a pane shown as a click shows it, and what a choice of a menu does |
| `chrome/rail/checkout` | a checkout's row: the fold, branch, shared (the people mark) and git marks, the dot and count while folded, the + that starts something, the pick's ring, its right-click menu, and the panes hanging from it while open; only the panes in a narrow rail |
| `chrome/rail/keys` | the rail's layers of the key walk: the picked row's, up while a row is picked and the rail has the keys, and the + menu's, up while the session says it is open; each sends the action the table binds to the session as it is (`RailKey`, `StartKey`) and holds every other key, and a press outside, a pane newly shown or a stage change hands the keys to the stage |
| `chrome/rail/listing` | the list under the nav: the project's repositories, or the quiet wait and the empty line when there is no project, dimmed while the daemon is gone |
| `chrome/rail/menus` | what the rail's right-click menus offer: a repository's, a checkout's and a pane's as menu items naming the calls they send |
| `chrome/rail/switcher` | the brand's menu, the project switcher: core's entries (projects with their tallies, add, workspaces, hosts, remove) as menu items naming the calls they send, the two lists that unfold in the page opening beside it |
| `chrome/rail/popup` | a rail menu drawn: its items as the kit's popup, each choice handed back to the rail to do once the menu has let go of the keys, and off while the daemon is gone |
| `chrome/rail/pane` | a pane's row (a browser's wearing a globe): its status as a dot or a word, its title, the close that appears under the pointer, its right-click menu, a middle click closing it, and the press that carries it; a tile of the task's initials with the status on its corner in a narrow rail |
| `chrome/rail/repository` | a repository's row: its chevron, name, status dot and count, the clock that opens its recent sessions, its right-click menu, and the checkouts under it while it is open; a hairline in a narrow rail |
| `chrome/rail/row` | the box every selectable line of the rail is made of, what a drawing of the rail is made against, and the quiet button a row shows under the pointer |
| `chrome/rail/scroll` | bringing the pane on screen into view once per pane, on the frame after its row says where it is |
| `chrome/rail/slice` | the part of what was heard that the rail draws, kept to compare: the rail redraws when it changes and not for every event |
| `chrome/rail/start` | a checkout's + menu drawn open in its row, the highlight the session's, a press outside shutting it |
| `chrome/rail/stages` | the nav rows: which stage each lights, the keys named beside them, the press that goes there, and the icon tiles of a narrow rail |
| `chrome/rail/status` | how a status looks on the rail's rows: the dot a status earns, and the words a few statuses say (the pill they are said in is `kit/pill`) |
| `chrome/rail/browsers` | the Browsers item's own parts in the nav: the count, the chevron that folds the list, and a sub-row for each browser in the stage's order, lit while its pane is in the grid, a press that shows it or drags it with no checkout |
| `chrome/rail/tail` | the nav's dynamic tail: the Extensions and Views folds, each drawing nothing while it holds nothing, their rows lit while on screen and handles that dock the view |
| `chrome/shell` | which regions the window is made of and how they sit: the sidebar at full height, and beside it the work area under the stage's header |
| `chrome/stage` | what fills the stage under the header by the one choice `route` makes: the Git stage, the Checkouts, Code and Browsers stages, the Workspace's live pane with the bar of an ended pane over its foot, the empty stage in its place with no pane, or a line saying a stage is not in the window yet; dimmed while the daemon is gone |
| `chrome/stage/empty` | the card on the stage when no pane is on screen: connecting (a spinner, where, the last failure, one way out), no projects (add one), or no pane (an agent, a shell on the first checkout, the panes to go back to), from core's `chrome/empty` |
| `chrome/stage/route` | which surface fills the stage (Git, the console, the pane, the empty stage, the Checkouts, Code or Browsers stage, or a stage not built yet) read from the session's stage, the window's Git and the console's place together, which parts of the frame are put away around it (the sidebar while the pane fills the window; the nav, foot and docks under a fullscreen console), and the 1200 px breakpoint |
| `chrome/stage/running` | the `Panes in <project>` strip under the empty stage's buttons: a card for each pane of the project, the status, the title, the note and how long since it changed, each selecting its pane |
| `chrome/stage/stopped` | the card over the window while the daemon was stopped on purpose, on the overlay host where Escape and the scrim do not dismiss it: up exactly while the link is stopped, and its Reconnect asks the session to reconnect |
| `chrome/window_controls` | the window's three controls: what each looks like, the red of the close, and what it asks of the window |
| `code` | what the Code stage shows and does: its frame of three columns, the files column, the start panel and the file read |
| `code/file` | the file read in the middle column: its header, the lines drawn read-only by the shared diff view with the places lines were removed between them, the change ruler beside them and the comment box under the line it is on; each file scrolled where it was left and only a moved cursor brought into view; while the stage edits the file the editor takes the lines place, with no ruler, and the comment box is an in-flow block above it |
| `code/file/header` | the file's header: the name first and the folder after it, the version when it is not the working tree, the facts and counts, and the buttons Back, Open in editor and Outline, each sending the gesture its key sends |
| `code/file/ruler` | the change ruler, the file's scrollbar: a button for each change or hit at its rows' place, the band of the file on screen, and the press and the drag after the decided 5 px that scroll the file, placed from the list's own measures every frame |
| `code/file/scroll` | where the file is scrolled: each file's offset kept by path and put back when its lines return, and a moved cursor brought into view by core's rule once the list knows how tall it is |
| `code/file/states` | what stands where the lines would while there are none: why the read failed, the note on a binary or oversized file, and the five loading rows |
| `code/file/editor` | the editor that stands in for the file's lines while the stage edits it: a session for each file opened, the kit's patched editor for the one on screen, the bar over it, and the previous file's editor kept up until the next file's text arrives so no frame is blank, Vim's keys, the grammar's colours and the clash drawn in the text |
| `code/file/editor/session` | one file open in the editor: the kit's editor state once the daemon has sent the text, the rope it last held, the burst of typing, the timers, the marks it draws, Vim's state and what it draws, the clash marks and the grammar slot |
| `code/file/editor/receive` | what the editor does with the daemon's words: the first text mounts the view, a change from the disk is shown without moving the cursor or entering undo, and what the sync asks of the daemon is sent |
| `code/file/editor/typing` | what the person types and when it is written: the change found against the text last seen and told to the sync, the write after a pause, and a file no longer on screen written and let go |
| `code/file/editor/marks` | what is drawn over the text: the change marks in the gutter, kept with the text as it is typed, and an agent's write lit for a moment |
| `code/file/editor/moves` | where the cursor goes besides where a key puts it: to Rust's line, to the next or previous change, and to the lines a selection covers for a comment |
| `code/file/editor/keys` | which chords are the editor's own while it has the keys (write now, go to a file, search, comment, step between changes) and the key hook that takes them from the kit's editor, after Vim, which hears every key first while it is on |
| `code/file/editor/bar` | the bar over the editor: the file's status in its colour; at its right the grammar on offer with Install and Not now, or while a clash is held the two buttons that say whose text stands; and Vim's mode, the keys of a command begun and the line being typed |
| `code/file/editor/clash` | a held clash drawn: the person's clashing text marked in the danger colour with a wavy underline wherever typing has carried it, and a strip under the bar of what the agent wrote at each, since the kit's editor has no inline widget |
| `code/file/editor/grammar` | the colours of the file in the editor: the installed grammar colouring the first paint, otherwise one found, offered or installed off the draw, every open editor of the kind coloured when one loads, and "Not now" |
| `code/file/editor/grammar/register` | a grammar that loaded put in the kit's registry: its query with the kit's capture names, and a parser factory that hands each parse a fresh parser without compiling the module again |
| `code/file/editor/vim` | Vim in the editor: every key sent to core's state machine before the editor's own chords, each change it makes applied to the buffer as neither typed nor undoable by the kit and told to the sync as the person's, the registers shared by every file, and Vim switched on and off live |
| `code/file/editor/vim/look` | what Vim draws over the text: the cursor as a block fill on its character, a visual selection filled, and the kit's caret put where Vim's cursor is |
| `code/file/editor/vim/effects` | what Vim asks of the window beyond a change: write now, step to a change mark, scroll a line to a place in the view, and a selection to the clipboard |
| `code/side` | the files column: the one field over it, and under it the tree with its scope row, the go-to matches or the search; which list and every sentence is core's, a press sends its gesture, and each list keeps where it was scrolled and follows the cursor's row || `code/side/find` | the one field: its edits sent as they stand to whichever list is open, reaching it opening the go-to list, the search's three options at its right, and the keys of the jump and filter places run before its own || `code/side/jump` | the go-to list: the matches core ranked, the lit one, a press reading a path, and the line when nothing matches || `code/side/search` | the search: the include globs, one summary line, and the hits grouped by file with the matched text lit, drawn only while they answer the query and options as they stand || `code/side/tree` | the scope row and the tree: the checkout's name, All or Changed, the rows core ordered and marked, the ignored top-level entries held in one fold that opens to names only || `code/stage` | the Code stage: the loading bars until a page is open, the three columns with their sashes reporting on release, the page kept after the stage goes, the strip's trail and keys button, and the keys it takes as it appears || `code/stage/keys` | the Code layer of the key walk: the stage's keys sent as the actions they are, the window's own run as anywhere, Escape answered by the stage and never leaving it || `code/stage/outline` | the right column while the outline is on and a file is read: its header with the button that puts it away, and its sash on the left edge || `code/stage/widths` | how wide the columns are from the stylesheet's rules in their order: the usual width, the stage's own ladder, the narrow window's cap, what the person dragged, the wider results while searching and the outline given up under 900 || `code/start` | what fills the file area with no file read: the changed files and those opened lately as rows that read them, the hint, or the card offering the match the cursor is on |
| `code/side` | the files column: the one field over it, and under it the tree with its scope row, the go-to matches or the search; which list and every sentence is core's, a press sends its gesture, and each list keeps where it was scrolled and follows the cursor's row |
| `code/side/find` | the one field: its edits sent as they stand to whichever list is open, reaching it opening the go-to list, the search's three options at its right, and the keys of the jump and filter places run before its own |
| `code/side/jump` | the go-to list: the matches core ranked, the lit one, a press reading a path, and the line when nothing matches |
| `code/side/search` | the search: the include globs, one summary line, and the hits grouped by file with the matched text lit, drawn only while they answer the query and options as they stand |
| `code/side/tree` | the scope row and the tree: the checkout's name, All or Changed, the rows core ordered and marked, the ignored top-level entries held in one fold that opens to names only |
| `code/stage` | the Code stage: the loading bars until a page is open, the three columns with their sashes reporting on release, the page kept after the stage goes, the strip's trail and keys button, and the keys it takes as it appears |
| `code/stage/keys` | the Code layer of the key walk: the stage's keys sent as the actions they are, the window's own run as anywhere, Escape answered by the stage and never leaving it |
| `code/stage/outline` | the right column while the outline is on and a file is read: its header with the button that puts it away, a row for each definition with its kind's badge and a dot where it changed, the changes in the file, each sending the line it stands for, and its sash on the left edge |
| `code/stage/widths` | how wide the columns are from the stylesheet's rules in their order: the usual width, the stage's own ladder, the narrow window's cap, what the person dragged, the wider results while searching and the outline given up under 900 |
| `code/start` | what fills the file area with no file read: the changed files and those opened lately as rows that read them, the hint, or the card offering the match the cursor is on |
| `connection` | how the window trades with the daemon and its own views |
| `connection/caller` | how a view hands a `Call` on without knowing who answers it |
| `connection/cells` | what the Workspace's grid is as the rail last said it: the pane in each cell by its number, whether the grid is on screen, and whether a cell has the keys |
| `connection/heard` | what the window has heard from its session: the latest of each kind of event a view draws (the tree, the docks, the console's tabs and sides, the screens by side and the selection, the review, the history, the window-local Git entry state and the notice showing), in one model every view observes, whether the sidebar is held, which drags hold a resize (a pane waits for them all), and the `Run` a key in a pane asked of the window, said once to whoever opens it |
| `connection/screens` | the screens the window has, one per side a pane is drawn on (the Workspace's and the console's two always, a split cell's or a view's while its view lives), the frame each takes, and the one selection across them, which goes with the pane whose rows it was on |
| `connection/owner` | the state the daemon loop and the views' calls share, and the order their effects leave in; the session behind a mutex, the events out over a channel the UI thread drains, and the frame tick that lasts as long as the window listens to it |
| `extensions` | what the running extensions put in the window |
| `extensions/bodies` | which element draws each extension view: one for each panel, made when a dock, a floating frame or the stage first asks and kept, so a view moved between its dock and the page keeps its focus and a console its terminal; and which of them has the keys |
| `extensions/confirm` | the question an action asks before it is sent: `<label>?`, the extension's question, who it is from, Cancel with the keys and a danger button named by the action, answered once, and gone when the page says there is none |
| `extensions/dialog` | the question an extension puts to the window: up while the session holds it, a card made once for each question so what is typed and the count waiting follow in place, one that answers a command taking the keys and one raised on its own leaving them where they were |
| `extensions/dialog/body` | what the card draws: the title with who asked under it, the message and the kind's control (a field, a list of choices, a labelled field for each of a form's), and the foot with Cancel and the answering button |
| `extensions/dialog/card` | what the card holds while it is up: its fields' text, the lit choice, where the keys are in it, and the answers it builds and sends, each once |
| `extensions/dialog/keys` | the question's layer of the key walk, up only while something in the card has the keys: Escape cancels, Enter and Ctrl-Enter answer, Enter on Cancel is a press on it, and a pick's arrows move its lit choice |
| `extensions/install` | what the window does once for the extensions: their three key layers in the walk, the strip naming the page by its view, and the two questions brought up or away to match the session |
| `extensions/keys` | the two key layers of an extension's view: a view whose body was clicked, which passes a key it has no use for, and the page, which holds every key; both ask core what a key is to the view |
| `extensions/view` | an extension's view as one element docked, floating or the stage's page: its table or console body, the sort and hovered row the window alone keeps, and the keys the page takes as it appears |
| `extensions/view/actions` | the buttons and menus an action is offered through: a small button with its key cap, a row's text button with the key in its tooltip, and the "..." holding a row's actions while the view is narrow, each sending a select first when its row is not the selected one |
| `extensions/view/console` | a console view's body: a terminal on the screen the session gave the view, kept as the view moves, under a strip that says Starting or how it ended with the view's actions; the console with the keys takes the focus and one off screen lets its screen go |
| `extensions/view/head` | the row a view carries as the stage's page: who it is from, the view-wide actions with their key caps and the Dock button |
| `extensions/view/table` | the view's table: core's shaped columns and rows, the columns that give way as the box narrows, and the sort a header press makes |
| `extensions/view/table/head` | the table's header: each shown column's title as a button that sorts by it |
| `extensions/view/table/rows` | the table's rows: the selected one lit, a click selecting and a double click taking the default on that row, a chip as a dot and a word, and the actions at the end of the row selected or pointed at |
| `keys` | how a key or committed text reaches the session |
| `keys/calls` | what a key of the Anywhere or Window place asks of the session: the call the page makes for each stage switch, console key, composer, views key and zoom key |
| `keys/caps` | how a chord of the session's keymap is written for a menu or a tooltip: the leader's press first when the chord follows it, modifiers before the key |
| `keys/extension` | which extension command a key is among those the session lists, in the scopes a layer hears, and the call that runs it with the row the rail has picked |
| `keys/field` | the first layer of the key walk, a field: a pane's screen sends every key to the session, a kit input types its own, and a field that registers a place (the jump field, the history filter, the comment box) has that place's keys run before its own handling |
| `keys/focus` | which layer has a key outside a pane, with no window in it: the layers that are up in `LAYERS` order, whether one takes the keys from a pane, and the Escape ladder that asks each layer's hook until one answers |
| `keys/hooks` | which layers and places of the keymap this window hears and which arrive with their surface, matched exhaustively so one added to the keymap is listed, and held to what a built window registers |
| `keys/layer` | what a layer is to the walk (is it up, what does it do with a key, what does Escape put away in it), the registry surfaces put themselves in, the places a key is looked up in, and the way the window's commands reach the session |
| `keys/layers` | the layers whose state is the window's own rather than one surface's |
| `keys/layers/menu` | a menu or a question over the stage: the kit's menu stepped before the dialogs under it hear anything, and a question answered no by Escape |
| `keys/layers/own` | a dialog that answers its own keys (the settings, add-project, an extension's question): up while its overlay is, holding every key and running none, but for the settings chord, which puts the settings away |
| `keys/layers/figure` | a drawing opened larger holding every key while it is up, ahead of the window's own keys and every layer: Escape closes it, `+` `-` `0` and `f` move it, any other key is held and does nothing |
| `keys/layers/sheet` | the shortcuts sheet: `?` and Escape close it, every other key is held for its list and find field |
| `keys/layers/stage` | the stage, always up and last: the Anywhere place's keys run as the page's `run` does, any other key held |
| `keys/over` | opening the palette, the sheet or the settings over the stage for a key and for a `Run` from a pane, one of the three at a time, the settings chord putting them away: all three are surfaces the shell builds |
| `keys/palette` | the command palette: a search field over core's ranked rows, highlighted by a row's key, run by Enter or a press, telling the session when it opens and closes |
| `keys/palette/rows` | a palette row drawn: its mark, its text with the matched characters marked, its detail and what it says at its end |
| `keys/place` | what a key is in a stage's own place: its action, one beyond it (Anywhere's), or held; the one lookup the Git tabs and the Checkouts table share, and Files and Code will |
| `keys/sheet` | the shortcuts sheet: core's sections beside a find field, opened on the section for where the person is, its keys on the close button |
| `keys/sheet/body` | a section of the sheet drawn: its rows as an action and its caps, the pane's note and the keys it keeps without its leader |
| `keys/page_key` | which `PageKey` a GPUI keystroke is: the DOM's names for the keys, the character typed rather than the key printed, AltGr as a character and not a chord |
| `keys/root` | the one interceptor ahead of every focused child: a keystroke translated to the page's key, the window's zoom keys first, then the layers that are up asked in order until one holds or runs it |
| `keys/run` | what an action of the Anywhere place does (open one of the three over the stage, or ask the session), refused while a menu or question is up, and the Escape ladder |
| `keys/text` | the text an input method commits into the pane: GPUI's input handler for the terminal view, which holds no text and draws no preedit |
| `kit` | widgets and machinery that more than one surface draws or follows |
| `kit/alert` | the parts of a question before something cannot be taken back: the danger tile, the warning callout and the foot whose Keep it has the keys; the project's removal question and the Checkouts stage's draw from them |
| `kit/area` | the area an element was laid out in, recorded for whoever turns a pointer's place into a place in it |
| `kit/button` | what the page's text buttons look like: `.btn` with its primary and small forms, and the `.chipf` choice chip, as pills that tint, lift and press under the pointer |
| `kit/caption` | the quiet type a page puts over a part of itself: the tracked capital label, what it says at its far end, the small muted line, and a note that can be found by what it says |
| `kit/copy` | a button that hands its text to Rust to copy and says `Copied` for 1200 ms, a second press starting the moment again |
| `kit/choice` | a row with a box or a ring to tick, `--row-h` tall with the label in the press: controlled, so it asks for the opposite of what it is given and draws what it is given |
| `kit/chips` | the small outlined words a row carries: a ticket key, a tag, the dot that says a row changed since the reconcile, and the bordered card of the surface colour |
| `kit/dialog` | the parts a dialog is cut from: the head with its close button, the foot bar, the field labels and the `Esc` cap |
| `kit/diff` | a diff in a virtual list so only the rows on screen are built: the view, its events and the slot under one row, the scroll kept across updates and the cursor brought into view; it reports a press and sends no call, and serves review, history, the patch block and the Code stage |
| `kit/diff/fold` | the lines between hunks the diff did not carry, and the buttons that ask for them |
| `kit/diff/geometry` | where the rows are and what is on screen in the window's pixels, for a ruler beside the list: a row found by its number, the content's height, how far the list is scrolled and a scroll to a place |
| `kit/diff/gutter` | the columns before a line's code: the number each half carries, the marker, and a unified line's pinned to the pane's left edge while the code scrolls sideways under them |
| `kit/diff/items` | what the list holds, in order: every row, and the comment box among them under the row it is about |
| `kit/diff/menu` | the menu a right-click on selected text in the diff opens, `Copy` under `Selection`, and no menu with nothing selected |
| `kit/diff/measure` | how wide and how tall the rows are: the cells of the widest line, the cells a split half wraps at in this pane, each row's height told to the list up front, and the width every row carries so a unified diff is one column as wide as its longest line |
| `kit/diff/paint` | a side's text painted with its highlight colours and its selection, and kept so a pointer's place can be read back from what is on screen |
| `kit/diff/pan` | moving the diff sideways without the scrollbar: the arrow keys' nudge and a held middle button |
| `kit/diff/rows` | what each kind of row draws: a hunk's heading, a unified line, a split pair of halves, and the border and click every row but a heading shares |
| `kit/diff/selection` | which text is selected: where a press of a line's code begins it, how a drag extends it over rows and past the list's edge, what it reads as, what copies, and the drag that crossed rows told to a view that picks |
| `kit/diff/shape` | what a row is, from the kind the session gave it, and how tall it is |
| `kit/diff/text` | a line's text as it is drawn: tabs stopped by cells, control characters shown, highlight kinds cut into spans, how many cells a text takes, and the stretches a selection splits them into |
| `kit/diff/tone` | which colour a highlight kind and a side get: the tone of a token, the wash of an added or removed side, and what the pick and the cursor lay over it |
| `kit/diff/wrap` | where a long line breaks in a split half (at the cell, never at a word), and how a place in the text maps between the line and its broken form |
| `kit/drag` | the one drag helper: a press that becomes a drag at 5 px and a click before, Escape cancelling with nothing dropped, the pointer followed over the whole window and the keys taken for the drag and given back |
| `kit/crop` | one line of text longer than its box, scrolled to its first mark and faded where it is cut: the page's `.hline .tx` and `cropHits` |
| `kit/field` | a text field that Rust also writes to: what Rust says is written only when nobody is typing in it and it says something else, so an echo never lands over the person's own edit |
| `kit/finder` | the field over a list of files: a lens at its left and at its right the key that takes it or whatever the surface puts there |
| `kit/flow` | a sentence with a cap or a name in it, wrapped word by word as the page's inline text wraps, since GPUI has no inline elements |
| `kit/fonts` | which typefaces the window ships (Inter, JetBrains Mono and the Nerd Font symbols, converted from the page's woff2 files, with their licences) and their registration with GPUI's text system |
| `kit/ghosts` | what a list or a diff looks like while it loads: the page's shimmering `.ghost` bars, each a share of the width or a fixed width, indented or not |
| `kit/hints` | the footer that names a tab's keys, as caps and what each does, shared by the Changes and History tabs |
| `kit/icon_button` | what the page's `.icon` button looks like: 28px, muted, lifting to the fill and the text colour under the pointer, pressing to the fill with the text over it, and the lit one |
| `kit/icons` | which glyphs the window draws that the kit's Lucide set lacks: the page's own icon set as SVG files, handed to the kit's `Icon` as bytes |
| `kit/ink` | what the drawn blocks paint with: a drawing in its own units mapped to the window's pixels, and strokes, dashes, fills, dots, boxes, arrowheads and baseline-placed text painted in them |
| `kit/keycap` | what one key's cap looks like wherever a key is named, in the sidebar, the hints, the menus and the comment buttons: the page's `.kbd`, and a row of them with alternatives apart by a slash |
| `kit/menu` | menus: what a menu offers as one model, and how it is drawn and keyed |
| `kit/menu/at` | a menu a view builds itself and shows at a point, kept 8px inside the window, with the keys, dropped when it says it was dismissed: the bind menu a key opens under the lit step |
| `kit/menu/item` | what a menu offers as data: headings, rules and choices, a choice saying what it does, why it cannot be chosen now, its hint and whether it is dangerous |
| `kit/menu/keys` | the keys a menu answers beyond the kit's: `j` and `k` step the highlight as Down and Up do |
| `kit/menu/popup` | a menu drawn once from the model into the kit's popup menu, anchored by the kit at a point or under a button, a choice run after the menu has gone and never while the daemon is gone |
| `kit/menu/selection` | the menu a right-click on selected text opens: `Copy` under `Selection`, asked of Rust |
| `kit/menu/stepping` | where Down, Up, `j` and `k` leave a menu's highlight: on a choice that can be chosen, the first press and the wrap never on a heading; the Menu layer of the key walk asks it for each key while a kit menu has the keys |
| `kit/mixed` | a box to tick that can also read mixed, the select-all of a table, controlled like the rest of `kit/choice` |
| `kit/overlay` | overlays over the window: which are open, what opening one closes, and who answers Escape |
| `kit/overlay/host` | the window's overlays opened and closed in one place over the kit's dialog layer: the palette, sheet and settings exclusive, Escape and the scrim answered by the topmost that can be dismissed or by one kind (a question answered no), a dialog that does not take the focus, and a pane given the keys back when the dialog that took them closes |
| `kit/overlay/kind` | which overlays the window has and the rules each is held to: exclusive of the others of its kind, taking the keys from a pane |
| `kit/overlay/stack` | the overlays that are up, bottom to top, and the rules that decide what opening, closing and Escape do to them, with no window in it |
| `kit/pill` | the one status pill: a dot and a word on a tint of how loud the status is, the words a status is said in, and a feature's standing in an agent's vocabulary |
| `kit/progress` | a progress bar: a fill in a small track, and the whole percentage a share of tasks is |
| `kit/scroll` | bringing a child of a scrolling list into view by the least movement, from the bounds the last frame left |
| `kit/section` | rows listed under headings: grouped in the order a heading was first met, and the heading with its count |
| `kit/segment` | a choice of a few drawn as pills in one rounded track: the Git tabs, the side and the layout |
| `kit/sequence` | a sequence diagram painted from core's layout: lifelines, boxes at head and foot, stick figures, arrows with heads, numbered dots, self-call loops and branch bands, one element for every place a diagram is drawn |
| `kit/split` | a resizable pair: a column at the width its sash gives it, the width reported once on release, a double-click giving the usual width back, and the drag held while it lasts |
| `kit/selected` | how a list row in a dialog says it is selected: the fill and the theme's mark |
| `kit/spots` | the places a carried pane can be dropped, published by the views that draw them (the grid's cells and stage, the rail's checkout rows), the checkout lit as a target, and whether a pane is carried |
| `kit/spinner` | the ring that turns while something is asked and not yet answered: a button waiting on a press and a view's tab waiting on a refresh |
| `kit/start` | a checkout's start menu as it is drawn wherever it opens, a shell then an agent per template with the session's cursor highlighted; the rail opens it under a row and the Checkouts stage above New pane |
| `kit/status_dot` | what a pane's or console's status looks like with no word: a turning ring, a filled dot, a dash, a hollow ring |
| `kit/strip` | the page's one tab strip, drawn once for the docks, the console and a split side: the bar, the tab chip with its selected and hover states, the close, the menu button |
| `kit/textarea` | a box of several lines that Rust also writes to and that never loses what is being typed: written only while it has not the keys, the caret put after its last character |
| `kit/wheel` | which scroll box takes the wheel: one that can still move the way it turns, and none else, so a log or code block at its end hands the wheel to the page around it |
| `kit/tip` | a hover tooltip for a control, built one way so no surface builds the kit's |
| `kit/toggle` | the page's switch: 40 by 24, a pill and a knob, controlled, drawing the state it is given and asking for the opposite |
| `kit/tree` | a checkout's file tree as one widget for the Files panel and the Code stage: a row with its chevron, glyph, name, controls and mark, the fold row for the ignored top-level entries, and the rows with the held ones dimmed under it |
| `layout` | where the stage and the docks sit around each other, and how a panel is moved between them |
| `layout/carry` | a pane carried by its header or from the rail: what is carried, and the handle any view presses it through into the work area's one drag |
| `layout/carry/landing` | where a carried pane would land if let go at a point (another checkout's row, a cell's zone, the stage with no cell under it, or nowhere) and the call that makes it so |
| `layout/carry/preview` | what shows a carried pane: its name following the pointer, the zone it would land on and the stage outlined when it would open there |
| `layout/dock` | one dock drawn from Rust's `DocksPage`: its tab strip while it holds more than one panel, the shown panel under a head of its own while it holds one (the console's own strip is its head, and a press in the top of it carries the panel), its sash with the page's grip, and the press each of them makes; dimmed and not answering while the daemon is gone |
| `layout/dock_menu` | what a panel's dock menu and a dock tab's context menu offer as the one menu model, the dock a panel is already in saying `here`, and the `Call` each sends |
| `layout/bodies` | which view draws a panel's body and which a stage that is a page, offered once by the surface that owns the content and asked by a dock, a float frame and the stage |
| `layout/drop_target` | where a panel dropped at a point would land (a dock, or the stage for a panel that has a page), the area to preview, and the call the drop sends |
| `layout/float` | the dialogs over the work area, each in a frame of its own (head, the panel's body, a corner grip) that raises on any press, moves by its head and resizes by its corner through the work area's one drag, draws the box the pointer has made until it is let go and reports it once; dimmed and not answering while the daemon is gone |
| `layout/holders` | who is holding a resize: each drag holds under its own name and the panes wait until all have let go |
| `layout/panel` | what a docked panel's body draws while it is not ported: its name and a quiet line that it is not in the GPUI window yet (the console is drawn by `panes/console`) |
| `layout/sash` | what size a dock has with its sash under the pointer: measured from the dock's own edge, the grab offset kept, held to Rust's bounds |
| `layout/tools` | what a docked extension view carries in its dock's tab strip: a button that opens it as a page and a `...` menu of its view-wide actions and the keyed ones run on the selected row |
| `layout/work` | the work area under the header: the stage between a left, right and bottom dock, and the one drag that carries a panel or a pane or sizes a dock, sending its `Call` only when it ends |
| `panes` | the panes on screen and how each is drawn |
| `panes/console` | the console: a tab per console the daemon holds with its dot, the strip's buttons and menus, and its first and second side each drawn with a terminal of their own under a header while split; every gesture is the page's `Call`, nothing on screen changes before the `Console` event |
| `panes/console/focus` | which screen of the console has the keys: the session's `typing` says, and GPUI's focus follows it unless a rail pick or a field has them |
| `panes/console/head` | the console's strip: a tab per console with its dot, the new, menu and hide buttons, and the context menu of a tab |
| `panes/console/menu` | the console's offers given to the one menu drawing: each choice sends its calls through the caller or opens the rename dialog once the menu has gone, and is disabled while the daemon is gone |
| `panes/console/rename` | the rename dialog: an overlay of the host with one field, `RenamePane` asked of the daemon on Enter (never for a blank or unchanged name), Escape closing it, the keys given back to what had them |
| `panes/console/sides` | the console's sides: one screen under its header while split, the header's pick and take-out, and what to press when no console is on screen |
| `panes/console/strip` | what the console's tab strip and menus offer as plain data in the one menu model: which tabs wear a dot, a tab's menu, the more menu (split, stage, fullscreen, the docks), and what each choice does |
| `panes/element` | the element that paints a pane's screen: one measured cell, shaped text runs and merged background quads, the selection washed over them, the cursor, and the room it holds said once per size |
| `panes/grid` | how many rows and columns a size in pixels holds: whole cells, never fewer than one |
| `panes/layout` | how the rows a screen holds become runs of same-styled text and rectangles of background, for the cells that fit |
| `panes/palette` | which colour a packed wire colour is in the terminal's palette (the theme's sixteen, the cube and the greys), and which text stays readable on a background a program chose |
| `panes/selection` | the cells a drag across a screen covers in reading order, per row, as the page draws them |
| `panes/size_label` | what the stage header says of the Workspace's grid: a parked view's place at rest, the size for a second after it changes, with time a value the caller reads |
| `panes/split` | the Workspace's panes side by side: a card for each pane the rail's grid names, placed by its sizes, one screen for each cell beside the first, with a gutter on each seam and a header on each cell when there are two or more |
| `panes/split/cells` | what fills a cell: a terminal, or a browser's view; and the empty body of a pane the tree no longer names |
| `panes/split/focus` | which cell's screen has GPUI's focus: the one the session says has the keys, and a focus that lands on the Workspace's screen sent on to it |
| `panes/split/gutters` | the gutters between the cells and the drag that sizes the two panes either side: sent once on release, held until the session echoes it or 600 ms pass, a double-click evening the row or the rows |
| `panes/split/header` | a cell's header: its title and status, the close that takes it out of the grid, the keys marked, and the press that carries the pane |
| `panes/screen` | what the window holds of the Workspace pane's screen: every row's segments, the cursor, the parked view and whether the child asked for the mouse, and how a full, partial or resized frame is applied to them |
| `panes/terminal` | a live pane's screen, the Workspace's on the stage, one of the console's two sides or a split cell: what it draws of the pane its side shows, who has the keys, the field it is in the key walk that sends every key as the page does (the command key's chords on a Mac left to the window), the palette read again when a theme is installed, and which screen is bound to its side |
| `panes/terminal/label` | the Workspace's screen keeping what the stage header says of its grid as frames arrive, and telling the header when the words change |
| `panes/terminal/pointer` | the mouse over a screen in GPUI's events: the press that claims the keys and clears selections, the child's reports, a drag that selects and copies on release, the wheel in notches, all followed on the whole window |
| `panes/terminal/room` | the room a screen reports to the session: once it has held still, only when it changed, and not while a sash is held |
| `panes/terminal/skeleton` | the bars that stand in for a pane a screen waits on, 250 ms after the wait began and gone with the first frame or the end of the wait |
| `probe` | the scripted probes an agent runs against a daemon without a person: show a pane, open a checkout's review, open its history, each printing what the stage drew at every step, reading or opening and shutting a layer and changing nothing in the checkout |
| `report` | what the window heard, in words: the whole of it on one line for the unattended run, and the pieces (the review, the history, the docks, the console, the screen) the probes print |
| `review` | the Git stage: how it is entered and left, and the Changes and History tabs it holds |
| `review/changes` | the Changes tab: the review of a checkout's working changes or of one commit as a full stage, its files beside its diff, every field of the session's `ReviewPage` drawn, and each press sending the page's call and changing nothing until the event |
| `review/comment` | the comment box under the picked lines: what it is about, the agent it goes to, the words kept until the session shuts the box, and the `Writing` place's keys run in the key walk ahead of the textarea's (Ctrl Enter and Meta Enter sending, Escape putting it away, a bare Enter left to type a newline) |
| `review/comment/ways` | what the comment box says to the session and how it sits among its rows, the one thing the review's box and the Code stage's differ in: the call that sends or puts it away, the call that chooses an agent, and the room round the card |
| `review/entry` | how the window enters and leaves the Git stage and the calls each way sends: Git opens the review of the checkout the window is on, a tab closes the other and opens its own, any other way out closes what is open; the stage is the session's to say |
| `review/files` | the changed-files column: the side read, the files or the jump field's matches, the lit file brought into view, and the jump field, a field of the key walk whose `Jump` place steps, chooses and closes ahead of the input's |
| `review/filter` | the history's filter field in the stage's strip: each edit sent as it stands with no debounce, its text its own until the history shuts, and the `Filter` place's `Leave` (Enter and Escape) run in the key walk, giving the list its keys back without closing the history |
| `review/history` | the History tab: a checkout's commits unfolding to their files beside the diff of the file under the cursor as a full stage, every field of the session's `HistoryPage` drawn, the diff only once an event carries it, and each press sending the page's call and changing nothing until the event |
| `review/history/commits` | the commits column: its header with the fold-all button, a commit's row and the files under it once it is open, or the loading bars and the reason there are none |
| `review/history/diff` | the diff pane beside the commits: the file under the cursor and its commit with the way to open it as a review, and the diff or the reason there is none |
| `review/keys` | which gesture a key is while a tab holds the keys, read from the session's own bindings: the review's, the history's (`/` unsent) or the jump field's |
| `review/stage` | the Git stage under its header: the branch, the Changes and History tabs, the count, refresh and close, each tab's heard, taking the keys as it appears and giving a closed field's or a closed history's back |
| `review/stage/keys` | the Git stage's two layers of the key walk, Changes and History: up while the stage has the keys on that tab, hearing the open tab's keys, the sideways scroll of the diff on screen, and the copy of what a drag selected, and holding every other |
| `review/stage/strip` | the strip above the tab: the branch, the tabs, the files' count, the refresh and close buttons, and the filter field while History is the tab |
| `style` | how a page token lands on an element: its type (weight, size and line height) as one call |
| `system` | what the GPUI window asks of the machine it runs on |
| `settings` | what the person chose for the window: the look it puts on, the dialog that changes it, and the fonts the machine offers |
| `settings/font_picker` | a font field: a family typed or picked from the installed ones, drawn over core's list, each row in its own face, the keys of the palette answered while it is up |
| `settings/installed_fonts` | which families this machine has, read off the UI thread from the text system by every name, monospaced when a narrow and a wide letter are one advance wide, the window's own faces left out |
| `settings/look` | the look put on the window as the session sends it, whether the dialog is open or not: the theme and fonts in the token space, the backdrop, the zoom as the rem size, the frame's border; and the system going dark or light |
| `settings/preferences` | the Settings sheet: six sections down one scrolling page with a nav, every control sending its choice at once and drawing what the session says back |
| `settings/preferences/appearance` | the Appearance section: the theme cards, following the system, the opacity slider, what shows through, the two fonts and the notes under them |
| `settings/preferences/cards` | the theme cards: a picture of each theme in its own colours, its name, the one drawn lit with a tick |
| `settings/preferences/nav` | the six sections and the column that names them, the one in view lit |
| `settings/preferences/plain` | the five sections after Appearance, and the segment and the switch every choice of a few is drawn as |
| `settings/preferences/rows` | what a row of the sheet and a section of rows are made of |
| `system/alerts` | the alerts a session raised handed to the operating system's notifications from the window, each its own, a click activating the window |
| `system/chime` | the chime out of the platform's own player (`PlaySoundW`, `afplay`, `pw-play`/`paplay`/`aplay`), with no audio crate, best effort |
| `system/clipboard` | the system clipboard through arboard, as `effects::Clipboard`: text, a picture encoded as a PNG, and the files a file manager copied |
| `theme` | the one design token space the window draws with: every colour (the palette layer, the sixteen ANSI colours and the semantic roles) and every number (type, spacing, radii, sizes, shadows, focus, motion, faces) under the stylesheet's own names, the active one a global that every view reads |
| `theme/choice` | which theme a choice draws, Mocha or Latte following the system, and the token space of it at the opacity chosen; every theme held to the stylesheet it was written from |
| `theme/colour` | what a colour is in a theme's data (`#rrggbb` or `rgba()`) and the two mixes the page's `color-mix` formulas are made of |
| `theme/define` | what a theme is as data: its base palette, its sixteen ANSI colours and the overrides it lays over `:root`, each named for the variable it sets |
| `theme/derive` | how a theme's palette and overrides become the whole token space by the page's formulas: the palette aliased to the semantic roles, the overrides laid over them and `:root`'s scale, the remaining roles mixed from the result |
| `theme/kit` | how the token space becomes gpui-kit's theme, the one place the kit's roles are filled, and how installing another set of tokens switches the theme |
| `theme/syntax` | the colours of a tree-sitter capture from the token space: each kind of `argus-protocol`'s capture table as a theme colour, the kit's syntax colours built from it, and a highlight query with the captures the kit would leave uncoloured renamed to the ones it colours the same |
| `theme/scale` | the page's scale as plain numbers (type, spacing, radii, sizes, shadows, focus, motion, the mix shares) with `:root`'s values, and how a theme's overrides change them |
| `theme/see` | what shows through the window: the background-opacity setting as the alpha of the three large fills and of nothing else |
| `theme/stylesheet` | the check that holds Ledger's token space, and the reader every theme's check uses, to the stylesheet: its palette, ANSI colours, scale and derived roles read back from `style.css` |
| `theme/themes` | every theme the window ships, as data in the one token space |
| `theme/themes/carbon` | IBM Carbon's Gray 100 as data: neutral greys, Carbon blue, square and sober |
| `theme/themes/catppuccin` | the four Catppuccin flavours as data, Mocha the default, Latte the one light theme: the palette layer alone over `:root`'s shape |
| `theme/themes/flexoki` | Flexoki's dark as data: warm ink on black paper |
| `theme/themes/kanagawa` | Kanagawa Wave as data: sumi ink greys and a wave-blue selection |
| `theme/themes/ledger` | Ledger as data: near-black greys, one indigo, hairlines at a single step |
| `theme/themes/night` | Argus Night as data: a cool navy ground, translucent lines and one periwinkle accent |
| `theme/themes/nord` | Nord as data: Polar Night greys and Frost blue |
| `theme/themes/radix` | Radix Colors' slate and blue as data |
| `theme/themes/rose_pine` | Rose Pine as data: dusky violet surfaces and rose as the one warm accent |
| `views` | the renderers an agent's view is drawn with, shared by every surface that draws agent content |
| `views/actions` | the buttons an agent put in a view: a press tells the agent through its pane or, in a saved view, the daemon; a button waits with a spinner, is disabled with a reason when the daemon cannot take presses, and asks first when it has a confirm |
| `views/agent` | the views agents showed, drawn: the panel with its tab strip and the page over the stage |
| `views/agent/acts` | what a view's head offers to press: the agent's buttons, then the `...` menu of refresh, pop out or put back, and close, each a request |
| `views/agent/head` | a view's header: title, status and buttons on one row, then the subject link and the meta line whose age the panel draws again on a timer |
| `views/agent/install` | where the agents' views are drawn: the panel body, the page over the stage, and the page's title, status pill and buttons in the strip |
| `views/agent/panel` | the one content element of the agents' views, as the panel with its tabs or as the page, keeping the reader's place in a view and drawing the age again every half minute |
| `views/agent/tabs` | the views panel's tab strip: a tab per view newest first with its spinner and unseen dot, close and middle click, then the dock button and Hide |
| `views/blocks` | what each kind of block an agent's view holds is drawn as |
| `views/blocks/board` | a board block: columns of cards of a fixed width, scrolling sideways |
| `views/blocks/callout` | a tinted note in a tone with the glyph the tone means |
| `views/blocks/card` | a card block: blocks in a bordered box with a heading, a chip and buttons, the tone only tinting the edge |
| `views/blocks/changed` | how a block that changed since it was seen is marked: one wash in the changed colour that fades once per change, and the dot a tab wears while its view changed out of sight |
| `views/blocks/chart` | a chart block: the chart in a box that stops at 720 wide with its bars and dots as elements that read their value, and a legend when there is more than one series |
| `views/blocks/chart/legend` | a chart's legend: each series' line in its dash and tone, and its name |
| `views/blocks/chart/paint` | the grid, labels, lines and areas of a chart painted from core's layout, a line keeping its width as it scales |
| `views/blocks/chart/spark` | a spark: a small line with no axes as wide as what holds it, in a stat's tile or as a chart of its own |
| `views/blocks/checklist` | a checklist block: what is done and what is not, read-only |
| `views/blocks/code` | a code block with its language and a Copy button over scrolling monospace text |
| `views/blocks/columns` | a columns block: side by side while the measured box has room, stacked when it has not |
| `views/blocks/details` | a details block: a summary that opens to its blocks, open or closed as the person last left it |
| `views/blocks/diagram` | a diagram block: a sequence diagram painted from core's layout in a scrolling box with its source folded under it, or the source as code when Rust could not read it |
| `views/blocks/dispatch` | a view's blocks each handed to the module that draws its kind, by the path of the block under the view |
| `views/blocks/embed` | a saved view drawn inside another in a frame, or the line that says why it is not |
| `views/blocks/fields` | a label column beside a value column, the shape of a ticket's details |
| `views/blocks/figure` | a drawing a block makes that opens larger: the corner button and the press on the drawing, and what a block hands over (a title, a size, a way to draw at any scale, a source) |
| `views/blocks/figure/bar` | the larger view's bar: the title, the hint, the zoom group, Fit, Source and Close |
| `views/blocks/figure/larger` | the larger view's state: where the drawing sits, whether its source is shown, and the drag that moves it after 5 px |
| `views/blocks/figure/open` | putting a larger view up in place of the one up, and reaching it so the keys can move it |
| `views/blocks/figure/stage` | the larger view's stage, measured each frame, zoomed by the wheel about the pointer and dragged, and the pane that shows the source |
| `views/blocks/form` | a form block: fields in a grid of columns, what is typed kept by form path and field name, the answers read at the press, and the drafts and boxes dropped when it is sent |
| `views/blocks/graph` | a graph block: core's layout in a framed box scaled down to its panel and never up, each box an element that is pressed where it opens a saved view, and the nodes that open one listed under it |
| `views/blocks/graph/paint` | a graph's arrows painted from core's layout: right-angled routes with their heads, and each edge's label haloed in the box's colour |
| `views/blocks/grid` | a grid block: equal columns as many as fit at a named minimum width |
| `views/blocks/kept` | what the blocks of a view keep between draws that is not the person's: a Markdown with its selection, a Copy button, a log's scroll, the widths boxes were measured at |
| `views/blocks/list` | a list block: comments and events, an item with no person an event on one muted line under a dot |
| `views/blocks/log` | a log block: a tail of lines by level that follows the newest unless the person scrolled up |
| `views/blocks/markdown` | Markdown drawn as a view: core's blocks as headings, paragraphs, lists, quotes, code and tables whose text can be selected and copied, a link asked of Rust, and the selection this view's own |
| `views/blocks/markdown/draw` | what each kind of Markdown block is drawn as, its leaves numbered in the order core numbers them |
| `views/blocks/markdown/hover` | which link of a Markdown the pointer is over: underlined, the hand, and its address beside the pointer |
| `views/blocks/markdown/menu` | the menu a right-click on selected Markdown text opens: Copy under Selection, asked of Rust |
| `views/blocks/markdown/runs` | a leaf's text as one wrapped run: marks as faces and colours, the selection as a wash, and the layout kept to read a pointer's place from |
| `views/blocks/markdown/select` | which Markdown text is selected: where a press begins it, how a drag extends it across leaves and past the text, what a click on a link asks for, and what copies |
| `views/blocks/matrix` | a matrix block: cells tinted by tone and weight under column names and beside row names |
| `views/blocks/patch` | a diff block: core's rows of an agent's unified diff in the review's diff view, in a header with the path and counts, bounded in height; text with no hunk as a `diff` code block |
| `views/blocks/pen` | what one draw of a view's blocks holds: the theme, the clock, where presses go, what the window remembers and keeps, and the app to make entities in |
| `views/blocks/pending` | a block with no drawing: one a newer daemon sent, which says so quietly |
| `views/blocks/pickers` | the form controls more than a text box, kept between draws: a select for a choice, a date picker and a number box whose arrows step by core's rule |
| `views/blocks/progress` | a meter in the tone's colour, the compact one a progress value draws in a cell or a field with its percentage, and the progress block with its label and note |
| `views/blocks/row` | a row block: values and buttons on one line, wrapping, spread or at the end |
| `views/blocks/section` | a section block, a quiet heading and the saved view it opens over its blocks, and the divider between two |
| `views/blocks/stats` | a stats block: tiles that share the width at as many a row as the measured box fits |
| `views/blocks/table` | an agent's table block: the rows as a table that scrolls sideways, sorts by a header, picks rows, opens a row to blocks and copies as TSV, each part named under `<path>-table` |
| `views/blocks/table/bar` | the bar over a table: the filter, the counts, the agent's actions on the ticked rows pressing with their keys in the order shown, and Copy as TSV |
| `views/blocks/table/body` | a table's rows: a tick or a radio, the chevron that opens a row, the cells with a long text cut and its whole text for a tooltip, and the opened blocks in a full-width row under it |
| `views/blocks/table/head` | a table's header: select-all, a blank over the chevrons and each title as a sort button, and the one place a column's width becomes a flex track |
| `views/blocks/tabs` | a tabs block: the one tab strip over the blocks of the tab chosen, held to the tabs there are |
| `views/blocks/timeline` | a timeline block: events on a rule their dots hang on, in lanes side by side or stacked narrow |
| `views/blocks/tree` | a tree block: nested nodes that fold, each remembered by its place under the view |
| `views/blocks/view` | the one element a surface holds to draw a view's blocks, handed the blocks and whose view it is whenever its model changes |
| `views/chip` | a status pill an agent drew, in the theme's hue for its tone, and the quiet dotted form of it |
| `views/ctx` | what every block of a view is drawn with: whose view it is, the block's path, what changed, and what a value is drawn against (tokens, clock, where presses go) |
| `views/link` | a link in an agent's view: a press asks Rust to open its address, which is shown only as a tooltip |
| `views/memory` | what the window alone remembers of a view's blocks (the tab read, a fold, ticked rows, a filter, a half-typed form, where a log was left) by block path, kept apart from the snapshot so a republish resets none of it and let go with the view |
| `views/saved` | a saved view in the GPUI window: its panel with the trail of columns, one element wherever it is shown |
| `views/saved/column` | one column of a saved panel: its header over its scrolling body, and the stand-in for a view not held |
| `views/saved/delete` | deleting a saved view: the `...` menu's one item and the question it opens in place, kept in the window's memory |
| `views/saved/details` | the Details fold under a column's header: scope, built, features, recipe and sources |
| `views/saved/head` | the strip across a saved panel that has a head of its own, and the marks a floating one's title bar wears |
| `views/saved/header` | a saved column's own header: title row, meta row with its buttons, Refresh, Mark stale, Details |
| `views/saved/panel` | a saved panel: the columns side by side, the scroll it keeps across and down, scrolling to a column added |
| `views/saved/registry` | the one element each saved panel is, held so docking or floating it keeps its place |
| `views/opens` | the small arrow beside an element of a view that opens a saved view: a press asks Rust to open it from the column it was in, and a view not held yet is dimmed and still pressed |
| `views/refs` | the values that point into Argus (a path, a commit, a feature, a pane) as buttons that ask Rust to open them, and a duration and inline code |
| `views/values` | one value as a field, a cell or a list item shows it, from core's words |
| `window` | how the frameless main window opens around a view: the application with its assets, the window's options, and the stage put under the header |
| `window/frame` | what the platform window itself is told about the look: the DWM border colour and the tint Mica takes |
| `window/quit` | ending what ran for the window when the last one closes, and answering a close of the only window by quitting |

## Views

The production client follows `Argus Command Center.dc.html`: a two-row product-and-navigation
band, a stable contextual rail, one dominant stage, and a one-row command band. Three tabs
name the stage surfaces: Workspace, Feature, and Checkouts. Their digits still open them;
from inside a pane the digits belong to the child, so a view is reached through the leader
(`Ctrl-Space`, then the digit). The empty tree replaces the shell with the first-run surface rather
than pretending first-run is a permanent view.

The rail is stable across all three surfaces. Its heading is one row: the selected project's name,
which of the projects it is when there are several, and at the right edge how many of its agents
need a person, written only when some do. Under one rule it lists that project's repositories, each
a status glyph, a name and the bare count of the panes running in it. A repository with nothing
running is quiet and drawn dim. Every repository is listed: the active ones first, in tree order,
then a rule, then the quiet ones, the most recently used first — the daemon records when a pane last
closed in each, so the one just left is nearest to hand, and one never used comes last. A project
with nothing active has nothing to divide and draws no rule. An agent is listed
once, on its pane's row under its checkout: the rail has no second roll-up of agents. `o` or a click
on the heading switches projects through a picker. Selecting a repository
changes the existing daemon-tree selection; the rail owns no parallel workspace model. Workspace
holds the selected pane's real terminal beneath a flat breadcrumb header. Checkouts is an
operational table for the selected repository, and Feature lists the repository's features and reads the selected one as work, as a record
of its decisions, and as flows. The command band follows the active surface.

Switching views changes the stage and nothing else. Every pane keeps running, its subscription
stands, and the Workspace stage is one keystroke back. Focus moves to the stage while another view
is open — a key left reaching a pane that is not on screen is a key nobody can see the effect of —
and returns to where it was when Workspace comes back. Which view is open is this client's own
state and is never sent to the daemon: two people attached to one daemon are not necessarily
reading the same thing.

There are three views: Workspace, Feature, and Checkouts. The panes have no view of their own:
the rail lists them, each with its status glyph on its row, and the status written out as a word
only when it is one that needs a person. Review, history, settings, briefs,
and editors remain overlays because they are temporary work over the current surface. The command
center is the only presentation, and the one the client's tests draw: the five-column spine it
replaced, with its folding columns and draggable gutters, is gone rather than kept compiled beside
it, since a second presentation nobody sees was the one every test was running against.

## Navigation model

The runtime hierarchy is:

```text
Workspace scope -> Project -> Repository -> Checkout -> Pane
```

A workspace is daemon-wide scope, not a navigation column. Switching it changes every attached
client. Panes in other workspaces continue to run. The production TUI projects the selected
project's repositories into the stable rail and gives the selected pane's terminal the
remaining stage. Repository rows are ordered active-first, the quiet ones under their rule,
and stay collapsed until chosen. Any number can be open at once, each showing its checkouts with
panes running — and its primary checkout even with none, so there is always somewhere to start a
shell or an agent from the rail — and those panes, indented under it without connecting glyphs,
with the pane being viewed marked in place. Which are open is this client's own state, like the
open view. Only the cursor's row and the pane being viewed are filled; the selected repository is told
by its bar and bold name. Checkout and pane identity remain the same indices and IDs used by the daemon
tree. While typing in a pane, `Ctrl-Space`, `f` lets its terminal take the main content area;
repeating the chord restores the shell. The command band remains visible in both layouts.

The keys walk the rows the rail draws, in the order it draws them, so anything a click reaches the
keys reach too. `j`/`k` and the wheel step through the project heading, the repositories, and the
open repository's checkouts and panes; `l`/Enter opens a repository and moves into it, goes to a
checkout's first pane, or types into a pane; `h`/Escape climbs to the row above, folding an open
repository first when the cursor is on it. Stepping past a repository does not open it — walking
the list would otherwise unfold every row it crossed — and opening one leaves the others open; a
click on an open repository folds it. A checkout folds too, by `z` (on a pane, its checkout's, the
cursor going up to it) or a second click on it, and `z` on a repository is its fold as well: the
checkout keeps its row, saying how many panes it hides, coloured by the loudest, so one waiting on
a person is not put out of sight with the rest. Checkouts start unfolded, the other way from
repositories, since one is listed only because something runs in it, and `l`/Enter on a folded one
unfolds it. `h` does not fold a checkout, or every climb out of one would put it away. The checkout
holding the pane being typed into is never folded, and going to a pane from elsewhere unfolds its
checkout. Which are folded is this client's own state; the desktop window keeps its own the same
way, with a chevron in the row's left margin, Collapse in its menu, and `z` on a picked row, its
shown pane keeping its checkout unfolded as it keeps its repository open. The rule between active and quiet is drawn but is not one of
those rows: the cursor steps over it and a click on it lands nowhere. On the project heading `n` adds a project and `D` removes this one.
The rail's cursor is not stored: it is the selection read at the depth focus names, so a new tree
cannot leave the two disagreeing. Checkouts with nothing running, and branches no checkout is on,
are not rail rows; the Checkouts stage draws every one, and there `n`, `D`, `s`, `a`, `F`, `P`,
`R`/Tab, `H` and Enter act on the row its table has selected — Enter opens a checkout in the workspace, or switches
the primary checkout to a branch. The table keeps no scroll position apart from that
selection, so `j`/`k` and the wheel both move the row and the table follows it.

The consoles (see "Panes and terminal state") are the rail's last rows, drawn at its foot under a
rule of their own rather than after the repositories in the scrolling list: they belong to no
project, so they are the same rows whichever project the rail is on, and pinning them there keeps
them on screen however long the repositories run. The foot takes a row per console up to half of
the rail. `j` walks on into them past the last repository row, and a console picked is the stage's
pane like any other: shown with the keys left on the rail, typed into with `l`/Enter, closed with
`x`, its breadcrumb naming it a console rather than a repository. `h` on one stays put, since it
hangs under nothing. `c` anywhere on the rail asks the daemon for a console, and `s` on a console's
row asks for another, since a shell *there* is one; the row appears, and takes the keys, once the
daemon's list holds it. The selection holds a console by id, beside the tree's indices it leaves
untouched, and only while the Workspace is up and the cursor is not on a tree row: what needs a
checkout — review, history, git, an agent — finds none on a console, and the Feature stage keeps
reading the tree's checkout. A console that ends gives its row to the next, as a pane does, but
hands the keys back to the rail, and the last one ending puts the cursor back on the tree. A daemon
that does not list `consoles` gets no section, and `c` says it predates them.

The status bar's keymaps are written as tiers and the widest that fits is drawn, so a narrow bar
shows fewer keys rather than one cut mid-word. A card or table holding more rows than it can show
puts a thumb in the blank cell between its rows and its edge, sized and placed like a scrollbar's:
without it a list scrolls silently, and twenty checkouts in a table that fits six look exactly
like six.

The left of the bar counts the fleet — how many agents are working, how many are done, how many
are waiting on a person — ordered so the count you have to act on is the one nearest the corner.
It held a breadcrumb before, which said the path already spelled out across the workspace
heading. What is *not* written anywhere else on
screen is the state of the agents you are not currently looking at — the rail shows one project,
and one repository of it open — which is the reason the
bar counts them. Idle and exited panes are left out: a tally that counts them
reads the same whether anything is happening or not, and when nothing is happening the breadcrumb
comes back rather than the bar spending a row on `0 working`.

The bar is a reminder rather than a reference. It carries the handful of keys worth having in
front of you and ends, at every width and in every mode, with the one key that lists the rest —
appended after the tiers are chosen, so it is never the thing a narrow bar drops. `?` opens that
list as a window sized to its own content, grouped by what the keys act on rather than by
character, and answering for the mode you are actually in: the diff's keys in a diff, the
rail's on the rail. It opens *over* whatever raised the question rather than instead of it,
so the review you asked about is still there when you close it, and any key that is not a scroll
key closes it — having to hunt for the way out of a window you opened to be told something is the
problem it exists to solve. `?` is only this where nothing is taking text; on a prompt, in a
pane, in a filter or a feature line, or in a brief being written it is a character, and the leader
chord reaches the list from inside a pane. Which mode has the keys is decided once
(`app/gestures/mode`) and key dispatch, the bar and `?` each match on that answer exhaustively, so
a mode one of them forgets does not compile — the three used to work it out separately, in
different orders, and disagreed. What each mode's keys *are* is decided once too
(`app/gestures/mode/keymap`): a mode declares its bindings — the keys, what `?` says of them, the
bar's hint and how many narrower tiers keep it, and the action — and dispatch looks the key up
there, `?` lists it, and the bar is read off the hints. A binding can be kept to `?`, or out of
both, but only on purpose: a test fails when a key a mode takes is on neither its bar nor in its
`?`. The bars that depend on more than the mode — the feature stage's per panel, the leader, a
parked or fullscreen pane, a brief being written, and the typing surfaces — stay written out in
`ui/status`, and typed text and the leader chord are still dispatched by hand in
`app/gestures/input`, with the keymap only saying what `?` lists for them.

A project takes its repositories from a root directory, from paths named one at a time, or from
both. Each becomes a repository with its own primary checkout and linked worktrees. Repository
identity is carried through daemon state and the protocol, so worktree discovery, creation, and
removal stay scoped to the repository that owns the selected checkout.

A root is scanned for the Git repositories at or beneath it. The scan stops at each repository it
finds, so a submodule or a vendored checkout stays part of the repository containing it rather than
becoming a sibling of it. It skips `.git`, `.argus`, `node_modules`, and `target`, plus whatever the
project's own `exclude` adds and minus whatever its `include` overrides; it does not
follow directory symlinks; it goes no more than eight directories below the root; and it treats
neither a linked worktree nor a bare repository as a repository of its own. Like the rest of the
read-only Git work it uses `git2` rather than the `git` executable, and it returns its repositories
in path order.

The scan runs at startup and every ten seconds after, on the blocking pool and never under the
daemon mutex. A repository cloned into a root therefore arrives on its own, and one deleted out of
it leaves once it holds no panes — a repository still running an agent stays until it is empty,
because a directory can go missing for reasons that have nothing to do with the operator's intent.
A path the configuration names outright is taken at its word: one that is not a Git repository at
all still becomes a row, and no scan removes it. A root with no repositories under it is a project
all the same.

`i` in the repositories column is the one gesture that makes a repository rather than finding one:
the directory browser picks where it goes, a prompt names it, and the daemon creates the directory,
runs `git init` in it, and adds it to the project the same way a named path is added. An empty name
means the chosen directory itself, so a folder that already exists gets initialized where it
stands; a directory that is already a repository is added without being re-inited, since rewriting
the hooks of a repository that exists is not what the gesture asked for. `git init` does the work
rather than a `.git` written by hand, so the result is whatever the user's own git configuration
would have produced. The walk probes for a `.git` entry (or a bare Git directory) before opening libgit2,
so a root of ordinary directories is not a libgit2 open per folder.

Checkout rows use the branch currently occupying their path as their display name, including when
another process switches the branch outside Argus. A live agent can report that it has started
working in another known checkout in the same project. Argus then moves the existing pane under that
checkout without restarting its PTY or changing its id, title, status, or conversation. The client
follows the pane when a pane row or the terminal has focus; project and checkout navigation stays put.

## Configuration

Configuration uses `ARGUS_CONFIG_DIR` when set and the platform config directory otherwise.

- `projects.toml` declares workspaces, projects, and agent templates. Argus only reads it.
- `client.toml` stores the terminal client's theme, editor, and layout settings.
- `desktop.json` stores the desktop window's: what it remembers (each repository's index layout, the
  agent last started, the chime, the review's side and layout, the zoom) and what its Settings
  dialog sets.
  It is a file of its own rather than a section of `client.toml`. The two clients overlap only in
  the editor mode and command and a theme's name, and even those part ways: the window's theme can
  follow the system, which the terminal's cannot, and its notification is on by default where the
  bell is off. Every terminal client writes its whole `client.toml` from the copy it loaded, so a
  second writer's changes would be quietly undone by the next theme change in a terminal, and the
  file would become a contract both binaries had to keep in `argus-protocol`. Each client keeps its
  own, and an editor set in one is set again in the other.
- `runtime.db` holds everything Argus writes: panes to relaunch, projects and repositories added
  from the TUI, projects and repositories removed from the panel, workspaces created at runtime,
  when each repository last had a pane close in it, and the daemon-wide selected workspace.
- `argusd.log` is the running daemon's log, and `argusd.log.1` the run before it.

## Runtime storage

`runtime.db` is SQLite in WAL mode, opened once per daemon with `synchronous = NORMAL` and a
five-second busy timeout. Its schema version is `user_version`; a store from a newer Argus is
refused rather than migrated backwards, and a store that cannot be opened at all costs the run its
memory rather than its startup — the daemon falls back to an in-memory store, which also cannot
overwrite whatever made the file unreadable.

The dividing line against `projects.toml` is ownership: the file says what exists and is the
user's to edit, comments included; the store says what happened while Argus was running. So a
project added from the TUI is recorded as an overlay keyed on its root directory, an extra
repository is recorded against its project's name, and a project the config declares is recorded
as *hidden* rather than deleted from the file. Startup merges the two, the config winning for any
root that appears in both.

Persistence is a store a daemon is given, not a flag it can be told to set: `Daemon::new` hands out
an in-memory one, and only `main` passes the store on disk. That is what keeps the test suite off
the user's state.

Schema v3 and v4 added `note` and `note_audit` for project and checkout notes. Notes have since
been removed; the tables are still created on the way up from an older store, and nothing reads them.
Schema v5 adds `decision`, keyed by project name: one row per recorded decision, with `parent` a
self-reference rather than a foreign key, since a board is read whole and reassembled by a reader
that already has to survive a parent it cannot see.

Schema v27 adds `saved_view` (id, the board key and feature number it is filed under, JSON body and
sources, recipe, when and at which commit it was built, the checkout it was saved from, the stale
mark, the pane that saved it) and `saved_view_feature` (the features a view is attached to, by board
key and number). Neither refers to a feature by foreign key; `remove_feature` clears them as it
clears the other rows that name one.

`session.json`, `excluded-repos`, and `open-workspace` are read once, on the first start that finds
them, and renamed to `*.imported` afterwards. A `session.json` that will not parse is left where it
is, since a file this version cannot read is one a later version might.

The current project and agent schema is:

```toml
[[workspace]]
name = "work"

[[project]]
name = "src"
root = "~/src"

[[project]]
name = "argus"
repos = ["~/src/argus"]
workspace = "work"
worktree_root = "~/worktrees"
setup = ["pnpm install"]
exclusive = true
exclude = ["vendor"]
include = ["target/scratch"]

[[agent]]
name = "claude"
cmd = ["claude"]
env = { KEY = "value" }
restart = "on-failure"
```

`projects.toml` is watched, and a save reloads it into the running tree — projects, repositories,
their per-project settings, and agent templates. Nothing is rebuilt: what the file still names is
matched and updated in place, so ids stay valid and panes keep running. What it no longer names is
removed unless it is holding panes, in which case the row stays until it is empty. A file caught
half-written fails to parse and is logged and ignored rather than allowed to take the tree with it.
Harnesses are not reloaded: a running agent's hooks on disk were written by the harness it started
under.

Projects without a workspace use `default`. A project may set `root`, `repos`, or both; a path
reached both ways is one repository, and the two lists are joined with `repos` first. When no
agents are configured, `claude`, `codex`, `opencode`, `pi`, `agy`, and `agent` templates are supplied. Adding a project
at runtime records the directory given as its `root`, in the open workspace, so what it holds is
discovered again on each start rather than frozen into the record. `AddProject` may also carry a
name, folders to skip (kept with the project as its `exclude`, so a restart scans as the add did),
and `init`, which makes the directory a repository first as `i` does. A second project of one name
is refused, since the store keys a project's added repositories and its hiding by name.
`PreviewProject` answers with what the same scan would find under a folder, with its depth and its
built-in skips, and writes nothing: the desktop's add dialog lists it before anything is added. A
client sends either only to a daemon listing the `add-project` capability.

## Panes and terminal state

Shells, agents, and editors use the same PTY primitive. Editors exist in daemon state while open
but are omitted from the normal pane list and counts.

A console is a shell that belongs to no checkout: it starts in the home directory and lives in a
list of its own beside the projects, outside every workspace. Screens, sizes, keys and closing find
it as they find any pane; what only an agent does looks in the checkouts alone. Clients read every
pane as part of a project, so the consoles do not travel in the tree: a client that greeted with
`consoles` is sent `ServerMsg::Consoles` after the daemon's greeting and after every tree, and may
send `ClientMsg::SpawnConsole`. How many consoles are running is recorded beside the panes, in a
`console_pane` table of its own so that every `pane` row keeps its checkout, and a restart starts
that many fresh shells in the home directory; a console whose shell has exited is not counted.
Both clients show them: the desktop window as its console, the terminal client at the foot of
its rail ("Navigation model").

An agent whose process exits leaves its row as `Exited` unless its template sets `restart`:
`on-failure` starts it again on a non-zero exit, `always` on any exit, and `never` — the default —
leaves the row alone. A pane closed by the operator is removed before it is killed, so closing is
never a restart. Three restarts of one template in one checkout within a minute stops the cycle and
leaves the exited row, which is what says what happened.

A pane does not inherit the markers of a Claude Code session that started the daemon (`CLAUDECODE`,
`CLAUDE_CODE_CHILD_SESSION` and the like): a Claude agent in it would read them as that session's
child and save no transcript. What a person sets to configure Claude Code, such as
`CLAUDE_CODE_USE_BEDROCK`, is still handed on.

On Windows, every pane's console server and child run in a Job Object of the pane's own, set to
kill what it holds when its last handle closes. The process that opened the pane holds that handle,
so however it dies — a daemon killed, a test binary stopped — the OS ends its consoles and children.
Before this an owner's death orphaned its `conhost.exe` or `OpenConsole.exe` servers, which spun on
their broken pipes: 63 of them once held 129k CPU-seconds and stalled other runs' ConPTY tests.
ConPTY starts the server as a child of whoever opens the pseudoconsole and hands back no handle to
it, so `pty/console_host` opens one pseudoconsole at a time per process and takes the console-server
child that appeared meanwhile. Neither kind of job caps memory or processes: the agent's cap of
32 GiB and 256 processes ended agents mid-task. Everything an agent starts stays in its job, and
what a shell starts stays in the shell's unless it is started with `CREATE_BREAKAWAY_FROM_JOB`, the
counterpart of the `setsid` that escapes a Unix pane's session sweep: `argus` starts its daemon and
window that way, and `scripts/reload.sh` schedules its restart through WMI, whose processes were
never the pane's. Letting every child leave silently orphaned Git Bash's real shell, since
`bin\bash.exe` only launches `usr\bin\bash.exe`, and MSYS bash outlives its console closing; with
128 of them MSYS had no consoles left. Closing a pane or dropping its runtime terminates the job, the
console server first: `ClosePseudoConsole` waits for the server to exit, and a teardown that left
the server to finish on its own could wait minutes. For the same reason a pane's output is read to
the end even after its pump has stopped, so the server's last frame is never left in a full pipe.

Windows panes run on the ConPTY built into Windows unless `conpty.dll` and `OpenConsole.exe` sit
beside `argusd.exe`, which portable-pty prefers. Argus does not ship them: nothing works worse
without them, and shipping them means a third-party executable per pane and binaries to keep
current. portable-pty loads that library by bare name, and Windows' default search would go on to
the current directory — the daemon inherits the one `argus` was run from, often a repository — so
the daemon's first act is to confine loads by name to its own directory and System32
(`system/dll_search`).

Each PTY starts at 24 by 80 cells. A blocking reader thread sends output through a bounded queue to
a Tokio task, which feeds the pane's terminal emulator (`alacritty_terminal`) and broadcasts what
changed plus the child cursor's position, visibility and shape. The task wakes on the first byte, takes whatever else is
queued up to a bounded batch, and after a frame waits 16 ms before the next, so a stream is
coalesced while a lone keystroke's echo goes out at once. A second thread owns the child and blocks
until it exits, so an idle pane wakes for nothing: the task used to tick every 16 ms for the life of
the pane only to ask whether its child had gone. The reader cannot stand in for that thread, since
it need not reach EOF while the pane holds the pty master open.

The task keeps its own copy of the screen as protocol cells and converts only the lines the
emulator reports damaged since the last frame: a keystroke's echo converts a cell or two rather than
every cell on the screen. That is why the emulator is `alacritty_terminal` and no longer `vt100`,
which tracks no damage, so every frame rebuilt the whole grid. Measured at 200 by 50, a frame cost
310 µs for an echo, 341 for an Ink redraw, 416 for a full-screen app and 655 for forty scrolled
lines under `vt100`; 63, 182, 63 and 517 under alacritty, which damages the whole screen on any
scroll. What the watchers hold is tracked the way they track it — the last frame's scroll and runs
laid over the grid before — so the next frame is diffed against exactly what they have. A cell's
grapheme is stored inline rather than on the heap, and a blank still carries the attributes it was
cleared to, so a TUI's coloured bars survive.

The daemon is the pane's terminal, so it answers what a terminal is asked — cursor position,
device status and attributes — as the emulator reaches each question mid-stream, and the pump
writes the answer back through the pane's input. A child that asks waits for the answer: ConPTY
opened to inherit the cursor, as portable-pty 0.9 opens it, starts no child until it has one, and a
line editor that asks at every prompt otherwise stalls there until its own timeout. It honours synchronized updates (`CSI ?
2026 h`), holding a frame back until the child ends it, and the pump wakes at the update's deadline
so a child that never ends one cannot freeze its pane. The cursor's shape (DECSCUSR) is the
emulator's; a shape it reports only when nothing asked for one stands for the host's own.

What changed travels compactly (`damage`). Each frame is diffed against the grid the client holds
into runs of cells — a run's graphemes as one string, its looks as counted styles of three small
integers, and a count of default blanks at its end — with an unchanged gap of a few cells riding
inside a run rather than starting another. A region that moved is sent as a move: rows are compared
by hash, and the scroll that saves the most rows is taken, a region rather than the whole screen
because an agent's output scrolls above an input box that stays put. A scroll is looked for only
when more than one row changed, since it saves only changed rows, and the row hash is FxHash rather
than SipHash; together they took the diff for an echoed keystroke on a 200 by 50 screen from about
570 µs to under 60. The frame is then diffed against the grid after that scroll, so a wrong guess —
a hash collision included — costs bytes and never a wrong screen. A 200 by
50 screen of text is a few kilobytes rather than the 620 KB it was as per-cell records, and a one
line scroll of it under 200 bytes rather than 413 KB. A client that did not greet with `cell-runs`
is sent the per-cell form, built by its connection from the runs; a scroll, which that form cannot
say, puts it behind, and it catches up with a fresh grid as a scroll always cost it. Rows read from
a pane's history come back as runs to a client that reads them, and as cells to one that does not.

The client also bounds incoming daemon messages and coalesces redraws to the same interval. Cursor-only
changes are broadcast even when no cell changed. The client places its hardware cursor there only
while that pane has typing focus. The parser retains 4,000 scrollback lines. An exiting process gets a 500 ms output-flush grace period.

A client can park a pane's view above its live screen. It asks for an offset in lines and the
daemon answers with the rows there, the offset it could actually reach, and how far back the buffer
goes; the parked rows are drawn in place of the live grid until the view returns to the bottom. The
rows are read from the emulator's history by line, so the live screen every other watcher is diffed
against never moves. The alternate screen
keeps no scrollback of its own, so a full-screen child answers with a depth of zero rather than
showing the shell's history underneath it.

Damage keeps landing on the live grid the whole time a pane is parked, so returning to the bottom is
immediate and never needs a fresh subscription. The parked rows are deliberately not re-read as that
damage arrives: they are what the operator scrolled up to read, and a pane still producing output
would otherwise shift the text out from under them. Scrolling further is asked by line rather than
by offset: the daemon numbers every line that goes up past the live screen, evicted ones included,
answers a read with the number of its first row, and takes that number back, so a view on a pane
still printing moves from the text on screen rather than from wherever the live screen has got to.
The emulator counts none of this, so the daemon reads it off the history growing: the history may
run past its cap by two reads' worth — a read is at most 8 KiB and a byte scrolls at most one line —
and is trimmed back after each. Lines scrolled on the normal screen by a read that then switches to
the alternate one go uncounted, and a change of width rewraps lines under their numbers; a line
evicted since its number was handed out reads as the oldest one kept. A daemon that numbers no
lines answers without one, and the client asks by offset as before.

A wheel over a pane on the normal screen moves that view rather than reaching the child, since that
is the screen with history behind it. Shift-PageUp and Shift-PageDown page by a screen less a line,
leaving the child its own unshifted paging keys. Typing returns the pane to the live screen: the
child's echo lands there, and the parked view is not somewhere input can be seen. A parked pane
leads its title with how far back it is, because it is otherwise indistinguishable from a quiet one.

Clients receive a full grid when they subscribe, then incremental damage. The grid and the damage
stream are taken under one hold of the parser lock, so no frame can be published between them and
be missed by both. Resize changes both the PTY and parser and emits another full grid.

A connection holds two queues to its socket. Everything a pane sends — its grids, its damage, its
copies and its exit — waits in one bounded queue of 32 frames, in order, since damage ahead of its
grid lands on nothing and an exit ahead of the last frame removes the grid that frame was for. The
subscription's first grid is queued before its stream starts forwarding, so no frame can overtake it.
Everything else — trees, replies, errors, acknowledgements — waits in an unbounded queue that is
written first, so a slow screen never holds up the answer to a request. A frame that finds the pane
queue full is dropped rather than waited for: the pane falls behind, its frames are skipped, and the
first free slot carries a fresh grid with the stream that continues it. A client too slow to keep up
therefore sees the latest screen late, not every screen later and later, and the daemon holds no
more than 32 frames for it. A copy or an exit is never dropped; it waits for room. Falling behind
the pane's own broadcast recovers the same way.

Each client's requested size for each pane it is showing is recorded against its connection, and a
pane's PTY is sized to the request of the client the person was last at — the latest of the views
clients report — as tmux's `window-size latest` does. The client being looked at fills; another
client on the same pane pads or crops until the person goes back to it, whose view then resizes the
pane again. Sizing to the smallest of them all left a desktop window padding a terminal-sized grid.
When no client showing the pane has reported a view, the smallest request in each dimension is used,
so none of them is sent more than it can draw. A client that subscribes without sending a size
makes no request, so one that only watches leaves the pane at the size the others asked for.
Unsubscribing releases that client's claim, and so does disconnecting, so a pane grows back once
whatever was holding it small stops showing it. A pane no client is showing keeps the size it has
rather than reverting to the default. A request that does not change the effective size is not
applied, so a second client agreeing with the first costs no snapshot. On the client side, a pane
leaving the screen forgets its remembered size, so returning to the screen re-sends it.

The client drops a pane's cached grid the moment it stops drawing it, so a subscription it takes
back is never redundant even when the daemon never stopped streaming: only a snapshot can rebuild
a grid, because incremental damage has no rows to land on. Subscription changes are coalesced over
one frame and reduced to the settled selection, so crossing a column of panes costs one full grid
rather than one per pane — but that settled selection is always sent.

The client enables bracketed paste and forwards each paste as one protocol message. On Windows
there is no bracketed paste: a paste, or a file dropped on the terminal, arrives as keystrokes,
so every text key is held until the burst goes idle (under the redraw tick, so it costs no echo)
and a burst longer than a person types is delivered whole as one paste. A paste with nowhere to
land is said on the bar, never replayed as keystrokes: a dropped path typed into the tree is a
run of commands. The daemon
consults the pane parser and wraps the text in bracketed-paste delimiters only when the child has
requested that mode, and then strips both delimiters out of the text first, as xterm and kitty do,
so a pasted end marker cannot turn the rest of the paste into typed commands. A wheel over a pane whose child is on the alternate screen and has not asked
for mouse reporting is sent as a cursor key (xterm alternate-scroll); a child that has asked for
mouse reporting still gets the mouse sequence. Keyboard, paste, mouse, and daemon updates all share
the client's 16 ms redraw tick, so input bursts cannot trigger an unbounded number of full UI renders.

Keys are legacy xterm bytes until the child negotiates better. The emulator tracks the kitty keyboard
flags a child pushes with `CSI > flags u` (and answers the `CSI ? u` query), and the daemon carries
them as `keyboard` beside `mouse` on every pane snapshot and frame, defaulted on the wire so an older
daemon reads as "none". A client types through `keys::encode_with` with the flags of the pane it
types into: at the disambiguate level Escape, modified Enter, Tab and Backspace, and Ctrl or Alt text
become `CSI u` sequences, so an agent CLI can tell Shift+Enter (newline) from Enter (submit); with
report-all-keys even plain ones do. Event types, alternate keys and associated text are tracked but not
acted on, since a client only presses and a `Key` carries no layout or text. modifyOtherKeys is not
tracked: the emulator ignores it, and the CLIs that want modified Enter ask for kitty first. The
terminal client still types legacy bytes.

A child that enabled focus reporting (DECSET 1004) is told when its pane gains or loses the person's
focus: the daemon writes `CSI I` or `CSI O` as the focused view (see "Where the person is working")
moves between panes, goes inactive because its window lost focus, or goes with its client. A child
that did not enable it is told nothing, since it would read the report as typed text.

A selection dragged in a pane is copied two ways at once: to the desktop clipboard, and to the
host terminal as OSC 52, which is the only route out over SSH or through a multiplexer (herdr
forwards it; tmux needs `set-clipboard on`). A child's own OSC 52 copy (Claude Code's `/copy`, a
shell's `osc52` helper) is caught by the pane's parser and sent to the client as
`ServerMsg::Clipboard`, which puts it on the clipboard the same two ways. Ctrl-V reads the desktop clipboard; where there is
none, as over SSH, the terminal's own paste key arrives as a bracketed paste instead. Argus never
asks the terminal to read its clipboard over OSC 52: most terminals refuse, and the reply would
reach the key parser as typed text. Alt-V is the same key: Windows Terminal keeps Ctrl-V and
Ctrl-Shift-V for its own paste, which of an image is nothing, and Alt-V is what Claude Code uses
there for the same reason.

An image crosses as a file, because the pane's process reads the daemon's disk and clipboard,
not the client's. Ctrl-V with a picture on the clipboard, and a paste that is one path to an
image here (what a terminal types when a file is dropped on it), go to the daemon as
`ClientMsg::PasteFile`; it keeps the bytes under its config directory's `pastes/<pane>/`, never
the checkout, and pastes the path spelled the way the host's terminal would drop it — which is
the one form every harness takes an image in. The files go with the pane, and a starting daemon
clears them all. Only to a daemon that listed `PASTE_FILE`; an older one is named on the bar.
None of this reaches a client that is itself running over ssh: there is no clipboard on that
side of the terminal, and a dropped path names the desktop's file, which it does not have. Both
are said on the bar, with the fix — run the client on the desktop, with `--host` — rather than
left for the agent to report as a file it cannot find.

The current pane states are `Idle`, `Working`, `Waiting`, `NeedsReview`, `Done`, `Failed`, and
`Exited { code }`. `NeedsReview` means work is ready for the operator to inspect; `Done` means it
has been reviewed and completed. `Failed` means the agent said something went wrong while still
running, so the row is worth going to rather than worth closing. A pane also carries a `note`: one
line from the agent about the state it is in — the question it is blocked on, or what failed — drawn
as the row's second line so a stalled pane explains itself without being opened. A note is set
alongside a status report and cleared by the next accepted report that carries none, so it can never
outlive the state it explains.
Automatic `Idle` events do not erase `Waiting`, `NeedsReview`, `Done`, or `Failed`; the agent reports
`Working` when it resumes.

Each client compares consecutive tree snapshots by pane ID, through `tree::held`'s one reading of
what changed. The first snapshot a client holds is a quiet baseline, as is a pane new to a snapshot,
and the last one is kept across a reconnect, so a pane that started waiting while the daemon was
away is still news; a later effective-state change into `Waiting`, `NeedsReview`, or `Failed` is announced. Effective
state includes child agents because their parent pane is the selectable place the operator can open.
Every row that stands for several panes — a checkout, a repository and its rail dot, the rail
heading's need count, the command band's tally — rolls up each listed pane and each child the same way, through
`tree`'s one ranking: a failed exit outranks a working agent, and only a clean exit is calm.
Transitions into `Waiting`, `NeedsReview`, or `Failed` also put the pane or child note in the status
bar. A client can optionally ring its terminal bell for those transitions when the pane is not the
active input pane; notifications default to off and are saved in `client.toml`. The desktop window
chimes for the same transitions unless the pane is on screen in a focused window. Its chime is on
by default, switched from the sidebar's foot, and saved in `desktop.json`.

When the client itself runs in a Herdr pane, it reports one aggregate `argus` agent for the open
workspace. Its message names that workspace and groups every live pane by harness, with each pane's
name and status, including child agents under the name `parent / child`. `Waiting`, `NeedsReview`,
and `Failed` map to Herdr's blocked state, `Done` maps to
idle, and `Working` takes precedence over idle; a blocked pane's name and note lead the message so
truncation cannot hide why it needs attention. A newly attached client reports the tree it receives
even when every agent was already running, and releases the report when it detaches. Herdr context
is removed from nested PTY processes so individual harness integrations cannot claim the outer pane.
This aggregate is limited to the open workspace because background workspace summaries carry pane
counts, not individual statuses.

Status is harness-agnostic. A *harness* is a description of how a particular agent CLI can be
asked to report, and there are three mechanisms; a harness may use any combination of them.

Every agent pane is handed `ARGUS_HOOK_URL`, `ARGUS_HOOK_TOKEN`, `ARGUS_PANE`, `ARGUS_HOOK` and
`ARGUS_INSTRUCTIONS`. That is the universal floor: a CLI that can run one command at some point in
its lifecycle can report without Argus knowing anything about its config format. On top of that,
a harness whose hooks live in JSON in the checkout can have Argus write and remove a managed block
itself — `settings` says where the file is, `shape` says how an entry nests (`matcher` for Claude
Code, `flat` otherwise), and `events` maps the harness's own event names onto the statuses Argus
draws. Third, a harness that extends through code rather than through JSON can have Argus write a
plugin module into the checkout and remove it on the same schedule. OpenCode and pi are the built-in
cases: OpenCode has no hook table, while pi exposes lifecycle events through project TypeScript
extensions. Both modules read `ARGUS_HOOK_URL` and `ARGUS_HOOK_TOKEN` at run time rather than having
a pane baked into them. A harness also carries
`resume`, the legacy arguments that continue the last conversation, and `resume_id`, an exact argv
template containing `{session_id}`. Both are used only when a recorded pane is restored. A third,
`first_message`, is the argv that opens a new conversation with a message (`{prompt}`, expanded
without a shell): `SpawnAgent` carries an optional `prompt`, which the desktop composer sends, and
a harness with no `first_message` refuses the start rather than dropping what was typed. Claude
Code, Codex, OpenCode, pi, AGY, Cursor Agent (`agent`) and `generic` are built in. Codex uses a project-local `.codex/hooks.json`
adapter (`SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `SessionEnd`) whose command reads routing from the pane environment, so its content hash stays
stable after the user trusts it. On Windows, Codex runs `commandWindows` through PowerShell, so those strings use `& $env:ARGUS_HOOK` rather than cmd-style `%VAR%` quoting, and build hook URLs with `($env:ARGUS_HOOK_URL + '/status/…')` so Constrained Language mode does not treat `/status` as division. Codex requires the user to trust project hooks before it runs. AGY uses
`.agents/hooks.json` with flat `PreInvocation` and `Stop` hooks. Cursor's `agent` CLI uses `.cursor/hooks.json` with
flat `sessionStart`, `beforeSubmitPrompt`, `preToolUse`, `postToolUse`, `beforeShellExecution`, and `stop` hooks
(schema `version: 1`) plus `.cursor/rules/argus.mdc`. `sessionStart` claims the conversation
(`conversation_id` or `session_id`) without posting `idle`, because that event is fire-and-forget
and can arrive after a tool has already marked the pane working. Tool-start events mark `working`
because the CLI often skips prompt/stop lifecycle hooks; only `stop` clears to `idle`. The helper
answers Cursor tool hooks with `permission: allow` (and Claude's `toolCall` with `decision: allow`)
and stops reading stdin at one JSON object so a runner that waits for stdout without closing the
pipe cannot deadlock the POST. Hook commands bake the helper path and pane URL — Cursor's runner
does not inherit the pane environment, unlike the shell where `argus-hook title` still works. A `[[harness]]`
block in `projects.toml` adds or replaces one, and an `[[agent]]` template selects one with
`harness = "..."`, defaulting to a harness matching its own name. A block cannot supply a plugin,
so replacing a built-in by name also gives up its module and its resume arguments.

Telemetry rides the same mechanisms. `PaneInfo::telemetry` is one harness-neutral record — model,
context tokens and window, cumulative input and output tokens, cost, the running tool, and a tool
count — and every field is optional, because each harness exposes a different subset. Adapters POST
partial reports to `/pane/<id>/telemetry` as JSON; the daemon merges each set field over the last,
treats an empty `tool` as the tool finishing, counts tool starts that carry no count, drops reports
from child sessions, and clears everything but the model when the pane's conversation changes.
Telemetry is live state and is not persisted. The installed hook form derives a report from every
event it reads: `tool_name` and `hook_event_name` (Claude Code, Codex, Cursor), `model` (Codex,
Cursor), and the tail of `transcript_path` — Claude Code's assistant `usage` for context, Codex's
`turn_context` and `token_count` records for model, window, and totals. Claude Code's transcript
holds neither the window nor the cost, so its mod reports both, with the context in use, from
`$.session.usage()` as each step and turn ends. The OpenCode plugin and pi
extension report from their message and tool events. Claude Code tool hooks are answered with `{}`
so the user's permission rules still apply. Clients show the report on the workspace breadcrumb.

A report arrives with every tool call, so it does not travel in a tree. The daemon publishes the
pane's merged record on the tree's channel, and each connection sends it as its client can take it:
`ServerMsg::PaneTelemetry`, the one pane's record, to a client that greeted with `pane-telemetry`,
and the whole tree to any other. A tree is read and a record merged under one lock, and each is
sent before it is let go, so the channel carries them in the order they were read: a tree taken
before a report cannot land after it and wind the numbers back. A connection that falls behind is
sent the tree, which holds them all.

What Argus has to say to an agent on its own account — a review comment's one-line notification —
goes through the agent's harness when the harness can take it, and is typed into its terminal
otherwise. Typing is typing into whatever has the agent's keyboard, and a dialog that has it takes
the text as its answer; a harness's own interface queues a message for the end of the turn and
lets nothing land in a dialog. So a plugin running inside the agent opens `GET /pane/<id>/inbox`,
which the pane API holds open as server-sent events of `argus_protocol::InboxItem`, a message for
the agent, with a heartbeat every fifteen seconds so either side notices the other going. Only the
pane's own session may open it, the newest connection replaces an older one, and it closes the
moment the plugin's end of the socket does, so nothing is handed to a plugin that has gone. An
adapter opens an inbox only once it can act on what it is told, so an open inbox decides the route
alone, in `state/agents/inbox`; `PaneInfo::live` says one is open. opencode's plugin turns a message into
`session.promptAsync`, and pi's extension into `sendUserMessage`, as a follow-up while a turn runs.

Claude Code's adapter is a mod: a plugin of function hooks that runs inside the session (2.1.287
and later). Its source ships in the daemon (`harness/claude-mod`), and `harness/plugin_dir` writes
it under Argus's config directory — only when a file differs, since Claude Code reloads a mod in
every running session when its folder changes — and names that folder in the pane's
`CLAUDE_CODE_PLUGIN_DIRS`, after any folders the template or the daemon's environment already
named. Nothing is written into the checkout, so nothing there needs sweeping. A mod has no
streaming HTTP — `$.http.fetch` answers once the body is read — but reads a child's output as it
comes, so it runs `argus-hook inbox <session>`, which holds the inbox and prints one item a line,
and opens it again whenever it closes, as whatever the session is by then. A message becomes
`$.prompt.submit` as the person's own words, which Claude Code holds until the session is idle.
Claude Code's own dialogs are still answered on its screen. A Claude Code without mods ignores the
variable, opens no inbox, and is typed into as before. The mod's other half is telemetry: it posts
the session's usage after each of the main loop's steps and turns, from a loop the session
started that holds the session's own `$`, since a step's hook is abandoned when its turn is
interrupted and a post made through it would never land.

The daemon's loopback receiver is a small pane API rather than a hook endpoint: `POST
/pane/<id>/status/<working|idle|waiting|needs-review|done|failed>` with an optional body as the note,
`POST /pane/<id>/title`, `POST /pane/<id>/session` with a validated harness session ID, and `POST
`/pane/<id>/checkout` with a directory inside a checkout of the pane's project.
`POST /pane/<id>/comments` requires a live agent source and returns the newest 100 durable review
comments for that pane's checkout as JSON, oldest first. The checkout comes from the pane rather
than request input, so callers cannot select another checkout through this endpoint.
The checkout endpoint changes affiliation only: the agent runs `argus-hook checkout` from the directory it has
already moved to, the innermost checkout holding that directory takes the row, and a worktree made
since the last poll is looked for before the move is refused. The refusal is the answer's body,
and the helper prints it, because a move that failed in silence left agents believing the row had
followed them.
A person moves a pane the other way round (`ClientMsg::MoveToCheckout`, a rail row dropped on
another checkout's row), so the process is moved after the row and the row is never left
claiming a directory its process is not in. An agent's row moves and the agent is told, through its
inbox or typed in, to change into the checkout and run `argus-hook checkout` there; a turn that
still ends in the old directory does not drag the row back, since only a directory that changes
does. A shell is typed a `cd` in its own syntax (`pty/cd`), after the keys that empty its line,
because `rm -rf ` left half-typed would otherwise take the destination as its argument; and only
while it waits at its prompt, since typed text goes to whatever runs in the foreground. On Unix
that is the shell leading the terminal's foreground process group; on Windows the pane's job
holding nothing but its console server and one chain of shells, none under a copy of its own
image, as a Git Bash fork running a native program is. A shell whose syntax is not known, or a
path it cannot be given intact, is refused rather than guessed at.
What each endpoint is posted and answers with is declared once, in `argus_protocol::hook::call`:
`argus-hook` makes its call through the declaration and the daemon answers through it. A body the
endpoint cannot read is refused with a `400` in serde's words, a refusal is a `409` carrying the
reason, and a body past the endpoint's bound (`Endpoint::max_body`) is refused by the helper before
it travels and by the daemon with a `413` naming the bound if it arrives.

An agent links another from a view with a `pane` value, and learns its id from `argus-hook agents`,
which posts to `/pane/<id>/agents` (`agents/roster`) and lists the other agent panes of every project
in the open workspace, the asker's own project first. The window resolves a pane across every project
already, so this is all a link into another project needed.

When the person wants a task kept in a pane they can go back to, an agent starts another agent
through the daemon rather than as a subagent of its own turn. It is an ability, not how worktree
work is done: an agent still works in a worktree itself, or delegates to a subagent, unless asked.
`argus-hook agent [--template T] [--worktree B [--from C] | --in C] (--stdin | <prompt>)` posts an
`AgentLaunch` to `/pane/<id>/agent` (`agents/launch`, `state/agents/launch`). The new agent gets a pane
of its own, as one the person started would, so it outlives the asking turn and the person can watch
and steer it. It is the asker's template unless one is named; it works in the asker's checkout, a new
worktree (made as the window makes one, in the asker's repository or the one `--from` names, with
setup run beside it rather than waited on), or a checkout Argus already shows in any repository. A
checkout Argus does not show is refused rather than added, since what joins the tree is the person's
call. Only a live agent pane may ask, and a project that allows one agent per checkout refuses a launch
into an occupied one, the same as the window. The prompt is required, because an agent with nothing to
do waits at a prompt the asker cannot type into, and it is bounded at 16 KB because it reaches the CLI
as an argument. The answer names the pane, template and checkout, and the helper prints it.

The row also follows without being told, since an agent that has to remember to report a move
mostly does not. The installed hook form carries the event's `cwd` on every report as
`X-Argus-Directory`. When a turn ends — an `idle` report from the pane's own session — in a
directory other than the one the last turn ended in, the daemon moves the row into the innermost
checkout of the agent's repository holding it (`state/agents/follow`), discovering a new worktree the same
way. A tool call's directory is not followed, because an agent that steps into another worktree to
read it has not moved there. Neither is another repository's, nor a directory already followed,
so a row moved by hand is not pulled back by an agent that stayed where it was. Claude Code's `Stop`
names the session's directory, which follows `EnterWorktree` and a `cd` within the checkout; Codex
reports no turn end, so its agents still say they moved. The status is named in the URL rather than the harness's event name, because the
installer already resolved that — which is what makes a new harness config instead of a match arm. Managed
blocks are generally per-boot: they name an ephemeral port and a per-boot token, so they are swept from every
configured checkout at startup and removed when the last agent pane in a checkout goes away, along
with any directory Argus made only to hold them. Moving a pane performs the same cleanup in its old
checkout and installs its harness in the new one. Codex is the exception: its trust-sensitive command
contains only environment references and remains identical across boots. Argus adds every generated
harness and skill file, and `.argus` (the default worktree root), to the repository's local `.git/info/exclude`; this changes no tracked file,
never enters a commit, and is refreshed whenever a checkout is discovered. Hook files are checkout-wide;
the helper uses or rebases to a valid`ARGUS_HOOK_URL`, so each process still routes to its own pane.
So a second agent starting in a checkout leaves the block as the first wrote it, keeping the pane it
names, and a write that would change nothing is skipped: Claude Code stops firing a running
session's hooks once their file changes, which left the first agent's row reading as working after
its turn ended. Cursor, whose runner inherits no pane, is the exception and is pointed at the newest.
The helper reads hook stdin once and can extract both a note and a configured
top-level session ID key. Claude captures`session_id` at SessionStart. OpenCode's plugin tags root
and child reports with their session IDs; only a root claims `/session`, and a newly created root
reports again when it replaces the previous root. Pi's extension claims
`SessionManager.getSessionId()`, reports input and low-level agent starts as working, settled runs as
idle, and blocking extension prompts as waiting. AGY captures`conversationId` at PreInvocation.
Cursor's `agent` CLI captures `conversation_id` or `session_id` at `sessionStart` without moving the
pane to idle.

Every report carries the session it came from, and the pane belongs to one of them. The session that
claims a pane first owns it; a report from any other session — a CLI started from inside the pane,
which inherits the same hook URL and token and cannot be stopped from calling home — is recorded as a
child of that pane instead. Children are listed as indented rows beneath the parent's in the rail,
under an open repository's pane and nowhere else, each marked in its status's colour and writing
the status out only when it needs a person. The
parent's row and header chip show the loudest of its own state and its
children's, so an agent idle at its prompt while a CLI it started works reads as working; the daemon's
own status stays the agent's, since that is what decides when a message can be typed. Children are
not separately selectable: the cursor steps over them, and clicking one selects the pane it runs in,
because a child is something happening inside that pane rather than somewhere else to go. A child can
change nothing about the pane it reports through:
not its title, not its status, not its checkout, and not the conversation it resumes. A new session
ID may take the pane over only while the pane is not working, which is what an agent starting a fresh
conversation looks like and what an agent spawned mid-turn does not; taking over clears the child
list. A child stops being listed three ways: it reports idle, its parent reports idle — the turn that
spawned it is over — or it goes ten minutes without reporting anything, which is the
backstop for one that was killed mid-turn. A background agent outliving its parent's turn is not lost
by the second of those: its next report lists it again. A pane lists at most eight. A report with no
session at all — `argus-hook status` run by hand — is the pane's own voice, as before.

Claude Code runs subagents, and the agents of a dynamic workflow, inside the session, and fires the
session's own hooks for them with an `agent_id`. The installed form reports nothing for such an
event: a subagent's tool calls are not the pane's turn and its numbers are not the pane's telemetry,
and a workflow can run dozens of agents, which listed as rows bury the pane they run in. No
`SubagentStart` or `SubagentStop` entry is written, and the ones an earlier version wrote are
removed on the next install.

A workflow, or a subagent run in the background, outlives the turn that started it: Claude's `Stop`
fires with it still listed as running in `background_tasks`, typed `workflow` or `subagent`. The installed form says so on the report that ends the turn, with
an `X-Argus-Background` header, and the daemon records it as the pane's `background` flag in the
same edit as the status, so no client is shown the turn ending before it is shown what the turn
left running. The pane's own status is idle, because an agent at its prompt can be typed to; its
row and header chip read as working, because
`background` is one of the states they take the loudest of. Every later report from the pane's own
agent restates the flag, so the turn Claude wakes the session for with a `<task-notification>`
prompt — never taken as a title — ends it when its own `Stop` lists nothing. A child cannot set the
flag, and the pane exiting or a new session taking it over clears it. Only agents count as work
left running: a background shell can run as long as the session does.

Agents name their own rows, and the daemon names them first. A prompt-submit
event — Claude `UserPromptSubmit`, Cursor `beforeSubmitPrompt`, AGY
`PreInvocation`, OpenCode `chat.message`, or pi `input` — carries the user's text; the helper
posts it to `/title` the same way an explicit `argus-hook title` does. Tool-start
events are not titles: a working pane named "Shell" says less than the template
already does. An agent can still refine the name once it knows the task; the
next prompt replaces it. Children still cannot rename the parent row.

Agent workflow guidance lives in the bundled `crates/argusd/skills/argus/SKILL.md` and its
`references/`, embedded into the daemon at build time. The skill explains titles, the statuses
hooks cannot infer, and checkout moves; its references cover features, tasks, decisions, and
diagrams, each read only before writing to that part of the board.

Every tool call re-reads the conversation, so startup is designed to cost none. `argus-hook
context` prints review comments and names the checkout's feature, with its tasks and decisions
counted. The brief is left out: it would sit in every request of the session whether or not the
work touches the feature, so `feature` and `task` show it to the agent that asks. Claude's context hook runs `context <instructions>`, and Codex's
`instructions` prints the inherited message followed by the same context, so both agents begin
with it in front of them. The bootstrap tells an agent to load the skill only before reporting
status or changing the board, not to answer a question. Lifecycle hooks still capture session
identity and report their existing events. A stopped turn is not proof of completed work.

Before starting a built-in agent, Argus installs the package in `.claude/skills/argus` for
Claude Code, `.pi/skills/argus` for pi, and `.agents/skills/argus` for Codex, OpenCode, AGY, and Cursor. The latter also gives
harnesses without a native skill loader a file they can read directly. `ARGUS_INSTRUCTIONS`
now holds a short bootstrap pointing at the installed `SKILL.md`. Claude's context event,
Codex's additional SessionStart context hook (including compaction), OpenCode's and pi's system-prompt
adapters, and AGY/Cursor's rules deliver that bootstrap through their existing context surfaces.
Pi discovers both managed files only after its normal project-trust approval.
The Codex context command runs `argus-hook instructions`, which prints the inherited message
without interpreting it as shell code, then the pane's context; the message still prints when the
daemon is unreachable. Its command string is stable
across panes, boots, and changes to skill content; its separate session-identity event keeps its
existing matcher, so compaction does not reset pane status.

A generic or custom harness without `skill_dir` receives compact fallback guidance in
`ARGUS_INSTRUCTIONS`, covering context, titles, status, and checkout isolation. Custom harnesses
can set `skill_dir` to a checkout-relative directory to opt into the same package. Missing,
incomplete, or conflicting packages also use the fallback. A user-owned skill or reference is
never overwritten; a symlinked skill directory or file is left alone. Managed files carry an
ownership marker and are removed with the hooks after the last agent leaves, on checkout moves,
or during startup cleanup; unrelated files remain. After moving, agents resolve the skill's
references in the new checkout. Removing the marker makes a replacement file user-owned.

Titles arriving from a model are flattened to one line and cut to 48 characters. Neither a rename,
status report, nor checkout move can touch a pane that has exited. A renamed row keeps showing its
template on its second line. The bootstrap and the skill direct agents to use linked worktrees for
branch changes, since switching a shared checkout in place changes files and HEAD for every pane
using it. The bootstrap says it because the skill is read only before reporting or touching the
board, which is not when an agent makes a worktree.

## Session restore

The store records each pane's checkout path, kind, title, status, note, and optional harness
session ID and harness name whenever a tree broadcast changes one of them. The daemon remembers the
rows it last wrote, so a broadcast that changes none of them — telemetry, a child agent's status —
writes nothing. The harness is the one the pane actually ran under rather than
whatever its template names now, since that is who wrote the conversation a restore claims; a record
without it falls back to the template. Recording is suppressed while a restore is in flight, so the
panes it is starting do not rewrite the rows it is reading.

On daemon startup:

- linked worktrees are reconciled against Git first, because only primary checkouts come from the
  config and a pane in a worktree would otherwise look like a pane whose checkout is gone;
- editors are skipped;
- panes whose checkout no longer exists are skipped;
- shells start as new default shells; on Windows Argus uses a runnable `SHELL`
  first, then `pwsh`, `powershell`, and finally `cmd.exe`;
- the saved display title, status, and note are reapplied after each pane starts;
- an agent with an ID starts with its harness's `resume_id` argv template expanded to that exact ID;
- every identified pane resumes independently;
- records without an ID retain broad `resume`; one legacy pane per checkout and harness claims it,
  so different template aliases of one harness cannot reopen the same last conversation;
- an agent started that way that exits non-zero within five seconds is taken to have had nothing to
  continue, and is replaced by a plain new agent in the same checkout;
- a missing or broken pane does not abort restoration;
- `ARGUS_NO_RESTORE` starts without restoring panes.

Exact templates are Claude `--resume {session_id}`, Codex `resume {session_id}`, OpenCode and pi
`--session {session_id}`, and AGY `--conversation {session_id}`. Their legacy broad forms are `--continue`, `resume --last`, and
`--continue`; `generic` has neither. A first message is the prompt as the last argument for
Claude, Codex, pi and Cursor Agent, and `--prompt {prompt}` for OpenCode; AGY and `generic` take
none. A `[[harness]]` block sets `resume_id`, `resume`, `first_message`, and an
event-level `session_id` stdin JSON key. Replacing a built-in gives up all of its defaults.

This is relaunch, not process reattachment. PIDs are not stored. Captured IDs are nonempty, bounded,
and control-free, and enter the child command as argv rather than shell interpolation. Exited panes
are omitted. Each recording replaces the pane table in one transaction, because the tree is the
truth and the table follows it: a pane that closed has no row to update.

## Transcripts

The daemon lists a repository's past sessions and reads one's transcript from the files their
harnesses keep, for the desktop window's recent-sessions dialog. Argus never stores a transcript and never
follows one as it grows: `harness/transcript` reads the harness's own file whole, when asked,
into `argus_protocol::agents::transcript::Transcript` — the person's turns, the agent's replies and the tools
it ran, each with the time the harness wrote, plus the session's title (the one the harness gave it,
else the first prompt that is not a slash or shell command), its first and last times, its turn
count and the directory it ran in. Thinking, tool output and the harness's own bookkeeping are left
out. Text is clipped to 8 KiB and a session keeps its newest 500 entries, saying how many older ones
it left out. A line that is not JSON, or a record a reader does not know, is skipped.

A harness's `transcript` names a format and where its files are, as path templates: `~` is the
home directory, `{session_id}` the identity its hooks captured, `{checkout}` the pane's checkout,
`{project}` a whole directory component the harness named after the checkout's path, and `*` any
run of characters within one component; where several files match, the newest wins. Harnesses spell
a path as a directory name each their own way (`C--src-app`, `--src-app--`), so `{project}` matches
a name whose runs of letters and digits are the path's, ignoring case. A session id that holds a
separator, `*`, `:` or `..` names nothing. The built-ins:

| harness | format | where |
| --- | --- | --- |
| `claude` | `claude` | `~/.claude/projects/{project}/{session_id}.jsonl` |
| `codex` | `codex` | `~/.codex/sessions/*/*/*/rollout-*-{session_id}.jsonl` |
| `agent` | `cursor` | `~/.cursor/projects/{project}/agent-transcripts/{session_id}.jsonl`, or in a directory of the same name |
| `pi` | `pi` | `~/.pi/agent/sessions/{project}/*_{session_id}.jsonl` |

OpenCode, AGY and `generic` keep none Argus reads, and asking after one of their sessions answers
`Conversation::NoTranscript`. A transcript a harness names but that is not on disk is `Missing`.

A session of a harness with no `transcript` still lists. Whenever the recorded panes change, each
agent pane with a session id whose harness keeps none is written to `store/seen_sessions` — its
harness, id, checkout and title, when Argus first saw it and when last — and the newest 1,000 are
kept. A listing adds those in the repository's checkouts that no transcript lists, with those two
times as RFC 3339 `started` and `ended` and the last as `modified`. One whose checkout is gone is
placed by the rule a transcript's directory is: under the worktree root or a tool's
`.<tool>/worktrees` in a checkout, it was a worktree of this repository, and is listed under the
primary checkout with `removed_worktree` naming it, dimmed and offering only its transcript like a
transcribed one; anywhere else nothing says it was this repository's, and it is left out. So is one
whose directory is still there but is no checkout, as a resume on the primary would run it where it
never ran. This is what Argus saw, not a
transcript: the preview shows the title, checkout and times and says the harness keeps none.

`ClientMsg::ListSessions` asks for a repository's sessions across all its checkouts, and
`ClientMsg::ReadSession` for one session's `Conversation`; both go only to a daemon that listed the
`sessions` capability, and both are answered from a blocking thread, correlated by request id. A
listing (`harness/transcript/listing`) turns each template's `{session_id}` into a wildcard and
collects every file it names — capped at 20,000 per template — then orders them by when they were
last written and opens only the newest, until it holds `sessions::MAX_SESSIONS` (100). A file
under `{checkout}`, or under a `{project}` named for a checkout, is placed by its path. Any other is
opened to the record saying where it ran: one filed by date (Codex), or one under a `{project}`
whose name only begins with a checkout's or the worktree root's, which is a directory below it. A
directory that is still there is the deepest checkout holding it, compared canonically; one that
is gone is a removed worktree of the repository when it was under the worktree root or a tool's
`.<tool>/worktrees` in a checkout, and is listed under the primary checkout with
`removed_worktree` naming it, for a client to show as past. Only the newest 300 such files are
opened at all, so a repository's Codex sessions older than the last 300 Codex sessions anywhere
are not listed. Cursor's transcripts record no directory, so its sessions from a removed worktree
are not listed. Where the id in a file name is ambiguous (`rollout-<time>-<id>`), the id
the transcript records is taken. Each listed transcript is read whole up to 256 KiB, its turns
counted; a longer one is read only at its first 128 KiB and last 32 KiB, which give its start,
end, title (a title named near the end wins over the first prompt) and directory, and its turn
count is left unknown for `ReadSession` to give. Two harnesses that name the same place, such as a
wrapper configured with `transcript = "claude"`, list its sessions once, under the first.

Each session says which harness ran it and its id — together what `resume_id` reopens — its
checkout, title, times, the file's modification time it is ordered by, and the agent pane running
it now, matched by the session id a hook reported; such a session is listed under the harness the
pane started with. A running agent whose session nothing lists yet — a harness that has not written
its transcript, or one Argus has not recorded — is listed first, with its pane's title.

`ClientMsg::ListResumes` asks how each harness reopens one exact session, and is answered at once
with `ServerMsg::Resumes`: for each harness some agent template runs whose `resume_id` is set, the
first such template's `cmd` with the `resume_id` after it, `{session_id}` left unfilled. A client
fills in the id to name the command before it runs it. `ClientMsg::ResumeSession` starts that same
template on a checkout with the id, answered with `Created` like any spawn; a harness no template
can resume is refused, and so is an id that is not plain (`sessions::is_plain_session_id`: letters,
digits, `-`, `_` and `.`), because on Windows an agent runs through `cmd.exe /C`, which reads `&`, `|`,
`%` and quotes in its arguments as its own. Unlike a restore, a resumed session is the person's own start: it is held to
a project's one-agent rule, and a CLI that refuses the id exits where the person can read why
rather than being replaced by a fresh conversation. Both go only to a daemon that listed
`sessions`.

The desktop window shows them from a clock button on each repository's rail row, or the palette's
"Recent agent sessions of this repository": a dialog of the repository's sessions across every
checkout, live ones first, then the rest newest first, each with its harness, checkout, turn count
and age, those from a removed worktree dimmed. Search and the harness and checkout menus narrow it.
The session under the cursor is read when the cursor lands on it, and its turns, replies and tool
calls fill the preview; a harness that keeps no transcript, or one gone missing, is said in its
place. Enter on a live session goes to its pane. On an ended one it resumes the session in a new
pane on its checkout, by the command the footer names, and the pane is shown only once `Created`
has named it and the tree holds it, so a refused resume leaves the dialog up. A session from a
removed worktree, or from a harness with no `resume_id`, offers only its full transcript.

The full transcript opens in place of the list (`O`, or Enter where there is nothing to resume): the
session read whole as `ReadSession` returned it, the person's turns as a jump list, a find over
what was said and the tools run, Copy session id and Resume. One whose read came back
`NoTranscript` or `Missing` has nothing to open, and says why. A session from a removed worktree is
read through the primary checkout it is listed under: a transcript is found by its id, and a
`{project}` template matches any project directory, so the worktree being gone does not hide it.
A pane resumed from the dialog says in its header when the session was last active and how many
turns it had. The tree does not carry this: the desktop pairs the pane `Created` named in answer to
its own `ResumeSession` with the daemon's listing of the session it asked to resume, which is a
fact about its own request rather than a prediction. Another client, or the window restarted or
reconnected, does not say it; it is forgotten with the pane.

A `[[harness]]` block sets `transcript = "claude"` (a known format where its own harness keeps it —
the form older configs wrote, for a wrapper around that CLI) or `transcript = { format = "…", path =
"…" }`, where `format` is `claude`, `codex`, `cursor`, `pi` or `neutral`. A table without `format`
is neutral, and one without `path` looks where the format's own harness keeps it. An unknown
format, or the neutral one with no `path`, is logged and the block loads without a transcript.

The neutral format is one JSON object per line, each a `transcript::Entry`:

```jsonl
{"kind":"user","text":"Tidy the release notes","at":"2026-10-14T09:00:00Z"}
{"kind":"tool","tool":"shell","summary":"npm run lint","at":"…","failed":true}
{"kind":"agent","text":"The notes are tidied.","at":"…"}
{"kind":"title","text":"Release notes"}
```

`at` (RFC 3339), `summary` and `failed` are optional; a `title` line names the session, the last
one winning; a line of any other `kind` is skipped, as a client skips an entry of a kind it does not
know. It is the serde shape of the protocol's entry, so it cannot drift from what a client is sent,
and what `argus-hook transcript FILE KIND …` appends for a harness that keeps no transcript of its
own: `user`, `agent` and `title` take the text from the arguments or, with `--stdin`, from stdin,
and `tool NAME SUMMARY…` takes `--failed`. Each line is stamped with the time in RFC 3339 UTC,
written in one call so hooks running at once do not interleave, and a leading `~` in `FILE` is the
home directory, for a hook config that runs it with no shell. It talks to no daemon: the file is
the harness's, and its `[[harness]]` block names the same path as its neutral `transcript`.

## Git and checkouts

Read-only Git work uses `git2`. At daemon startup only HEAD is read for each checkout, so the first
client gets branch names without a workdir walk of every repository under a project root. Every two
seconds, on a blocking-pool thread, the daemon then refreshes:

- branch or detached-HEAD state;
- dirty state and changed-file count, including untracked files, with how many paths are staged
  and how many unstaged counted apart — a path staged and then edited again is in both;
- ahead/behind against the tracking branch, and that branch's name;
- the commit HEAD is on;
- for each branch no checkout is on, how many of its commits the main branch lacks;
- linked worktrees added or removed outside Argus.

The first of those ticks is immediate, overlapping session restore, so dirty counts follow by the
time the tree has been on screen a moment.

A worktree that stops being listed leaves the tree, but a pane in it whose process is still running
does not die with it: it moves to the repository's primary checkout (`state/agents/follow`), and an
agent's managed hooks are written there so its reports keep arriving. Removing a worktree is the
usual last step of finishing a branch, and the agent that finished it is usually standing in it,
often having run `git worktree remove` itself; killing its pane ended the session mid-command, with
nothing to tell the user why. The process keeps a working directory that no longer exists, so the
primary checkout is only the nearest row that still is one, where the person can read the pane
and close it. A pane whose process has already exited is retired with the worktree, as before.

The rail's checkout rows and the Checkouts stage draw that state the same way, as parts rather
than a first match: `↑2 ↓1 +3 !4` is two ahead, one behind, three staged, four unstaged, and
`clean` stands in for the last two. A dirty checkout therefore still says it has unpushed commits.
"No upstream" and "in sync" both read as no arrows.

On a slower ten-second beat it also rescans each project root for repositories added or removed
there. Both run on the blocking pool.

Alongside the poll, each repository's Git metadata is watched with `notify`, so a branch switch, a
commit, or a worktree made in a shell reaches clients as it happens rather than up to a tick later.
The watched set is the Git directory itself (HEAD, index, packed-refs) non-recursively plus its
`refs` and `worktrees` trees — never `objects`, which takes thousands of writes per commit for a
change `refs` reports once. Events are coalesced for 150 ms, then run the same reconcile, status,
and branch refresh the poll runs. The watched set is re-derived on the ten-second beat, since
repositories come and go. The poll is not replaced: editing a file touches nothing under `.git`, so
dirty state and changed-file counts still come from the sweep, and a platform where the watch cannot
start logs and leaves the poll on its own.

Edits have a watch of their own, so a review follows an agent within a fraction of a second. Each
checkout with a pane open is watched recursively; the watcher only gathers paths, dropping Git's own
under `.git`, and a burst is read 100 ms after it starts. Of what it gathered, a path counts for the
deepest checkout it is inside and only when not ignored there, each directory's answer kept for the
burst, so a build writing thousands of files under `target` costs one question. The checkouts that
are left have their status read and the tree sent. Past 4096 paths a burst counts for every watched
checkout rather than be held. A checkout with no pane is left to the sweep: nothing works in it, and
on Linux a recursive watch is one inotify watch for each of its directories, ignored ones included.

Status is cached on the checkout rather than read when a tree is snapshotted. A snapshot is taken
under the daemon's one lock, and a keystroke needs that same lock to find the pty it belongs to, so
reading git there put several milliseconds of blocking I/O per checkout in front of the next key —
on every structural change, not just on the poll. The refresh collects paths, reads git with the
lock down, and stores results back by checkout id, dropping any whose checkout moved meanwhile. A
branch switch, a new worktree, and a scan that found new repositories refresh what they changed, so
a row does not name the branch it just left for the rest of the tick; anything changed outside Argus
waits for the poll.

A status that could not be read is unknown, not empty. `git switch` in another terminal rewrites
HEAD, and a poll landing in that window used to report a checkout on no branch, which is how a
detached HEAD reads: the row fell back to the directory the worktree was created as, and the branch
it was really on turned up in the free-branch list, rearranging the column under the user. A failed
read now leaves the cached status alone, and only a genuine detached HEAD reports no branch. A
repository with no commits yet is a settled answer rather than a failure, and reports one too.

The checkouts column is ordered around the repository's main branch, which leads it whether that
is a checkout sitting on the branch or a row offering one. Which branch that is comes from
`origin/HEAD` where the remote has said, and from the conventional names only where nothing ever
set it — so a repository whose trunk is called something else still gets the same treatment.

The repository's other local branches — the ones no checkout is sitting on — are cached on the same
poll but stay out of the column until `B` asks for them: the column is for what is running, and a
repository with forty branches would bury the two checkouts that are the point of it. Reaching a
branch that has no row is what the `b` picker is for. Expanding also shows the branches that exist
on a remote and nowhere here, as `origin/feature`: what the last fetch turned up.

On any branch row, `a` and `s` give it a worktree and start the agent or shell there once the
daemon says which checkout it made, Enter switches the primary checkout to it, `n` gives it a worktree, and `D`
deletes it — `git branch -d` in the primary checkout, so the deletion is local, never pushed, and
refused while the branch holds commits nothing else does. That refusal is the one the user has an
answer to, so it comes back as a second confirmation rather than an alert: an unmerged branch is
reported as such, with up to twenty of the commits no other branch or remote reaches, the popup
reopens asking whether to delete it anyway, and yes reruns the deletion as `git branch -D`. Every other refusal is still an error, because forcing would not have helped
it. The main branch is refused outright, and so is a remote-only row: deleting one of those would
be a push, which nothing in this column does.
A remote-only row answers to the local name it would take, so Enter and `n` name `feature` and let
git take it from `origin/feature` — a worktree for one starts from the remote branch rather than
from this checkout's HEAD.

`F` fetches every remote and prunes, and `P` pulls the selected checkout fast-forward-only; a
branch row runs both in the repository's primary checkout, having none of its own. Both refresh the
tree on the way out rather than waiting for the poll, since a fetch that appears to have done
nothing for two seconds reads as a fetch that failed. A merge that needs a decision is git's to
refuse, and its message is what the user sees.

Branch and file pickers run in process. Branches are local branches, current first. File discovery
uses `ignore`, follows Git ignore rules, and caps the result at 50,000 files.

Git mutations use the `git` executable:

- switch to an existing branch;
- create and switch to a branch;
- add a worktree, creating the branch unless it already exists;
- delete a local branch, refusing an unmerged one until the user confirms the forced deletion;
- fetch every remote, and pull one checkout fast-forward-only;
- force-remove a linked worktree and best-effort delete its branch;
- `git init` a repository that does not exist yet.

A root scan skips `.git`, `.argus`, `node_modules`, and `target` for every project. `exclude` adds to
that and `include` overrides it, both taking either a bare directory name, matched anywhere under
the root, or a root-relative `/`-separated path, matched once; `include` wins over both `exclude`
and the built-in list, so a repository kept where the defaults would never look is still reachable.

A project may set `worktree_root`, which holds one directory per repository, so two repositories in
one project can have a branch of the same name. Its `setup` commands run in a worktree Argus has
just created, in order, parsed into arguments without a shell like the editor command; the row is
broadcast before they run, and a failure is reported without removing the worktree.

A checkout may hold several agents (`CheckoutInfo::shared_by`). That is shown — in the desktop
window, `⚠N` on the rail row and "shared by N" in the Checkouts table — rather than prevented,
unless the project sets `exclusive`, which turns a second fresh agent in one checkout into a refusal
naming the one already there. Shells never count, nor does the marker count an agent that has
exited, and restoring a session is exempt: those panes were already running together.

Argus-created worktrees live under `<primary>/.argus/worktrees/<branch>` by default; a branch name that is not
a plain path component is refused before it becomes a directory, and a name starting with a dash is
refused before it reaches a command line. Removing a worktree decides what git would refuse — a
locked worktree, a path that is not a linked worktree of this repository — before killing the
checkout's panes, because the panes have to die first for the directory to be deletable on Windows
and a refusal afterwards would cost them for nothing. A registration whose directory is already
gone is pruned instead. Unlike a removal made outside Argus, this one still closes the
checkout's live panes: the confirmation names them, and a process left standing in the directory
would keep Windows from deleting it.

Switching a checkout follows Git's rules: uncommitted changes that do not conflict move across the
switch, while a conflicting switch is refused and Git's reason is shown to the user. A worktree is
still available when the user wants the work kept in its own directory. Linked worktrees are Argus's
own and switch under the same Git rules.

## Review

`R`, `Tab`, or leader-Tab requests a checkout review. The daemon computes the diff on a blocking
task with `git2`; the client opens it in a floating overlay while leaving the terminal behind it
subscribed.

Review shows uncommitted work split into the two sides Git itself keeps apart. `b` toggles:

- `unstaged`: the index against the working tree, plus non-ignored untracked content — `git diff`.
- `staged`: `HEAD` against the index — `git diff --cached`.

The chosen side is a setting rather than a per-visit choice, and it survives closing and reopening
the overlay. Both sides are asked for at once, the chosen one first, so one round trip settles
what the review can show and `b` is a look at what is already in hand. An empty side never strands
anyone: the toggle lives inside the overlay, so a side that comes back empty gives way to the other
once it has said, and only when both are empty does the review say so and open nothing. Giving way
is not choosing: the setting stays the side asked for. `b` onto a side known to be empty keeps the
diff already on screen and its side.

A review of working changes follows them without being asked. Each checkout's status carries
`changes`, a fingerprint of every changed path's state, staged blob, and size and time on disk, since
an edit to a file already modified moves no count; when a tree says it moved, the review reads both
sides again. Not over a read still out, which would drop its answer and, under edits faster than a
diff is read, never land one: it catches up when the answer is in. Not under an open comment box
either, whose quoted lines must stay what the comment is about. A read again keeps the person's
place: a side that came back as it was changes nothing, a file it left alone keeps the folds opened
in it, and the cursor and a picked range stay on their lines, found by path and line numbers — the
cursor on a line that is gone goes to the next one in its file, and a range that lost an end is let
go. The Refresh button stays, for a daemon that does not fingerprint.
Both clients hold this exchange as one `review::session`, so the terminal and the desktop window
open, give way and refuse alike. Closing a review or history returns the keys to where it was opened from, the rail or the
Checkouts stage.

A third snapshot is one commit against its first parent, reached through the history overlay rather
than the side toggle. `H` lists the newest 100 commits on the checkout's HEAD — identities only,
which is one revwalk and no diffs at all. Naming the files a commit touched means diffing it
against its parent, so that is asked for one commit at a time, when `l` drills into its row, and
kept while the overlay stays open; `h` folds a commit back up before it closes the overlay.
Drilling into a commit that is already unfolded, or into one of its file rows, asks for that commit
as an ordinary review, so a commit diff is the same viewer, the same navigation, and the same
comment path as uncommitted work, with the comment's anchor recording which commit it was made
against. `h` from a commit review returns to the list it was opened from; escape closes both. An
unborn branch is an empty history rather than an error. Comparing a branch against its fork point,
or against a remembered snapshot, remains out of scope: that is what a Git client is for. A commit
whose diff cannot be read says why in the preview, and is asked for again when the cursor next
lands on one of its files.

Each request captures the index as a tree, and for the unstaged side also an immutable synthetic
tree built from the index plus working-tree deletions, edits, and non-ignored untracked content.
Both are written to Git's object database without changing HEAD, branches, the index, or files.
Diffs use three context lines, preserve old and new line numbers, and detect renames. Binary files,
files over 1 MiB, files over 5,000 rendered lines, and content beyond the review's 20,000-line
budget are listed without their content. Capture and diff failures are reported rather than
rendered as empty work.

Diff lines carry syntax spans, produced by tree-sitter in the daemon. The daemon parses because
the daemon is the side holding whole blobs: the client is sent hunks, and a parser given a bare
hunk reads a fragment torn out of its syntax. Each side is parsed from its own tree, so a removed
line is read in the file it was removed from rather than the one that replaced it. A span carries
what a token *is* — keyword, string, comment, number, type, function, constant, property,
operator, punctuation — never a colour, so the client's theme keeps the palette. Identifiers are
deliberately untagged: colouring every name buries which lines changed.

Ten grammars link in and are chosen by file extension: Rust, TypeScript, TSX (which also serves
JavaScript), Python, C#, CSS, YAML, TOML, JSON, and Markdown. Both TypeScript grammars are
configured with the JavaScript query concatenated underneath their own, which is only its
additions; without it they parse correctly and highlight nothing. Anything else is plain text, and
so is a file over 512 KiB, an unreadable blob, or a parse that fails — highlighting is decoration
and never costs a review. The client validates every offset against the line before slicing it.

`s` flattens the same diff the other way. Unified gives every diff line a row; split pairs a
hunk's removals against the additions that replaced them, one row holding both sides, and ends a
run at each context line because that is where the two sides are known to line up again. Where a
run of one side is longer than the other, the surplus rows leave the far side empty and recessed.
Nothing is asked of the daemon: the rows are rebuilt from the hunks the client already holds. Each
side is ellipsized at its own half so one long line cannot push the other off the row, and each is
numbered from its own tree — the old file on the left, the new one on the right. The cursor stays
on the line it was on across a toggle, a half-made range is dropped, and the choice is a setting
that persists like the side toggle. A row holding both sides anchors a comment to both, which is
the anchor the unified view produces for the same two lines selected together.

The lines a diff did not carry are folds: one before the first hunk, one between each two, and
one after the last, each saying how many lines it hides. The cursor stops on a fold but a fold
takes no comment. `Enter` opens twenty lines of it — up from the first hunk, down from the hunk
above anywhere else — or all of a short one, and `z` opens the rest; the desktop window's fold
also has buttons for both. The lines come from the file's new-side blob, which each shown
`FileDiff` names along with how many lines that side has, rather than from the working tree: the
unstaged side is a snapshot the agent may have edited past since. The daemon reads and highlights
the asked lines of that blob (`ReviewContext`) and sends new numbers only; the client numbers the
old side by the offset the hunks above left, and splices the lines into the hunk they border as
ordinary context, joining two hunks once nothing lies between them and rewriting the `@@`
ranges. So a revealed line is commented on, split and navigated exactly as the diff's own
context is. One fold is read at a time, and lines that arrive after a refresh read the file anew
are dropped. A daemon that names no blob offers no folds, so an older one is never asked. The
history's commit preview draws no folds; the review it opens into does.

Every request has an id and the client accepts only the latest exact reply. Review capture is
globally serialized, with each connection holding one place in that turn and its asks read in the
order it made them, and every ask is answered under its own id, with the diff or with why it could
not be read. One a connection asks again — the same checkout, side and commit — while the first
still waits for its turn is answered by that one capture, which is read after both; any other is
captured on its own, so two commits asked back to back are both answered and a history preview
never waits on an ask that went nowhere. A review of a checkout that is gone is refused the same
way, under its id.

The client supports line and file navigation, single-file range marking, a changed-file fuzzy
picker, refresh, and opening the selected line in an editor. A comment records the review side,
paths, separate old and new ranges, quoted diff text, and body. Who hears it is `review::agent_comment`'s,
the same for a review, a commit's review and the Code stage in either client: a live (never exited)
agent in the checkout. The terminal asks between several each time; the window's box lights the
first when it opens and keeps the one chosen. One that has exited since it was chosen is refused as
no longer running, never swapped for another, and the box stays open for someone else to be
chosen. A body longer than the daemon takes is refused before it is sent. A comment names itself
with a request id and its save names it back, so a save closes the box that sent that comment
alone — a review's, a parked review's or the Code stage's — and lets its lines go; an older
daemon's save names none, and a box waiting takes it. A box sends one comment at a time. A
refused one is said, and the box keeps it to send again; so does a box whose connection dropped
while it waited. The daemon validates that recipient, persists the comment under the checkout
path, then sends a flattened one-line notification to the recipient: through its inbox when one
is open, else typed into its PTY. A failed delivery does not discard the stored comment. Live
agents in the checkout can read the newest 100 comments with `argus-hook comments`.

Added and removed lines are marked by a background wash rather than by foreground colour, which
syntax now owns; the `+` and `-` markers keep their own colour so the signal survives a terminal
that drops backgrounds. Selecting a line brightens its wash instead of replacing it, so a
selected range still shows which side each line was on.

Review is a viewer. There is no vetted state, no stage/unstage/revert action, and no
comment-resolution lifecycle. Closing the review sends no daemon message.

## Code

The Code stage reads a checkout's code rather than its diff, and the daemon answers three requests
for it, each only to a client that listed `CODE`. `ListCode` names every file of one version and
how many lines each changed; `ReadCode` sends one file of it, highlighted as a review is, with the
lines that version added or changed marked, where it removed lines (a count before a line, since a
removed line has no line of its own to be drawn on), and the file's outline; `SearchCode` finds
text across the working tree.

A version is the working tree, the index, `HEAD`, or one commit, and each is read against a base:
the working tree and the index against `HEAD`, a commit against its first parent, and `HEAD`
against nothing, so nothing in it is marked. The working tree is read off disk, untracked files
included, and the rest out of trees Git already holds; nothing is written to the object database.
A path is checked to stay inside the checkout before anything is read. Files over 1 MiB, and
binary ones by Git's NUL heuristic, are named but not drawn. A file Git calls modified whose patch
changes no line — its line endings moved, as on a checkout with autocrlf — is not listed as
changed, and a file read is compared with its base with CRLF folded to LF, so neither marks every
line of an untouched file.

The outline is what the grammar's own tag queries call a definition — functions, types, methods,
modules — in file order, with the grammar's word for each kind shown as it is. It covers the
grammars that ship such a query (Rust, TypeScript and JavaScript, Python, C#); TypeScript's query
is only its additions, so it runs with JavaScript's underneath, as its highlighting does. Which
grammar reads a file is decided once, by extension, for highlighting and outlines alike.

Search keeps ripgrep's rules, with ripgrep's own crates (`regex`, `ignore`) in the daemon rather
than an `rg` that may not be installed: literal text unless the query is asked for as a regular
expression, smart case — case matters only when the query has a capital — unless it is asked to
always matter, anywhere in a line unless only whole words are asked for, over the files
`.gitignore` leaves, narrowed by include globs read as ripgrep's `-g` reads them (`!` excludes).
Binary files and ones over 1 MiB are skipped. It returns every line with a match and every match
on it, in path order, and stops at 500, saying so. A pattern or glob it cannot read is refused,
by the search's id, and said in the search, rather than nothing found.

The stage is the desktop window's; the terminal client has none and asks for none of this. It
opens first on the checkout picked in Checkouts, else the one on screen, and after that comes back
as it was left — checkout, file, cursor, scroll — whatever pane the Workspace has shown since;
only a checkout picked in Checkouts moves it to another. The page keeps what it last drew while
another stage is up rather than being told the stage closed, and remembers each file's scroll and
each list's, which a hidden element would forget. Its tree lists every file of the version, folders closed
until opened, or with **Changed** only the changed files, every folder open; a folder with a change
under it is marked. Reading a file opens the folders above it. A change counts its run of changed
lines once for `]` and `[`, and a place lines were removed counts as one; the same changes are
listed under the outline and marked on a ruler beside the file, each at its place in the whole
file, and the open file's search matches are marked there too. The ruler is the file's scrollbar:
its band is the part on screen, dragged to scroll, and marks and band are measured off the same
rendered rows, so a removal marker or a comment box between lines moves them together.
The outline is put away and brought back with `O`, and a narrow window gives it up before the
file loses its width. The tree's and the outline's edges are dragged to widen them, and a
double-click gives back the usual width; Rust keeps the widths as it does the rail's. Search runs on Enter rather than on every letter — a search reads the whole
working tree — while flipping an option on a search made searches again at once. Its matches are
grouped by file, each file foldable, and reading a match switches the stage to the working tree,
where it was found.

Escape never leaves the stage. It closes what is over the file, one thing at a time — the comment,
the search, the go-to-file list, a picked range — and stops at the file; the stage keys leave, and
`B` goes back to the dialog Code was opened from.

`o` in a review puts the review away and reads the cursor's file in Code at its line, in the
version that line is in: the working tree for unstaged work, the index for staged, the commit for
one commit. A removed line has no number there, so the cursor goes to the line after it. `B`
brings the review back exactly as it was, cursor and folds included, over the stage it was opened
over; it is not read again. `c` in the history does the same with the commit under its cursor,
reading it at that commit, on the file the cursor is on when it is on one.

A comment is made in Code as in a review — `v` picks a range, `c` opens the box — and goes the same
way, as a review comment, stored and then told to an agent in the checkout, refused in the same
words as a review's. Its anchor is the side
of a review the version is: the working tree as unstaged work, the index as staged, a commit as
that commit. It carries the version's line numbers only; the other side's would need a diff, and
an agent reading the comment needs no more than one.

### Editing a file

A working-tree file is shown in an editor rather than read: no key opens it. A commit's or the
index's is read as above. Only a daemon that listed `code-edit` edits, and only a file the stage
could read as text. Vim's keys are on unless Settings turns them off (`vim` in the window's
settings); with them `:w` writes and `]c`/`[c` step between changes. The Vim a window without the
page's editor offers is core's `code/vim` state machine, which holds the whole of the page's
`codemirror-vim` set (search, marks and the jump list, macros, visual block, the `z` and paging
keys, sentence and tag objects, `g?` `gq` `gc`, the ex table with `:s :g :v :normal :sort :set
:map`), with what every editor shares (registers, search, macros, mappings, options) in `Shared`.
It never predicts what the window must do: scrolling, copying and writing are `Effect`s. The keys the stage needs with
the editor holding them are the editor's own: Ctrl P, Ctrl Shift F (the checkout's search; Ctrl F
is the file's), and Alt C, which makes the selection's lines the pick and opens the comment box over
the editor. The gutter marks the lines the working tree changed as Rust last read the file, moved
with what is typed; the file and tree are read again 400ms after writes stop (`Written`), which
keeps the marks, the outline and the tree's counts current without the cursor or a comment moving.
The editor is CodeMirror in the page, and its buffer is the page's, the
one thing in the window Rust does not hold: a key is applied and drawn in the handler that heard it
and never waits on Rust or the daemon.

Its syntax colours come from tree-sitter grammars run as WebAssembly (`web-tree-sitter`), not
CodeMirror's language support, which parsed inside the key's transaction and redrew every visible
line's colours there. The colours drawn move with the text at once; the tree is told each change
(`Tree.edit`) and parsed again from the old one in a task right after the key, which lands before
the frame that paints it, and the query is run over the lines drawn only. A capture is coloured by
its first word, as the daemon colours the read view, plain variables left alone. The grammar only
colours, so Enter copies the line's indentation.

No grammar ships with the window. One is installed when a file needs it, on the person's word:
the bar over the editor names it, where it comes from and its size, with Install and Not now (not
asked again until the window closes). `known` pins 23 languages' npm packages — each one's parser
and highlight queries, the queries of a language that only adds to another's (TypeScript to
JavaScript's, C++ to C's) with both — to a version and the SHA-256 of every file, downloaded from
jsDelivr and refused if a hash differs. Settings → Editor → Install grammars without asking installs
a pinned one when its file opens. For any other extension npm is searched for
`tree-sitter-<language>` (or `@scope/tree-sitter-<language>`) shipping a parser named for it and
`queries/highlights.scm`; nothing vouches for that, so the bar says so, names the publisher, and it
is always asked about. A grammar is kept only once it has loaded and its query compiled, in the
webview's IndexedDB, so an instance downloads each once. `node test/code/registry.mjs` checks every
pin over the network.

CodeMirror lays itself out with a `<style>` element it writes, so the window's content security
policy allows inline styles (`style-src 'self' 'unsafe-inline'`), and Tauri is told not to add its
nonces to that directive, which would make a browser ignore `'unsafe-inline'`. Scripts stay
`'self'`, with `'wasm-unsafe-eval'` for tree-sitter's WebAssembly, and the page may fetch from
jsDelivr and npm's registry, where grammars come from. The screens harness serves the same policy, so a page that only works without it
fails there too.

The file on disk is the authority, and the daemon referees it. `OpenEdit` reads the file, CRLF read
as LF and written back as it was, and watches its folder for writes. A client's `Edit` is written
only when its `base` is the version the daemon holds and the disk still holds that text; otherwise
the write that moved it is sent first as `EditChanged`, and the edit refused with no reason, so
the client makes it again on top. Any other write — an agent's above all — is sent as
`EditChanged` the moment it lands: the daemon waits only until two reads 8ms apart agree, so a
write that truncates first is never sent half done, and narrows the change to the characters that
differ. Positions count UTF-16 units, as the editor does. What is typed is written once typing
pauses for 200ms, or every second while it never does; one write is in flight at a time.

In the page, `sync` holds the disk's text at the daemon's version, the write in flight, and what
was typed since. An agent's change that touches nothing the person has not yet written lands in
the buffer at once, lit for a moment, and what was typed is moved over it, so the cursor stays
where it was and nothing is undone by it. One that touches the same characters is held: the
buffer keeps what was typed, the agent's text is shown beside it, and nothing is written until the
person keeps theirs (written over the agent's) or takes the agent's (their own clashing changes
dropped, the rest kept). Undo never undoes the agent's change. The connection going leaves the
buffer as it is; once back, the file is opened again and what it became meanwhile is folded in the
same way.

## Features and the decision board

Work is scoped to a **feature**: a short document saying what is being built, with the decisions
taken while building it hanging off it. The board used to be one tree per project, which answered
"what has this project ever decided" — a question nobody asks. What an agent picking up work needs
is the handful of choices made about the thing it is about to touch, and everything else on a
project-wide board is noise it reads past. So a decision is filed under a feature, and a board is
read one feature at a time.

The board containing those features defaults to one Git repository. Every linked worktree shares it,
so a feature can move from the primary checkout to a feature worktree and remain readable after that
worktree is removed. The daemon derives a durable key from the repository's shared Git directory
instead of persisting its runtime ids. Synthetic and non-Git checkouts use their repository path.
For work that deliberately crosses repository boundaries, the pane API accepts an explicit workspace scope,
selected by `ARGUS_ARTIFACT_SCOPE=workspace` in `argus-hook`. This scope is independent of the open
workspace in the TUI: opening a workspace changes visibility, not where an artifact is filed. No
client view shows a workspace board, so a change to one is answered to the agent that made it and
pushed to no client: a push names the repositories it is for rather than the board it is, and pushed
under the workspace's repositories it would replace each one's own board on screen.

A feature is stored, not derived: schema v6's `feature` table holds a slug, a title, the document
body, and the checkout and branch it originated in. The slug is derived from the title once, at
creation, and made unique by suffix inside the transaction — a title someone later rewords must not
orphan the decisions under it, and two agents opening the same-sounding feature must not silently
share one scope. Schema v12 merges older branch-keyed boards into their repository board; colliding
slugs receive deterministic numeric suffixes while tasks, decisions and events follow them. The
document is edited rather than appended to as
a tree: it is prose both sides write, bounded at 8 KiB, because a brief that has outgrown a screen
has become the design document it was meant to point at.

A slug is what a write names and a **number** is what a person says: `#7`. Schema v19's
`feature.number` is counted up per board and never handed out twice, so it is not `MAX(number) + 1`
— removing the newest feature would give its number to the next one, and a `#7` said last week
would name something else. `feature_counter` holds each board's high-water mark, and the migration
numbers existing features in the order a board has always listed them, by `at` and then slug.
`argus-hook feature` and `feature list` print it; every command still takes the slug.

An agent adds to a brief a paragraph at a time — `argus-hook feature note` — and replaces one only on
purpose, with `feature brief <slug> "<text>"`, which is how a paragraph it appended in error is taken
back: an agent can undo any change to the board it can make. `feature retitle` renames a feature,
keeping its slug, and `feature drop` removes one with nothing under it — the undoing of `open`. A
feature with tasks, decisions or diagrams under it is refused, since those may be other agents'
work; an agent empties it a task at a time, each of which it can undo, and a person removes a whole
one from the view. A human replaces a brief in the view: `e` on a feature, from either board, opens
the brief in the brief editor and `FeatureAction::Rewrite` writes back whatever it says. It is the same editor a
task's brief gets because it is the same job, prose a person reads and corrects. The editor is
modal: `j`/`k`, `0`/`$` and `g`/`G` navigate, `i`, `a` or `o` start typing, and `Esc` leaves insert
mode and saves; `q` saves and closes. The work a brief might have listed lives in its tasks. The decision view draws the
brief above the tree, bounded to a third of the panel, which is the order `argus-hook feature`
prints them in and the order they are read.

Which feature an agent is on is resolved from the checkout and artifact scope, not from a flag.
Schema v10's `artifact_feature_scope` maps a scope and checkout path to a slug, so repository
and workspace work can select different features in the same checkout. A checkout that was never
pointed anywhere falls back to the one
feature that originated there — which is what makes worktree-per-feature need no ceremony. The
fallback deliberately gives up when a checkout has two features to its name: guessing would file a
decision under whichever happened to be older, which is worse than asking. `decide` from a checkout
on no feature is **refused**, in prose that says how to open one, because a decision nobody can find
again is the pile this scoping exists to end.

Agents reach it through four pane API endpoints. `argus-hook feature` reads the current feature —
its brief, then its decision tree, as one answer, since a decision without what the feature is for
explains half of itself. `argus-hook feature list`, `feature open "<title>"`, `feature use <slug>`
and `feature note "<text>"` are the writes. `argus-hook decisions` reads the same tree alone, and
`argus-hook decide "<chose>" [--over ...] [--because ...] [--under <id>] [--supersedes <id>]`
appends to it, answering with the id the next decision hangs off; `--context`, `--detail`,
repeatable `--consequence` and `--task <id>` say the rest of what a decision holds, `--rejected`
records an answer turned down, and `--open` with repeatable `--option` asks a question instead.
`decisions resolve <id> "<answer>" [--because ...]` answers one, `decisions show <id>` prints one
decision as a document and `decisions rewrite <id> --stdin` reads that document back.
`argus-hook task` reads the current task tree; `task add "<what to do>" [--key <tracker-key>]
[--under <id>]` adds a task, and the other task verbs change its state, title, brief, criteria,
notes, or presence. The view writes the same things with `ClientMsg::Decide`,
`ClientMsg::ChangeDecision` and `ClientMsg::Task`, and after the feature is resolved the two take
one path, so nothing a person can do to the board is closed to an agent or guarded differently.

Every one of those — `feature`, `task`, `decisions`, `decide`, `diagram` — can name the feature it
is about with `--feature <slug>`, and `feature <slug>` alone reads one. The checkout's pointer is
shared by every pane in it, so an agent reaching another feature by moving it moves everyone's; a
named request reads or writes that feature and leaves the pointer where it was. The name travels as
`feature=<slug>` beside the scope in the request, a name that is not a slug never leaves the
helper, and one the board does not have is refused by name rather than read as naming none. A
board read by name is about that feature: its decisions, and `current` naming it.

The decision board never loses a row: there is no delete. A decision that a
later finding invalidates is *superseded*: the replacement is a new row that takes the old one's
place in the tree — its parent, not its children — and the old row's `superseded_by` records what
replaced it. The old node stays on the board and the view draws it dimmed, because the road not
taken is most of what a reader came back for. A decision recorded in error rather than overtaken is
*withdrawn*: `argus-hook decisions withdraw <id>` sets its `withdrawn_at` (schema v17), agents stop
reading it, and the view keeps it, dimmed and marked, so the history still says it was once decided.
`decisions restore <id>` undoes that. Either is refused for a decision not under the feature the
request is about. A withdrawn replacement replaces nothing — what it superseded reads as standing
again until it is restored — and `--supersedes` and `--under` name only a decision on the same
feature, since a number from another feature's board is a mistyped one. Decisions recorded before features existed keep a
NULL `feature` and are reported as unfiled rather than dragged under a feature nobody chose.

A decision is not always made yet. Schema v19 gives one a `state`: `accepted`, which every row
written before it reads as; `open`, a question the work is waiting on, whose `chose` is the
question and whose `options` are the answers being weighed; and `rejected`, an answer turned down
and kept because it is the road not taken. An open decision is the one thing on the board an agent
can get wrong by not having read it — from inside the work a question left for a person looks like
a choice to make — so agents are not spared it the way they are spared a withdrawn one: every read
carries it with its state, `argus-hook decisions` and `feature` mark it `(open)` under a line saying
whose it is to answer, and `argus-hook context`, which otherwise only names the feature, lists the
open questions by id. `DecisionChange::Resolve` answers one: it is refused for anything but a
question still open, the answer becomes the decision's `detail`, the options not taken become what
it was chosen `over` unless it already says, and it is accepted. The skill tells agents to raise a
choice that is the person's as an open decision and to resolve one only on their word; the daemon
does not enforce who answers, for the reason accepting a feature is not gated.

The same schema lets a decision say more than its name — `context`, `detail` and a list of
`consequences`, each a gain or a cost — and name the `task` it was raised under, which must be on
the same feature. That link is what holds a task behind an open question. It is not a foreign key:
removing the task, or moving it to another feature, clears the link and keeps the decision, since
what was decided is still true of the feature it was decided about. `DecisionChange::Rewrite`
replaces a decision's prose and nothing else — not its place, its state, its task or who recorded
it — under the bounds a new decision is held to, and keeps `options` only on a question. It is how
a person corrects what a decision says, as they correct a brief; a decision that turned out wrong
is still superseded, so the board goes on recording what was believed.

`consequences`, `options` and a task's `criteria` are JSON text in a column rather than rows of
their own: each is a short bounded list read whole with its owner and addressed by place, so a
table would only add a position to keep dense.

There is no policy flag on these writes: the board exists for agents to write, never loses a row,
and attributes every one, so there is nothing for a gate to protect.

A feature changes through one vocabulary, `FeatureAction`: the helper posts it to the pane API and
the view sends it in `ClientMsg::Feature`, and both reach one apply path in `state/board/features` that
takes who asked as a parameter. What differs by who asked is in that one match, side by side: an
agent's `open` records the checkout and branch it was cut on and points that checkout at it, where
a person's records neither; `select` and `note` are about an agent's checkout and refused to a
person; an agent's `done` and `reopen` are logged as the agent's, with its session, and carry no
detail, which is the person's to give; an agent's `drop` is refused while anything is filed under
the feature; and transfers and captured notes are the view's.

Every other change to a board is built the same way. A task or a diagram (`state/board/parts`), a
decision added or changed (`state/board/decisions`), the tracker map (`state/board/tracker_map`) and
a turn of a reconcile (`state/board/reconcile`) each have one apply, taking the board's address and,
where a rule depends on it, who asked (`state/board/asker`); the rules that differ by who asked are
arms of its match. Changing a decision has no such rule, so its apply takes no asker. The address is
resolved once per request, from the agent's pane or the client's checkout (`state/board/address`):
the store key, the project, the checkout a feature pointer is kept against, and the repositories a
push names. The `_for_agent` and `_for_client` entry points resolve the address and the feature the
request is about — a client names its feature, an agent's is the one it names or its checkout's —
and call the apply, so a refusal or a push cannot differ by which side asked.

A person starting a feature has a paragraph, not a title, so the view sends a whole note —
`FeatureAction::Capture`, read by `argus_protocol::board::capture` — and `store/capture` writes it in
one transaction: the feature, opened exactly as a person's `FeatureAction::Open` opens one, its tasks
nested in order, and its decisions, a `?` line open with its options and a `!` line accepted with
its reason, each under the task it was indented beneath. A refusal at the last row leaves nothing
behind, not even the number the feature would have had. With `into`, the tasks and decisions are
added to a feature that exists and its title and brief are left alone.

Clients read the selected checkout's repository board — every feature and every decision in
that scope, each with its number, state and everything else above — with `ClientMsg::GetDecisions`. Every client artifact request carries both project and
checkout ids; the daemon validates their relationship and resolves the durable board key. Boards are
pushed with `ServerMsg::Decisions` whenever that scope changes, since a tree is meant to be
watched being built and the daemon deliberately does not track which view a client has open; every
board, task list and diagram list names the repositories in the tree it belongs to — the daemon's
read says so as its push does — and a client reading another drops it. Both clients follow a board
through `argus_protocol::board::follower`: the board is asked for once per repository per
connection, so another checkout of the same repository asks nothing, another repository drops the
board at once rather than drawing the wrong one while its own is fetched, and a reconnect asks again
while keeping what it holds; between those the pushes keep it current. Across a push the cursor
stays on the same feature by slug — among the features the client's index lists, so it never lands
on one the index folds or filters away; the feature open is listed whatever they say — and the same
task or diagram by id, and one that is gone hands it to its parent or to whatever took its place.
The project's name alone is not enough: every repository of a project shares it, and a write to one
repository's board once replaced the board on screen for another. The daemon reads each repository's
board key once and keeps it, so a push costs no repository opens. `DecisionBoard::scoped` is what
narrows that to one feature in the client, so switching scope costs no round trip. The tree is drawn
two lines per decision with branch rails and elbows connecting every child to its parent, and
`argus-hook decisions` draws the same topology for agents. Decisions from before features existed
are reported as unfiled by the daemon, but the feature list offers features only.

### The feature view

The Feature stage has two levels. Its **index** lists the repository's features grouped by how they
stand, with the one under the cursor peeked at beside the list: its brief, the task that comes next,
what is waiting on a person, what is in flight, and the last thing decided. `Enter` opens a feature,
and inside one the same object is read through three **lenses**: `1` work, `2` record, `3` flows.
A breadcrumb over the stage says where that is — repository, feature, lens, and the row under the
cursor with the row it sits under — and `Esc` climbs it one level at a time: out of the detail
column, out of the feature, and from the index back to the workspace. `q` goes to the workspace
from anywhere.

Inside a feature the digits are the lenses, not the views. A lens is where a reader goes next far
more often than another stage is, and the tab strip still answers to a click; from the index the
digits open views as they do everywhere else.

It replaced one document of stacked panels, which had itself replaced three views. A decision
board, a feature board and a task board were the same object drawn three times with a selection
each, and they disagreed about which feature was on screen. One selection fixed that, and still
holds: `feature_sel`, an index into the list every lens is scoped by. What the stacked document
could not do is show how the parts bear on each other — it listed the tasks, then the decisions,
and left the reader to work out which decision was about which task. The lenses are the answer to
that, and they all read one link.

**That link is `Decision::task`**, the task a decision was raised under, and `outline` reads
everything else off it rather than storing any of it:

- the **work** lens draws tasks and the decisions they raised as one outline. A decision already
  made follows the work that carries it out; a question still open leads the work it holds up;
- a task is **bound by** every standing decision raised under it or under a task above it;
- a decision is **carried out by** the tasks under the one it was raised under;
- the tasks beneath a task holding an **open** decision are drawn as waiting on it. The holder is
  not: it is where the question was asked, and answering it is the work.

A sub-decision is raised where its parent was, and a decision under no task closes the outline,
since it binds the feature as a whole. Tasks and decisions are named by their place — `2.2`,
`D2.1` — which is for reading: a write names the row by id, because a place moves when a sibling
is added.

Each lens is a list with the selected row read whole beside it. For a task that is its
description, what done means (`Task::criteria`, checked off with `space` from the detail column),
what binds it, and the notes left on it. For a decision it is a record in the shape of an ADR:
context, the decision, why, what it costs and buys, what was turned down — the `over` text and any
child decision in state `rejected` — and what carries it out. `e` opens that record in the brief
editor as the document `y` copies out (`argus_protocol::board::decisions::adr`), so the sections a
reader sees are the ones a writer fills in, and saving sends `DecisionChange::Rewrite`. An open
decision lists its options instead; `Tab` crosses into them and `Enter` takes one, which sends
`DecisionChange::Resolve`, and `o` answers in words when none fits.

`a`, `s`, `d`, `e` and `x` act on what the cursor is on: a root task, a subtask or sub-decision
under it, a decision raised under it, a rewrite, and a removal. A decision is never removed — `x`
withdraws it and `x` again restores it. A line being typed takes the stage's last row, so what is
being written and what is already there are readable at once; a decision typed there is open when
it starts with `?`, rejected with `~`, and made otherwise, which is the note grammar's marks.

A new feature is written as **one note** rather than a title and then a task at a time
(`argus_protocol::board::capture`). The note is typed on the left and what it reads as is drawn on the
right — the title, the brief, the numbered tasks, the decisions — so what `Ctrl-S` creates is on
screen before it is sent, and a line the grammar cannot place is named instead of dropped.
`FeatureAction::Capture` writes all of it in one transaction. `i` inside a feature writes more
of it the same way, where the first line is a row rather than a title.

The client still predicts nothing. Marking a task done sends the move and draws the mark when the
pushed list says so; the one optimistic write the old view made is gone with it.

The desktop window's Feature stage is the same object read the same way, built in Rust off the
same `outline`, `standing` and `flow` (`board/feature_view` and a module per lens). Its index is a
list grouped Active, Queued, Done — held and archived folded behind a toggle (`z`) that on the
board folds the Archived column — beside a peek, or a board (`V`, remembered per repository in
`desktop.json`) whose columns run Queued, Active, Done, then Archived and Held. A filter by title or
`#number` narrows either; it is not saved, and going to a feature it hides clears it. Which
features are listed, in what order, and where `j`/`k` go are Rust's; the page draws the rows it is
sent. Each card's footer says how it came to stand there and how long since the feature or a
decision on it changed: the board keeps no history of moves, so that is the latest moment it can
name. Only Held and Archived take a dropped card, a hold asking why, and each drop has its key (`H`,
`Shift-A`) and its Move to… item; the peek's After… sets what a feature comes after. Inside a
feature its header names the tickets it is linked to, and `1`, `2`, `3` are the lenses, heard before
the stages' digits. The Record lens lists every decision as the tree it was made in, struck through
once superseded or rejected; `u` supersedes the one under the cursor and `y` copies it as the ADR
`adr` writes. The Flows lens draws the selected flow as an SVG beside its steps, and `F` expands it
over the window with a flow switcher, zoom, Fit and the steps beside it. There a step's arrow
dragged up or down, `J`/`K`, or a lifeline dragged sideways is sent as the flow's source rewritten
by `flow` (`DiagramAction::Rewrite`), and the diagram moves when the pushed list says it did; no
message names a step or a participant, since a flow is only its Mermaid.

### Sequence diagrams

Sequence diagrams are stored as Mermaid source on a feature. The flows lens lists them, reads the
selected one's source as steps (`flow`) — who asks whom for what, with each `else`, `opt` and
`loop` as a line between them — and draws the diagram beside the list. The list is read off the
source because a renderer returns a picture, and a picture cannot be stepped through. A step
names what binds it in a Mermaid comment on its line or the next, `%% decision #4 task #9`: its
row shows each decision's label and each task's, and the selected step's decisions are listed
under the steps with the tasks that carry them out, read off the outline as the work lens reads
them. The binding lives in the source so it is written with the flow and needs no table; the
renderer skips comments. The selected step is lit where the diagram draws it, found by its label
below the step before it, and the picture scrolls to keep it in view. `Enter`
opens the diagram whole in a floating overlay. The client removes the dependency renderer's decorative block-fill glyphs so
`alt`, `opt`, and related frames retain their borders and arrows without filling the panel with
visual noise. The rendered grid is recalculated against the overlay's current inner width after a
resize; when the natural diagram is still wider than the panel, `h`/`l` or the left/right arrows
pan it horizontally, while `j`/`k` scroll vertically. `q` and Escape close the overlay.

A flow is written by an agent (`argus-hook diagram`) or added as a skeleton with `a`. Changing one
is `DiagramAction::Rewrite`, its title and source replaced whole (`argus-hook diagram rewrite`):
`e` opens the source in the editor, since steps, participants and branches are all lines of it
and there is no smaller editor a flow needs. `d` binds the selected step to a decision named as a
reader calls it, `D2.1`, or by `#id`, by writing the comment under the step's message and sending
the flow back; the lens changes when the daemon's list does.

### What a feature row says

A feature's row and its standing are read off what Argus already observes rather than maintained
by anyone:

- `#7`, its number: counted up per board and never reused (schema v19), so a number said aloud
  last week still names the same feature;
- how its tasks stand, as a bar and `3/7`;
- its questions still open, or else how many decisions stand;
- the branch it is being worked on, and the agent pane a person would go to — the loudest on its
  checkouts, coloured when it has stopped for somebody;
- `after #7` for each feature it comes after that is not yet done.

Its **standing** is derived the same way, and is what the index groups by: `ACTIVE` when a task is
under way or an agent is on its checkouts, `QUEUED` when it is written down and not started or
comes after a feature still open, `DONE` when every task is done, and `NEW` on its own page when
there is nothing in it yet. Two are set by a person and folded out of the index until `z`: `HELD`,
which carries its reason, and `ARCHIVED`, which is acceptance.

`Feature` carries `checkouts` and `tasks` for this. The checkouts are every path `feature_scope`
points at plus the one it originated in, so a feature a person wrote down and an agent later picked
up with `feature use` gains a place where work on it can be seen happening; without them the
feature list is an island describing work with no way to tell whether anything is happening to it.
The counts are one grouped query in the daemon rather than every task of every feature on the wire.
Both are pushed with the board, and a task change re-broadcasts the features so the heading over a
list cannot contradict the list.

This replaced five drag-maintained columns. `proposed`, `active`, `blocked` and `submitted` were the
one surface in Argus showing an assertion rather than an observation — a pane's status comes from
its harness, a checkout row from the branch actually occupying the path, a diff from Git, and a
board column from whoever last pressed `H`. `argus-hook` never exposed the move at all, so the only
thing that ever maintained them was a person dragging cards, and a column nobody updates is stale
rather than wrong-but-current, which is worse.

Schema v9 collapses `FeatureState` to `open` and `done` and maps the four old names onto `open`.
`feature_event` is not rewritten — it records what was believed at the time, which is the whole
reason it is a table — so `FeatureState::parse` still reads `submitted` and answers `open`. The
`claimed_by`, `claimed_at`, `blocker` and `evidence` columns are cleared and left in place; nothing
writes them, and the claim they held is what the panes now say directly.

`done` stays stored and stays the human's decision; the index calls it archived, since `DONE` is
what it says of a feature whose tasks are all finished and nobody has accepted yet. `v` archives
the selected feature and brings an archived one back over
`FeatureAction::Done` and `Reopen`; the pushed board is the only state change the client trusts. An agent can
carry the decision out — `argus-hook feature done <slug>`, and `feature reopen <slug>` to take it
back — and the bundled skill says to only on a person's word, never because its own tasks are
finished. `feature_event` records every move with who made it, so one an agent made is logged as the
agent's, with its session, rather than claiming the person moved it themselves.

A feature can be held, and can come after other features (schema v18). A hold is the one thing a
row says that is maintained rather than observed — "until the user answers" shows on no pane and in
no task — so it is never bare: `feature.held` is the reason, NULL meaning not held, and a reader
who sees the reason no longer stands can lift it. That is the difference from the `blocked` column,
which said only that somebody once dragged a card there. What a feature comes after is a
`feature_wait` row per prerequisite, and whether it still waits is read off the prerequisite's
state, so accepting one releases what came after it without anyone remembering to. A wait that
would close a loop is refused, since two features each waiting for the other to be accepted never
will be, and removing a feature clears its waits both ways. A held or waiting feature's title is
drawn dim, and its hold and open prerequisites lead its brief. In the view, `p` asks why and holds
the selected feature, or lifts its hold, and `w` picks a feature it comes after — open features,
and any it already waits on — where confirming one it waits on takes the wait back. An agent uses
`feature hold <slug> "<why>"`, `feature unhold <slug>`, `feature wait <slug> <on>` and `feature
unwait <slug> <on>`; `feature list` prints a hold and the open prerequisites under the row, and
`feature` and the pane context lead with them.

A feature can be linked to tickets in the team's trackers (schema v20): a `feature_link` row per
ticket holds the tracker's name, the key and the URL, in the order they were linked, and nothing
read from the ticket, because Argus never reads it and a copy beside the link would go stale.
`tracker_links` reads the tracker and key out of a Linear, Jira or GitHub URL without going online,
and otherwise takes the host and the last path segment, which `--tracker` and `--key` override; only
an `http` or `https` URL is a link, since the client hands it to the system's opener. Linking a key
the feature has corrects its URL, an unlink names the key in any case, and removing a feature clears
its links. The heading names them on every lens; `t` opens the ticket in the browser, or offers a
choice of several, and `T` takes a pasted URL. Over SSH there is no browser at the desk to start, so
`t` copies the URL instead. An agent uses `feature link <slug> <url>` and `feature unlink <slug>
<key>`, and `feature` lists the links with their URLs. Argus talks to no tracker: an agent reads and
writes the ticket with its own tools, and only what the person asked for and approved.

Each board has a tracker map (schema v21, `tracker_map`): a page of plain words saying what a
feature, a task and a decision answer to in each tracker, which every agent that links or reconciles
reads first. Argus stores it and never parses it. The row holds two texts, because who may write
each differs: the map is the person's, and an agent can only fill the draft beside it, with
`argus-hook tracker-map draft --stdin` over the pane API's `tracker-map` endpoint, which answers a
bare read with the map and any waiting draft. The board carries both to the view. The index's
heading says when a draft waits; `M` opens the draft if there is one, else the map. The map edits
and saves like a brief over `ClientMsg::TrackerMap` with `MapChange::Write`; a draft is never saved
by leaving the editor, only approved with `A` as it then reads, edits included, or turned down with
`D`. Approval carries the text the person read rather than naming the draft, so an agent that
redrafts in between cannot have its newer text approved unseen, and the store refuses an approval or
a turn-down when no draft is waiting.

A link also carries its role (schema v26, `feature_link.role`): a few of the person's words saying
what the ticket is to the feature, `the PM's ticket`, given with `--role` or in the desktop's link
dialog, and rewritten by linking the key again.

A feature is reconciled with its tickets by an agent, never by Argus (`reconcile`, `state/board/reconcile`;
schema v26, `feature_reconcile`). The person asks with `ClientMsg::Reconcile` and
`ReconcileAction::Ask`, naming a live agent in the repository and saying what to bring in and send
out; the daemon tells the agent through `notify`, on one line, to read the map, the feature and the
tickets, write nothing, and propose. The agent posts a `ReconcileChange::Propose` to the pane API's
`reconcile` endpoint (`argus-hook reconcile propose --stdin`): lines, each going into Argus or out to
one tracker, with the exact text it writes and the call it will make, and what it noticed outside
the ask, which is listed and never acted on. Every line starts unticked. `CarryOut` sends the lines
the person ticked in the words they ticked them, perhaps edited; the daemon keeps them as `carrying`
and tells the agent, which reads them with `argus-hook reconcile` and carries out those and nothing
else. Nothing more is ticked or proposed until it reports with `argus-hook reconcile report`; a line
named `--failed` comes back unticked with what the tracker said, and the rest are done with.
`Discard` turns a proposal down or takes back an ask. Reporting records the reconcile as carried out
(`Reconciled`) with how the feature's tasks and decisions stood, as a mark per row of its title or
answer and state; `reconcile::Changes` compares the rows now with it, which is the "N changes since"
a client shows and the dot on each task or decision changed since. Argus keeps the proposal's words
only while it is under way, and keeps nothing read from a tracker after it.

An agent can show the person what it read from a tracker, or from anywhere else it can reach, as a
view. It writes an `AgentView` as JSON — an id, a title, an optional status chip, an optional
subject URL, and blocks, which it translates from what its own tools returned. The blocks are
meant to compose almost anything: text, fields, tables, lists, stats, code, Mermaid and callouts;
containers (sections, columns, tabs, cards, grids, details that fold, rows of values and buttons);
shapes a table flattens badly (charts, progress, timelines, trees, graphs, diffs, matrices,
checklists, logs, boards); and forms. Values add progress, durations, inline code and references
that point into Argus rather than out of it — a path, a commit, a feature, a pane — and runs `argus-hook view show --stdin`, which posts
it to the pane API's `view` endpoint; `view close <id>` takes one down. Argus still talks to no
tracker, and the page draws strings as text, never as markup. The daemon checks every limit in
`AgentView::checked` (256 KiB of JSON, 200 table rows, four levels of nesting, and an `http` or
`https` subject) and answers a refusal with a `409` carrying the block's path, `blocks[2].rows`, in
words the agent can act on; a body past the size limit is refused by `argus-hook` before it is sent, and by the daemon with a `413` before it is parsed. The same check refuses a number past 1e15 in magnitude (a finite one near the top of `f64` hangs a chart's axis), a control character in any id or label the agent is later typed, two actions sharing an id, a path that leaves the checkout (`checkout_path`), and a block or value kind this Argus does not know. A client decoding what the daemon sent keeps such a kind as `Unknown` and says it needs a newer Argus.
What it keeps it stamps with the pane that sent it and the time it arrived, and with the subject
read as a tracker link. Views are held in the daemon's memory only (`state/views`) and never reach
the store, so nothing read from a ticket outlives the agent's next look at it or the daemon's
restart. A pane holds eight: showing an id it already has replaces that view in its place, and a
ninth drops the pane's oldest. A pane's views go with it when it is closed. Only a client that sent
`ClientMsg::WatchViews` is sent them — every view held now, oldest first, then `ViewShown` and
`ViewClosed` as they change. The person closes one with `ClientMsg::CloseView`. A refresh is a request
to the agent, not a read by Argus: `ClientMsg::RefreshView` sends the agent a message, through its
harness's inbox or typed in as a review comment is, asking it to read the source again and show the
view anew with the same id, and the view stays as it is until it does.

A view can be kept. `argus-hook view save --stdin` takes a `SavedViewWrite` — the view, a `scope`, the
`features` it is attached to, a `recipe` (at most 4 KiB, how to build it again) and up to 32
`sources` (`file`, `commit`, `url`, `note`; a `url` is metadata and is never fetched) — and the
daemon stores it as a `SavedView`, under the view's `id`, which is unique across the whole store.
Saving an id again replaces the view in place: there are no versions. `view saved` lists what the
pane's repository and the global scope hold, `view get <id>` prints the stored record in the shape
`save` takes back, `view stale <id>` and `view delete <id>` do what they say. All five go through one
pane API endpoint, `saved-view`, which takes a `SavedViewChange` and answers JSON; the ephemeral
`view` endpoint and `state/views` are untouched, so a shown view still lives and dies with its
agent. The save is checked like a shown view (`AgentView::checked`) and by `SavedViewWrite::checked`
(recipe, source kinds and their own rules, a feature number from 1, at most 16 attached features),
and refused with the limit's words. The store keeps 500 at most; a save of a new id past that is
refused with a reason, and a save over an id already kept always lands.

Scope says where a client offers a view, never who may read it: a person sees them all. `Global`
belongs to no repository, `Repository` to one, `Feature` to one feature of one. A view is filed under
the repository's board key (the string its features are filed under), not its `RepositoryId`, which
is handed out afresh on every start; the daemon names the id when it sends the view, and does not
send a view whose repository is not in the tree (it keeps it, and sends it when the repository
returns). A feature is named by its number on that board, which is never reused. Attachments are
rows of their own (`saved_view_feature`), so a view may be attached to features of several
repositories; a save replaces the attachments of the saver's repository and leaves the rest. Removing
a feature, in the same transaction, drops it from every attachment and moves a view scoped to it up
to its repository. An agent reaches the views of its own repository and the global ones; saving over
an id that belongs to another scope's board, or turning a global view into a repository's or back, is
refused, so one agent cannot overwrite another repository's work by choosing the same name — delete
it first to re-scope.

`built_at` and `built_commit` are the daemon's: the time of the save, and HEAD of the saving pane's
checkout read from Git at that moment (`None` outside a repository). A view is **stale** when the
person or an agent marked it (`MarkSavedViewStale`, `view stale`; kept in the store and cleared by
the next save), or when the saving checkout's HEAD, as the tree last read it, is another commit than
`built_commit`. Only the HEAD move is checked: whether a `file` source changed would be a diff per
view on every status poll, and the head moving is the signal the tree already carries. The check is
a comparison against memory, run beside each tree broadcast and each Git poll; the daemon holds
the rows and, per client-visible view, remembers what it last sent (a revision, the resolved scope
and features, the stale flag), so an unchanged tree sends nothing and the move of one checkout
sends one `SavedViewChanged`. The moved-HEAD flag is derived and never stored.

A client under `SAVED_VIEWS` sends `WatchSavedViews` and is answered with `SavedViews` (every
view), then `SavedViewChanged` and `SavedViewRemoved` as they change; a watcher that lags is sent
the list whole. `RefreshSavedView` types one line to the agent that last saved the view, through its
inbox as a refresh of a shown view is: the id, title, recipe and sources, and the command to save it
again. `PressSavedView` relays a button press to that agent against what the view offers, as
`PressView` does. The pane that saved a view is remembered in the store and forgotten when a daemon
starts, as the other pane ids are, so a view nobody has saved since the restart answers a refresh
with `SavedViewFailed`, a sentence saying so. Every refusal of a request about a saved view
(it is gone, the agent is not running) is a `SavedViewFailed` to the client that asked, and only to a
client that listed the capability; the messages themselves are ignored from one that did not.

Any element can link to a saved view. `Value::View { id, label }` is a reference value beside
`Path`, `Commit`, `Feature` and `Pane`, so every field, table cell, row item and extension table
cell can carry one; the elements drawn from plain text — section, card, details, list item, stat,
tree node, graph node, timeline event, checklist item, board card — take `opens: <id>`, absent from
the JSON when unset. `Block::Embed { view }` draws another saved view's blocks in place; it is a leaf,
the embedded body is never copied into the embedding view, and a client resolves it from the saved
views it holds. A link or embed to a view that is not saved is allowed and drawn as missing. The
check refuses a link whose id would not read back (empty, padded, past 64 bytes, a control
character) and a view embedding its own id; a longer cycle cannot be seen from one view, so a
client follows an embed at most `MAX_EMBED_DEPTH` (3) deep and draws a line instead.

The terminal client draws the Views stage's saved views as a list after the live ones, with the
open one as text under it. The embeds are expanded before anything is drawn, from the saved views
the client holds, so a refresh of an embedded view shows in the views that embed it at once; an
embed of a view already drawn above, a view not saved, or one past the depth is a line saying so.
A link is drawn as its label and a `›`, which is also what marks a section, card, details, list
item, stat or checklist item that opens a view. `l` on an open saved view lists the views it links
to (its links, and the views it embeds); `Enter` opens the one under the cursor and pushes the
view it left onto a trail, which `Backspace` or `h` pops. Choosing a view from the list starts the
trail over. Nothing is predicted: a refresh or a stale mark is a request, and the view changes when
the daemon sends it saved again. The terminal does not draw tree, graph, timeline or board blocks,
so a link in one of them is reached by the link list alone.

The desktop window holds the saved views (`views/saved`) and gives each one a panel, `saved:<id>`
in `desktop.json`, docked on the right the first time it opens and moved like any other; it has no
page on the stage, since its columns want a dock. A panel is a trail of columns side by side,
sharing its width and scrolling sideways once they no longer fit, each one saved view under its own header: the title, the scope, a stale chip,
when it was built and at which commit, and Refresh, Mark stale, Delete (asked first) and Details
(the recipe, the sources and the attached features). Following a link in a column puts its target
in the column after it and drops the columns that were after; a column closes by itself (the first
closes the panel and takes its tab from the docks) or pops out into a panel of its own. A link
followed in an agent's view or an extension's table has no column, so it brings out an open panel
showing the view, else opens it as the first column of one. A view the daemon does not hold is not
opened, and one deleted takes its panel with it. Every `Value::View` and every `opens` draws as
something to press, reachable by keyboard; one whose view is not saved yet is dimmed. `Block::Embed`
draws the held view's blocks in a frame with its title and an Open link, a cycle, the depth cap and a
missing view being a line saying so, as the terminal draws them. A press inside a saved view goes
as `PressSavedView`, and a reference in one is read against the checkout on screen. The sidebar
lists them under Views, grouped by scope, filtered to those about the chosen repository and open
feature or all, a stale one marked. A view another of its scope links to (`views::links`) is listed
under the first that does, folded, so the views saved to be reached from one do not crowd its group;
views that only reach each other are cut at the first. The window says which (`PageSaved::under`)
and the page draws the folds. A saved panel's `✕`, its dock tab's `✕` and a middle click on either
close the panel, not the dock (the tab's as `DockGesture::Close`). A saved panel can also float as a
dialog over the stage (`Place::Float`), moved by its header, resized by its corner and brought in
front when pressed, the docks keeping where and how big in `desktop.json`; its dock menu docks it
again or floats a docked one, and a column's dialog button takes that column out into one. A link
followed inside a saved view opens
its target as a dialog by default, so the view it came from keeps its dock, or brings out the panel
already showing it; the Settings dialog's Views section (`saved_links`) makes it the next column
instead; the Feature stage lists those scoped or attached to the open
feature beside its tickets. Nothing is predicted: refreshing, marking and deleting are requests,
and the column changes when the daemon sends the view saved again or gone.

A view can offer buttons (`ViewAction`) on itself, a card, a row, a table's selected rows, or as a
form's submit. Argus runs nothing for one: `ClientMsg::PressView` (under `VIEW_ACTIONS`) is relayed
to the agent as one line, as a refresh is, naming the action, the selected row keys and a form's
answers as JSON. The daemon relays it only when the held view offers that action id, and keeps only
the row keys of a table that carries it and the answers of the form it submits, so the agent hears
nothing its own view could not have produced. Row keys and answers count once, an answer must be of the kind its field takes and inside its options and range, and text is cut at 4 KiB (`views::press::relay`). A press that cannot be relayed — the view is gone, it no longer offers the action, or its agent is not running — is answered to the client that pressed with `ServerMsg::ViewPressFailed`, so the button does not wait on an agent that was never asked. The pressed button waits until the agent shows the
view again. `ViewChange::Patch` (`argus-hook view patch <id> <path> --stdin`) replaces one block
by the path refusals name blocks by (`views::patch`), and the daemon shows the whole view anew, so a
progress bar or a log tail moves without the view being sent again. A view's `refresh_every` (at
least a minute) has the daemon ask its agent for a refresh that often while the view is held; showing
it again keeps the timer while the interval is unchanged and restarts it when it changed, a patch never touches it, and closing the view or losing its pane stops it. A view's `feature` offers
it from that feature's header too; nothing about it is stored.

The desktop window watches views when the daemon offers them (`AGENT_VIEWS`); the terminal client
watches them too and draws a text subset in a fourth stage, `views` (`4`), beside the rail, with a
muted "open in the desktop" line for the kinds text cannot carry. The views are a panel in a dock (see the docks above), the right one until it is
moved, beside the stage, so the rail and the stage stay in sight and the pane shrinks to fit rather
than being covered. It comes out by itself only when a view comes from the pane on screen and its
dock is not already showing something; otherwise a button in the title strip counts the views
waiting and brings the newest out. The panel has a tab per view, newest first, shows the newest
until one is chosen and the next newest when the one chosen closes, says who read it and how long
ago, and offers Refresh, which waits for the agent to show it again, and Close, which takes it down
for every client. Ctrl-. (in a pane, the leader and `v`) puts the panel away or brings
the newest view out. A pane's row on the rail counts the views its agent has up. When a view is shown
again the window names the blocks whose content changed (`views/agent`): they flash once when the
person is looking at that view, and otherwise stay marked, with a dot on its tab, until it is opened.
What the page alone remembers about a view — a chosen tab, an open details or tree node, a table's
selection, opened rows and filter, a form's draft — is kept by block path across those redraws. Pop
out opens a view as a stage page over whatever stage was up, and Dock, the views panel brought out
or the agent closing it goes back there; a view closed says so by the title its agent last gave it.
While a view is the page it is the one in front of the person: shown again it flashes there, and the
panel does not come out by itself beside it. A reference value opens in the window and never a
browser (`views/refs`): a path in the person's editor, a commit in History, a feature in the Feature
stage, a pane focused; each is read against the pane that showed the view, or the checkout on screen
for an extension's table, and a path that is absolute or climbs out of the checkout stays plain
text. Copy, on a code block or a table as TSV, goes through Rust's clipboard. A feature's ticket
links, in the index's peek and the open feature's heading, open the ticket in the browser, or, when a
view of that ticket waits, carry a dot and open the view instead; the page matches them by URL, or by
tracker and key. An open feature's header is one quiet line (state, progress, its tickets as one button, the brief as one line that opens it for editing); when it was last reconciled and how many of its tasks and decisions changed since is the secondary text of Reconcile in the ⋯ menu, which carries a dot while a proposal waits; the Work lens dots each of those rows.
`T` (or Link…) opens the link dialog: a pasted URL is read as its tracker and key by
`LinkWrite::checked`, as the daemon will read it, with a role beside it; the tickets already linked
open in the browser or unlink, and the repository's tracker map shows under them, with Edit the
map… (`M`) opening its editor on the draft when one waits, where saving approves the draft as it
reads or writes the map. The fields clear only once the pushed board holds the link. `t` opens the
one ticket, or the dialog to choose among several. `R` (or Reconcile…) opens the reconcile dialog:
with nothing under way it asks a live agent of the repository, the one that did the last one by
default; with a proposal it lists the lines under Into Argus and Out to each tracker, every one
unticked, with the line under the cursor in full — its exact text, editable before ticking (`e`),
and the call the agent will make — and what was noticed outside the ask, which nothing ticks. Space
ticks, Enter carries out the ticked lines in their words, `D` discards. The ticks are the dialog's
until the board says the lines are being carried out, and a new proposal starts them again.

A diagram block is read in Rust, as the Flows lens reads a flow (`flow::rows` and
`flow::participants`), and drawn by the page as an SVG sequence diagram, its source a disclosure
away. Every string is drawn as text, Markdown included; a link opens through `system/opener` in the
system's browser and only when it is `http` or `https`.

`m` transfers the selected feature's active checkout association to another checkout in the same
repository over `FeatureAction::Transfer`, sent on the source checkout's board. The daemon validates both runtime ids, removes the
source association and writes the destination in one store transaction. This changes where work is
happening, not where it began: `origin_checkout` and `origin_branch` remain historical. Removing a
worktree removes neither the repository-owned feature nor its accepted history.

A feature is opened with `a`, renamed with `R`, and removed with `x` — the list is where a person
writes down work, so it cannot be a surface only agents can add to. A feature opened here records
no origin checkout, because work a person wrote down has not been cut anywhere yet; whichever agent
picks it up says so with `argus-hook feature use`, and that is when it gains a home. A rename
changes the title and freezes the slug: every decision row, task row and `feature_scope` entry
points at the slug, so re-deriving it from the new title would orphan exactly the work the feature
is about. Removing one takes its tasks, events and checkout scope with it and **unfiles its
decisions** rather than destroying them — the board is append-only because it records what was
believed at the time, and that outlives the feature it was believed about. The torn-off decisions
survive as unfiled and stay readable through `argus-hook decisions`, but they are not a feature and
the feature pane draws no row for them.

### Tasks

A feature says what is being built and its decision tree says why it is being built that way.
Neither says what is *left* to do, which is what a human actually hands an agent. So a feature
carries a list of tasks, drawn as the middle panel of the feature view.

A task is a row, not a checkbox line in the feature's document. Its title stays a compact line, and
an optional multiline brief carries context, boundaries, acceptance criteria and verification when
the title is not enough. The selected row expands to show that brief; Enter opens it in the same
multiline editor used for feature briefs, while `e` keeps the fast title editor. A checkbox line in
a document would be addressed by line number, and a line number moves whenever the text around it
is edited — which is the one thing a list cannot take, since a row has to stay the same row while a
human rewrites the list. Schema v8's `task` holds the title, the state, the
claim, a `position` and an `external` key; schema v11 adds its optional body, schema v15 adds
its nullable `parent` task id, and schema v19 adds its criteria, its notes and the pane holding its
claim. `position` is per sibling list, so moving a child never reorders its
parent's other children or the feature's root tasks.

The states are still todo, doing and done, and unlike the feature columns they sat beside they are
maintained by whoever is doing the work: an agent takes a task up and finishes it as part of the
job, so `doing` says which row somebody is actually on rather than which was last dragged. They are
drawn as a mark on the row — `○`, `▶`, `✓` — rather than as three columns, which is what lets the
list keep the order a person put it in. That order is most of what a list is for, and three columns
spent it saying what a glyph already says. Distinct shapes rather than colour alone, since a state
you can only see by telling two greys apart is a state half the readers cannot see.

`external` is whatever key the team's tracker uses. Argus stores it and knows nothing else about it:
an agent with access to Jira, Linear, GitHub Issues or a spreadsheet is what puts tasks here, which
is why Argus works the same with any of them and needs credentials for none. `argus-hook task add
"<what to do>" --key ORION-412` is the whole of the integration. When work reveals more work,
`argus-hook task add "<child>" --under <id>` keeps that discovery beneath the task whose id `task`
printed.

Both sides write, and here they write the same things — there is no acceptance step and so no move
either side is refused. That ceremony belongs to the feature the tasks are under, which is where a
human accepts the work as a whole. An agent reads with `argus-hook task` and writes with `task add`,
optionally using `--under <id>`, `task doing <id>`, `task done <id>`, `task todo <id>`, `task retitle
<id> <text>`, `task brief <id> <text>` and `task drop <id>`; the states are named as verbs rather than
hidden behind a state argument, so what an agent types is what a reader of the transcript understands
happened. Taking a task up is what claims it and finishing it is what releases it, so a row always
says who is on it without anyone claiming by hand. The claim is the harness session, which tells a
reader who, and since schema v19 the pane it was taken up through, which tells them where to look:
the pane API knows which pane is asking, so a task moved to `doing` through one carries it and one
moved from the view carries neither. Pane ids are handed out again from one on every start, so the
store forgets them when the daemon opens it and keeps the session.

A task's **criteria** are what done means for it, each a line checked off as it lands: `task
criterion add <id> "<text>"`, and `criterion check`, `uncheck` or `drop` with `<id> <n>`, where `n`
is the number `task` prints, counted from one for the person reading and sent as a place in the
list. A place that is not there is refused in words. Its **notes** are lines left by whoever is on
it — `task note <id> "<text>"` — each recording the session and pane it came through, or neither
from the view. A task keeps its newest forty, in a `task_note` table rather than a column because
they are appended one at a time, possibly by several agents, and a column would make every note a
rewrite of the others. `argus-hook task` prints the criteria with their marks and the last three
notes; removing a task removes its notes.

`task move <id>` changes where a task sits, never its state: `--under <id>`, `--top`, `--before
<id>` or `--after <id>` within its feature, or `--to <feature>` onto another feature on the same
board, where it arrives at the top level. `TaskAction::Place` names the place against another task
rather than as an index, so the daemon, which holds the tree, works out the numbers. The subtasks go
with it, a task placed beside another takes that one's parent, and a place inside its own subtree is
refused. Both sibling lists it touches are renumbered dense, and a move to another feature pushes
that feature's list as well as the one it left.

Ids are database-wide and an agent numbers its tasks from what it last read, so every change naming
a task or a diagram is refused when the row is not under the feature the request lands in — the one
the agent's checkout is on, or the one the client names: a stale id would otherwise let one
feature's agent tick off another's work by arithmetic. Both sides and both parts take that one path
(`state/board/parts`), so the guard cannot differ by who asked.

From the view, `H`/`L` move a task along todo → doing → done and `J`/`K` move it earlier or later among
its siblings. The order is usually a human's statement of what to do first, and the skill tells
agents to leave one a person arranged; they can still set it, so a task an agent put in the wrong
place is one it can put back. `>` puts the selected task under the sibling above it and `<` lifts it
out to sit after its parent, and `m` in the tasks moves it, with its subtasks, to another open
feature. `s` starts a line for a subtask under the selected task; `a` starts a
root task. Both are refused unless the tasks have the keys, so a capital `H` on the feature list does
not move a task the cursor is nowhere near. Lists are pushed whole on `ServerMsg::Tasks` whenever one
changes, and the wire stays flat: every row carries its parent id while the client and hook project
the rows into depth-first branches. The cursor is held by task id across the push rather than by row
number: a card you have to go looking for reads as having been lost. Removing a task removes its
descendants with it, so a discovered subtree cannot become orphaned work.

Adding and rewriting type on one `LineInput`, shared with the feature list because adding a feature,
a root task, and a subtask are the same gesture on the same kind of surface and two of them would
drift. It
takes a row off the bottom of the view rather than floating over it, so what you are writing and
what is already there stay readable together, and while it is up it swallows every key: a title with
an `x` in it does not delete the row behind it, and the first `Esc` puts the line away rather than
the view. The prompt names itself — `new task`, `rewrite`, `new feature`, `rename` — which is the
whole of the affordance, since there is no other cue that the keys have changed meaning.

### Limits of the current model

A feature is still both a context container and a unit of work. Its selection is shared by every
pane in a checkout, and a checkout with one originating feature selects it without an explicit
assignment. The managed skill asks agents to read that feature, its decisions, and its tasks, but
the daemon does not decide whether they apply to the user's current request. Agents pull these reads
at startup or task changes; unlike clients, they receive no board updates while running.

The stored briefs, decisions, tasks, attribution, and supersession are useful durable material, and
retiring the Kanban states removed the second tracker an operator had to maintain. What is still
missing is the rest of TARGET.md, "Agent context and memory": work contexts that a checkout may
suggest but never silently assign, typed artifacts beyond decisions and tasks — findings,
assumptions, open questions, summaries — and a bounded context packet that says why each durable
item was included. Until those land, an agent still infers its work from whichever feature its
checkout is on.

## Browsers

An agent pane can have a browser. `argus-hook browser` posts a `BrowserRequest` to the pane API's
`browser` endpoint; the daemon starts Chrome, Chromium or Edge (`ARGUS_BROWSER` names another)
headless, on a fresh profile under the temporary directory's `argus-browsers-<instance>`, which the
next daemon of the instance clears should this one be killed outright. The browser listens on no
port: it is handed two anonymous pipes (fds 3 and 4 on Unix, `--remote-debugging-io-pipes` on
Windows), so no other process on the machine can drive it. The daemon answers the URL of the pane
API's `devtools` endpoint, `ws://127.0.0.1:<port>/pane/<id>/devtools?token=<token>`, the hook token
in the query since a browser tool given a URL sets no header. The agent drives it with whatever tool
it brings — `agent-browser --cdp <url>`, a DevTools MCP server, Playwright — so Argus never chooses
how an agent browses, and a second open answers the same URL. An agent with no tool of its own uses
the built-in commands instead (`BrowserRequest::Act`, `browser::act`): `go`, `back`, `reload`, `new`,
`tabs`, `text`, `click`, `type`, `eval` and `shot`, each a few DevTools calls on the daemon's own
session to the tab shown, a load waited for off the follow loop so the pictures keep coming. They
exist because an agent left to write its own client opened a tab for every page; the skill points
it to `agent-browser` first and to these second. Those that move the page are refused while the
person holds the browser, as the same moves through the mux are. `browser close`, the person's close
(`ClientMsg::CloseBrowser`), the browser pane or its agent's leaving the tree (`retire_pane`) and
the daemon stopping each kill it; on Windows a job takes its renderers with it.

A browser is a pane of its own: `PaneKind::Browser`, in the checkout of the agent that opened it,
with its own `PaneId`, so it tiles, fills, splits and closes in the Workspace grid as a shell does.
It is owned by that agent, one each: `BrowserPage::owner` names the agent, and every other browser
message (`BrowserFrame`, `BrowserInput`, `CloseBrowser`, `BrowserClosed`) names the browser pane. The
DevTools URL stays the agent's, since the agent's token and identity are what the pane API knows.
Its content is pictures, not a terminal, so its process (`browser::process`) is inert: keys, sizes
and scrollback come to nothing, and the person's hand arrives as `BrowserInput`. Closing the pane
closes the browser, a browser that ends on its own takes its pane out of the tree, and closing the
agent's pane closes the browser's. The pane is titled for the page it shows, `Browser` with none.
`CheckoutInfo::listed_panes` leaves browsers out, as it does editors: the sidebar lists a browser
only in its Browsers section, not under its checkout. A browser is never restored after a daemon
restart (`session_panes` does not record it, `restorable` skips it): the agent opens it again. A
client that did not list `BROWSERS` cannot decode `PaneKind::Browser`, and with it the whole tree,
so the Tree topic strips browser panes from every tree it sends such a client; the tree joined
before the greeting is sent again after it, once the client is known to take them.

A pipe is one DevTools connection, and `browser::mux` shares it: the daemon's own view and each
agent's socket are clients whose command ids it rewrites so each answer finds its way back, a
session's events go to the client that attached it (or every agent, when the browser attached it
itself), and the rest to everyone. Sitting there, it sees which tab an agent acts on and whether
the act moves the page, and tells the daemon's view as an `Argus.agentActed` event. While the person
holds the browser (`BrowserInput::Hold`) it answers an agent's input and navigation with an error
that says so; reading the page is still the agent's.

What the person sees is read by the daemon, not the window, so a window on another host sees the
daemon's browser and the terminal client never has to. `browser::screen` keeps the browser's tabs
and attaches its own session to one: the one the agent last acted on, or failing that the last
opened, since an agent that opens a tab is about to work in it — unless the person picked another
(`BrowserInput::ShowTab`), which is shown until it closes or they follow the agent again
(`FollowAgent`). The blank tab Chrome starts with is closed when another opens before anything has
taken it anywhere, since a tool that opens a tab of its own would leave it behind; a built-in `new`
while it is the only tab goes there instead. It asks that tab for a screencast: JPEG frames, each acknowledged before the next
comes, so a slow reader throttles the browser rather than queueing behind it. The tabs' addresses
and titles come from the targets' events and from reading the shown one's again once it loads,
since Chrome does not report a title its document sets. Who moved the page in the last three
seconds, the agent or the person, is its `driver`. A view drawn large sends
`BrowserInput::Viewport`, its room in CSS pixels and the screen's density; the tab is laid out at
that size (`Emulation.setDeviceMetricsOverride`) and its frames asked for at that many device
pixels, so they are drawn one to one. A headless screencast is drawn at the browser's own density
whatever density is emulated, so Chrome is started at a scale of 2. A client that lists `BROWSERS`
sends `ClientMsg::WatchBrowsers` with the browsers it draws, and again whenever that changes; it is
sent every browser's `BrowserPage`, and the frames only of those it draws, so a browser no window
looks at costs no connection anything. The channel is shallow, and a client that falls behind is
sent each browser's latest picture rather than the backlog. The frame carries Chrome's base64 as it
came, so neither side decodes it on the way.

A browser pane tiles in the Workspace's grid like a shell or an agent, with the same fill, split and
close, and the sidebar lists it only in its Browsers section, whose row puts that pane on screen
(`Call::ShowBrowsers { pane: Some(_) }`, as a click on a pane in the rail does). When a browser opens
while its agent's pane is the one the Workspace shows, the window opens the browser beside it, as
`OpenBeside` would; the browser's page can arrive before the tree that holds its pane, so the open
waits for the tree. A browser opened for an agent the person is not looking at is only listed. The
window asks for the pictures of the browsers the grid holds while the Workspace is up, and of every
browser while the Browsers stage is. The Browsers stage (`6`, `Call::ShowBrowsers { pane: None }`) is
the overview: every browser as a live tile, named by its owner's title and who has it. A cell that
draws a browser has its tabs, Follow agent while the person has picked a tab, Take control or Hand
back, an address bar, back, forward, reload and close. Keys, pastes and mouse bytes never go to a
browser pane as pty input (`Session::typed_to`): the page sends it `BrowserInput` itself.
Frames reach the page past the session (`Shared::browser_shot`), since they come many a second and
change nothing else; the page decodes each into a bitmap and paints its canvas, because its CSP
admits no `data:` image. A click, wheel or key on a browser's cell is sent as a
`ClientMsg::BrowserInput` in the page's CSS pixels, which `browser::input` turns into DevTools input
on the tab shown; a navigation is refused unless it is `http`, `https` or `about:blank`.

## Extensions

An extension is a program outside Argus that extends the interface: a tab of dev servers or Docker
containers, a key that restarts the selected one, a deploy command that asks which environment
first. The daemon owns extensions and the clients only draw them. Under `argus --host` only the
daemon there is beside the containers, every client draws the same tab, and the client never
predicts: a tab changes only when its extension publishes it again.

```text
extension ──HTTP (pane API)──▶ argusd ──protocol──▶ argus-desktop / argus
    ▲                             │                        │
    └──── events (SSE) ◀──────────┴──── ViewAction ◀───────┘
```

`projects.toml`'s `[[extension]]` blocks name each one and its `command`, run without a shell in
its `dir` (Argus's config directory by default) with its `env` added. The daemon starts every one
after its hook server binds (`state/extensions/running`), handing it `ARGUS_HOOK_URL`
(`http://127.0.0.1:<port>/extension/<name>`), `ARGUS_HOOK_TOKEN` and `ARGUS_EXTENSION`; its output
goes to `extension-<name>.log` beside `argusd.log`. One that exits is started again after a wait
that doubles from a second to a minute, back to a second once one has run a minute, and all are
ended when the daemon stops. On Windows each runs in a job object the daemon holds, so a daemon
killed outright takes its extensions with it; elsewhere an extension notices its event stream is
gone, as `scripts/docker-extension.py` does, and exits. The extension's token is its own, minted when it starts and bound to
its name, so only an extension the daemon started may publish, and one cannot speak as another; a
pane's per-boot token is refused there. While one is down, everything it published is taken off
every client: its tabs, its keys and commands, and its open dialogs. The blocks are read at start;
a change to them takes a daemon restart.

Everything an extension says is declarative, in `argus_protocol::extensions`' types as JSON:

- `PUT /extension/<name>/view` — one view or a list, replacing every view it had, or none of them
  when one breaks a limit (`ExtView::checked`, refused with a `409` naming the path,
  `views[1].rows[3][0]`). A view is a named table in the agent views' vocabulary — `Column`s, and
  cells that are `Value`s, held to the limits an agent's table keeps (200 rows, web links only) — so
  the window draws it with the same renderers; and what only an extension adds: a key per row, a
  status per row as a `Chip`, and actions, each per-row or view-wide, with an optional
  one-character key and an optional question asked first. A row's status is a chip rather than the
  pane API's status words because those say where an agent's turn is, and an extension's rows are
  containers and servers whose states only it can name; it picks the word and a `Tone` for what it
  means. Its scope is global, a repository's or a checkout's (by the daemon's ids, which a
  command's context carries). `place` (`panel` unless it says `stage`) is where a window first
  opens it, docked or as a page, and only that: the person moves it between the two. `icon` names
  one of `EXT_ICONS` for its sidebar entry, a name the window does not draw being drawn as a
  generic icon; an extension never sends an image.
- `PUT /extension/<name>/commands` — its commands, each with a title, an optional key and where
  the key is heard: anywhere (and through the leader from a pane), in one of its views, on a
  checkout row, or on a pane row.
- `POST /extension/<name>/dialog` — a confirm, a one-line prompt, a pick from a list or a small
  form, answered with its id.
- `POST /extension/<name>/notice` — one line for the status bar.
- `POST /extension/<name>/pane` — a shell on a checkout, or a console when it names none, with a
  command typed into it, answered with the pane's id: the same spawn a client asks for.
- `GET /extension/<name>/events` — held open as server-sent events with the inbox's heartbeat:
  each action (`{event: "action", view, action, row}`), each command run (`{event: "command",
  command, id, context}`, the context being the client's view read against the daemon's tree as
  `argus focus` prints it), and each dialog's answer or cancellation. The newest connection takes
  over, as a pane's inbox does, and an action sent while none is open is refused to the person in
  words rather than lost.
- `PUT /extension/<name>/about` — what it is: a one-line summary and a Markdown guide to using it.

What an extension says it is, with the optional one-line `help` of each command and action, is for
whoever reads it, a person or an agent, and not for a window to draw: the daemon holds it in memory
until the extension goes and sends it to no client unasked. `argus extension list` and
`argus extension show` ask for it with `DescribeExtensions`, which a daemon that listed
`describe-extensions` answers with every running extension, its views without their rows, to that
client alone; so they read the daemon there under `--host`, as the window does.

An extension's key never shadows a key Argus binds, the leader, or the digits that open the stages,
so the shortcuts sheet stays the one account of what a key does. The daemon decides when the key is
registered and knows no client's keymap, so the keys the window keeps where a command would be
heard are listed in `argus_protocol::extensions::keys`, and the desktop's tests fail when it binds a
key that list does not hold. A conflicting key — Argus's, another extension's, or one of the
extension's own view's actions' — is refused: the command or action is kept without it, the answer
to the put names each refusal and why, and every client's status bar says so. An action's key is
one character the view's own stage does not keep for moving and filtering.

The protocol carries it to a client that listed `extensions` in its greeting, and to no other:
`ExtensionViews` and `ExtensionCommands` whole after the daemon's greeting and on each change, `ExtensionNotice`
for a notice; the client sends `ExtensionAction` and `ExtensionCommand`, which the daemon checks against what is
published before forwarding (`ExtView::offers`, which the window reads as well, to decide what an
action sends and whether it asks first) — a client a publish behind cannot act on what has been
taken away. A dialog goes to one client, so only one window asks: the one that ran the command it
answers, when it names one (`answers`, the command's id), else the client the person was last at
among those that listed `extension-dialogs`. Only that client may answer it, in its own kind
(`ExtensionDialogAnswer`), and a dialog nobody answered is withdrawn, its extension told it was
cancelled, when that client goes.

The desktop window lists every extension view whose scope holds — a repository's while the rail
reads that repository, as the Checkouts stage does, a checkout's while its pane is on screen or it
is that repository's own — under Extensions, a fold in the sidebar after the built-in stages, each
with its title and icon (`extensions/views`). Nothing listed, the fold is not drawn, as the rail's
fold of quiet repositories is not; whether it is unfolded is kept in `desktop.json`. Each view is
a panel of its own (see the docks): it opens in its dock, or as a page filling the stage
(`Stage::Extension`), wherever its extension's `place` (`left`, `right` — or `panel`, the same —
`bottom` or `stage`) says the first time and wherever the person put it after: a docked view's
header offers an Open as page icon, a ⋯ menu (the extension's view-wide actions and every row action's key) and its dock menu, a page's Dock; a row's own actions, text without key caps, show only on the selected or hovered row, its status a dot and a quiet word; the console's strip likewise keeps one ⋯ menu for split, move and fullscreen, also the keymap's `ExtensionPage` and
`ExtensionDock` (`>` and `<` while the view has the keys, from the palette on the view last used),
and the docks keep the choice in `desktop.json`, across its extension republishing or restarting.
Docking goes back to the stage the page opened over. A view has no digit, the digits being the
built-in stages': it is reached from the sidebar, its Tab and Enter included, from the palette's Go
to, and the shortcuts sheet says so under its extension's heading. Docked beside the agents' views
it is another tab of their dock, and an agent's view arriving never takes the dock from it; in a
dock of its own both are on screen, and a page beside views docked elsewhere too, each keeping its
own selected row. Either way it is drawn by the agent table's renderer (`views/blocks/table.ts`,
`views/values.ts`), a row's status leading it as a chip, with the selected row marked and its
actions on a wrapping line under it, view-wide actions in its header, and a column marked optional
dropped when the table is narrow. A double-click on a row, or Enter on the selected one, sends the
view's `default` action for it. A page has the keys while it is up; docked, a click in it gives
it them: `j`/`k` move the row, and each action's key runs it; each call the page makes about a
view names it, since several may be on screen at once. An action that asks first asks in the
dialog the Checkouts stage asks before a removal in, and an action is sent as it stands — the table
changes when the extension publishes again. A view taken away, or out of scope where the person now
is, closes: a page of it goes back to the stage it opened over and says so, a docked one is only no
longer drawn, keeping its dock, and a page left for another stage closes. A page opened over another
takes its place, and going back goes to what the first opened over. A reconnect closes a page
without a word: the next daemon publishes its own views. Commands (`extensions/keys`) are in the palette with
their keys, and on the shortcuts sheet under a heading per extension beside the keymap's: a command
heard anywhere runs from wherever the page has the keys and, through the leader, from a pane; one
on a checkout from the Checkouts stage or a checkout picked on the rail; one on a pane from a pane
picked there or through the leader; one in a view while that view has the keys. Its context is the
rail's pick, else the Checkouts stage's row, else the pane on screen. A dialog (`extensions/dialog`,
drawn by `extensions/dialogs.ts`) opens over the window's top without a scrim. One that answers a
command run in this window takes the keys; one an extension raised on its own does not — it waits
for a click, so a pane being typed in keeps every key — the simpler of the two choices weighed,
against turning it into a notice until the person left the pane, since nothing is typed into it by
surprise either way. Later ones wait behind the first, Escape or Cancel sends the cancellation, and
the daemon withdrawing one takes it down. The terminal client does not greet with `extensions`, so
it is sent none of this: agent views are the desktop's, and which of their blocks a terminal could
draw is a decision of its own.

A view that is a console (`ExtView.console`) has a terminal where the table would be, in its dock or
as a page, drawn by the console's own renderer with the same keys, wheel, selection and paste. The
window gives each such view on screen a screen of its own (`Side::View(n)` in `panes/shown`, beside
the Workspace's and the console's two) and subscribes the pane the daemon names in
`ServerMsg::ExtensionViews` while the view is on screen: docked and shown, or the page that is up. A
view that goes off screen or away lets its pane go; one published again after it ended names a new
pane, and the old one is let go as the new is subscribed. The pane is in neither the tree nor the
consoles, so the window counts it held by its view, and a close never kills it: the extension owns
its lifetime. A press on its screen gives it the keys, as one on the console does, and a page that
is a console has them the moment it is up; the leader and Escape behave as in the console. Before
the daemon names a pane the body says Starting; an ended console keeps its last screen under a
strip saying Ended (exit N) or Ended by a signal, with the view's view-wide actions — Restart,
say — beside it. A new connection drops these screens, the new daemon's pane ids being its own,
until its views name them. Row selection and the default action do not apply to it.

## Editors and overlays

Files open in one of three modes:

- a floating PTY overlay, the default;
- as a pane in the checkout — though an editor is never a listed pane, so the workspace cannot
  hold it and it floats as the first mode does;
- an external detached process with no PTY.

Known GUI editors always launch externally. Editor lookup uses `$VISUAL`, then `$EDITOR`, then an
installed terminal editor fallback. Daemon-side validation rejects absolute and parent-traversing
paths. Editor command text is split on whitespace, so quoted arguments and executable paths with
spaces are not supported yet.

Floating panes close through leader-Escape, F12, clicking outside, or process exit. Closing a
floating editor kills it; closing a window over a listed pane only hides the window.

Settings save immediately to `client.toml`. The client ships Catppuccin Mocha, Macchiato, Frappe,
and Latte themes. `ARGUS_THEME` overrides the stored startup theme. The desktop window keeps its own
editor mode and command in `desktop.json` and sends them with each file it opens.

## Testing

`cargo test` covers protocol framing, grid damage, key and mouse behavior, navigation state
machines, UI rendering, Git status and diffs, worktree reconciliation, hooks, workspace scoping,
session restore, and real short-lived PTY processes. Tests live in `#[cfg(test)]` modules because
the binary crates currently have no library targets.

On Windows an argusd test process holds itself, before its first real pane starts, in a job that
ends everything in it when the process does (`pty::job::hold_test_process`). A pane's own job is
what ends its shell, and that is the code a mutation run breaks: without the hold, a broken run left
Git Bash shells behind with their console gone, and enough of them used up MSYS's 128 consoles.

### Characterization transcripts

`test_host` runs the daemon inside the test process, behind its real accept loop, connection
and dispatch, on an endpoint of its own, and a test talks to it through the transport and framing
every client uses. The transcripts under `test_host/tests` pin what it answers today in the
conversations clients have most, chosen so that a side effect leaking into an unrelated answer
shows up: the opening and greeting every connection makes; a pane's life (spawn, watch, type,
paste, resize, rename, kill and exit), the terminal client's commonest requests; a feature worked
through on its board, the desktop's commonest; a change reviewed with its history and a file;
and a worktree made, worked in and removed with a shell open while a second client watches. Each
kind of answer is written whole in one transcript and as what changed everywhere else, so a field
added to the tree is a diff in one place. A snapshot changes only with a person's approval; a diff
is a finding, not something to accept. The host has no Git poll, watcher or hook server, so a
tree's Git status is what the first read of HEAD gave until a request changes it.

A transcript is text before it is a snapshot. Each test pins it with `pin!`, which asserts the
snapshot, or, while `RELIAB_CAPTURE_DIR` names a directory, writes it there under the snapshot's
name instead, so `reliab characterize` can compare a capture taken before a refactor with one taken
after without committing either.

### Request fuzz

`test_host::requests` has a strategy for every `ClientMsg` variant, made against a fixture every
host is prepared with — a shell, a feature with a task and an open question — so most requests
name something that exists and some name what does not. `test_host::tests::fuzz` sends arbitrary
sequences of them over the socket, a fresh host per case, and holds the daemon to §"Process model":
it never hangs up, hangs or panics, every request named by an id is answered by that id or refused
by it, and an unnamed one is never answered. An editor request always names an editor no machine
has and every path is under the host's directory, so a case never starts a program or writes a file
outside it.

### Store writes under faults

The store's rule is its header's: a write is a transaction, not a rewrite of everything, so a write
that fails leaves the store as it was. §"Session restore" says the pane table is replaced in one
transaction; §"Features and the decision board" says a feature's slug is made unique inside the
transaction that opens it and a capture is written in one; saved views say removing a feature drops
it from them in the same transaction; and a transfer writes its destination in one store transaction.
`store::tests::faults` holds every write the store makes to that rule, listed in its `WRITES`: the
board (features, captures, tasks and their criteria and notes, decisions, diagrams, the tracker map,
reconciles, pane claims), saved views, review comments, the panel, and the session. Each runs
against a fixture with something of everything and is failed at each of its steps, a step being a
writing statement prepared or a row written, and at its commit. Every row of every table, ids
included, must afterwards be what it was before or what the write leaves when nothing fails, an
error must come back where a panic could, and a write that was stopped must succeed when tried again.
A property runs the same on a store in a file with a full disk and another writer's lock as well,
and through the daemon a stopped write is refused and the connection lives on.

`store/faults` puts each fault in through SQLite's own hooks on the one connection, an authorizer and
progress handler counting steps, a commit hook, `max_page_count`, and a second connection holding the
write lock, so the code under test runs unchanged. SQLite checks the progress handler only every few
steps, so a fault meant for after a write's last row can land past its commit and leave the write
whole, which the rule allows; each write is still stopped before its first step and at its commit.

Migrations run in one transaction with the `user_version` they reach, and the legacy file import
inserts with `OR IGNORE` and retires a file only once it is in, so a stopped import finishes when
it runs again; neither is in `WRITES`. An agent opening a feature adds it and points its checkout at
it in one transaction (`open_feature_in`), as decided on 2026-10-18: as two, a fault between them
left a feature its own checkout was not on.

A known gap, deferred: a write whose commit landed can still return an error, when a read it makes
after the commit fails, such as the board it reads back for its answer. The test allows that, since
nothing is left half done, but the daemon then refuses a request whose write was saved, and a client
that tries again writes it twice: a second task, a second decision. The intended fix is that a write
answers from what it committed and logs the failed read rather than returning it, so an error from a
store write always means nothing was written.
