# Argus: Current Design

Argus is a terminal workspace for running shells and AI command-line agents against Git
checkouts. This document describes the code as it exists today. Desired behavior lives in
[`TARGET.md`](TARGET.md); unfinished work lives in [`ROADMAP.md`](ROADMAP.md).

## Process model

Argus has four binaries:

- `argus`: the ratatui/crossterm client.
- `argusd`: the daemon that owns PTYs, terminal state, Git state, and runtime persistence.
- `argus-hook`: the helper managed hooks run, and the command an agent runs to report on itself.
- `argus-desktop`: a desktop window, a second view of the same daemon, drawn in a webview.

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
a terminal client being typed in. Its keys are one table, `keymap`, of bindings each with its keys,
where it works (Anywhere, In a pane, Sidebar, Feature, Board card, a dialog, a text field — the
shortcuts sheet's groups), what it does and what it is called. A key in a pane goes to Rust, which
dispatches it from the table; the table crosses to the page once it loads, and the page dispatches
every other key from it, a text field's Enter and Escape included, so a key cannot be run by one and
missing from what the sheet and the palette are drawn from; a test fails a page module that reads a
key's name itself. Who has a key outside a pane is the table's too: an ordered list of layers — a
text field, each dialog, the sidebar's pick, a menu over the stage, each stage — each with the places
it hears in order and whether it takes the keys from a pane it opens over. The first layer up has
the key, Escape puts away the first thing it finds in the same order, and the page (`focus`) holds a
handler per layer but no order of its own, so a dialog is added in one place and a test checks every
layer's places for a key bound twice. A binding of the Everywhere place, the zoom keys', is heard by
the page before anything else, a pane and a text field included, and so is bound nowhere else. The
zoom is the webview's own rather than the page's CSS, so a pane's cells grow with the rest and its
grid refits to the new size; it steps as a browser's does and is kept in `desktop.json`.
In a pane every other
key is the program's except the leader, `Ctrl-Space` as in the terminal client: the key after it is
the window's, one nothing binds is dropped, and a second leader sends one through. `Ctrl-K` (in a
pane, the leader and `k`) opens the palette, one fuzzy search over the table's commands and the
panes, checkouts and features Rust lists while it is open (`palette`), `>` narrowing it to commands
and `#` to features; a command chosen is a call that `session` runs where its key would, the
checkout ones on the Checkouts stage's row or else the pane on screen's checkout. A stage hides the
pane, so from one the pane commands find nothing on screen, and a held or archived feature chosen
unfolds the Feature index to it. `?` (in a pane, the leader and `?`) opens the shortcuts sheet,
which is the table's groups drawn whole, opening with the rule that a pane has every key but the
leader. Both are drawn over everything, a review included, and a pane's leader key reaches them as a
`Run` event, since only the page draws them. So does the Settings dialog, `Ctrl-,` (in a pane, the
leader and `,`), also in the palette and behind the gear at the sidebar's foot: the theme (Mocha,
Macchiato, Frappé or Latte, or following the system as Mocha in the dark and Latte in the light),
the background's opacity and what shows through it, the two fonts, where a file opens and the editor
command, the system notification an agent coming to need you raises, and the layout a repository's
Feature index opens as before it remembers its own. Each choice is a call that `preferences` holds
and saves as it is made, and the page draws the look from what comes back. Each font is the family of
one installed on the machine, typed or picked from a list under the field that draws each family in
its own face: the families `fonts` reads at start, those the webview can find under that name, with
a note when nothing is called what the field holds. A half-typed name is not chosen until it is
picked, entered or left. The code font, offered from the monospaced families, goes in front of the
bundled JetBrains Mono and Symbols Nerd Font Mono, so the panes draw a prompt's or an editor's Nerd
Font icons whichever text face is chosen; a pane re-measures its cells when it changes. The
interface font, offered from every family, goes in front of the system's own and draws the rest. The window is always built transparent,
since that can only be set as it is made, so the opacity is only the page's alpha on its large fills
— the sidebar, the strip, the stages and the terminal's background — and never on text or borders,
and moving it never rebuilds the window. What shows through is Clear, or on Windows Mica (Windows 11
only, greyed out with the reason elsewhere) or Acrylic, which `backdrop` puts on and takes off the
live window through `window-vibrancy`; elsewhere only Clear is offered. A notification names the
pane, what came of it and where, for an agent turning waiting, needs-review or failed, never for the
pane watched in a focused window, and by default only while the window is in the background: the
window's version of the terminal client's bell, beside its chime. The leader and `f` fill the window with the pane,
putting the sidebar away until the same keys again. The console (`console`) is a fourth place in
the nav, keyed 4 (in a pane, the leader and 4): the daemon's consoles (`ServerMsg::Consoles`) as
tabs, `+` asking for one (`SpawnConsole`) that appears once the daemon's consoles hold it, and a
tab's close ending it. It is docked beside the Workspace, Feature and Checkouts stages — at the
bottom until it is moved, by its head's dock menu or the palette's Dock the console on the left, on
the right or at the bottom — with a sash to resize it, fills the stage as the Console, fills all but
the sidebar's projects fullscreen (the
leader and `z`; the nav and the sidebar's foot are put away, the strip stays for the window's
buttons), or is hidden, a `›_` in the sidebar's foot and `` ` `` (the leader and `` ` ``) showing it
again. The Workspace's pane and the console each have a screen of their own, so a docked console
and an agent are on screen together and only the keys move between them: a console shown or picked
takes them, and the Workspace's pane takes them back when it is pressed, chosen on the rail, or
asked for by the Workspace's key, or when the console hides and lets its panes go. The console
splits in two side by side (its split button, the leader and `\`, or the palette), the second side
showing the first tab on neither side, or a console asked for when every one is shown. Each side —
the Workspace's and the console's two — is its own screen (`shown` keeps a subscription, a
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

Which stage is up is `stage`'s: one `page::Stage` set through `go`, or a page opened over one,
which closing, docking or its view going takes back to the stage under it and going anywhere else
closes. The rail goes to the page whenever it differs from the last one sent, so no move has to say
it changed it. It says too whether the Workspace's panes are drawn — only while its stage is up and
no fullscreen console covers it, the composer covering them as every other stage does — and the page
hides them by the rule that takes their keys. `go`'s callers hand the keys on and put fullscreen
away themselves, since half the ways out of fullscreen move no stage.

The docks (`docks`) are the stage's left, right and bottom edges; the sidebar keeps the window's
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
one button (`docks.ts`'s `dockMenuButton`); what a panel can do there — float, take the stage, close
for good — is the docks' to say (`PresentPanel`), and the page reads it from `panels.ts`, never from
the panel's name. A panel is dragged where it goes (`drag.ts`): its dock tab, its header, or an
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
(`session`); the page draws what it is sent and reports gestures. It is TypeScript in
`crates/argus-desktop/page/src`, one module per subject (`chrome`, `frame`, `compose`, `console`, `docks`, `floats` (the dialogs a saved view floats in, moved by the header and resized by the corner), `drag`, `panels` (what Rust says each panel that exists is and can do), `screen`, `input`, `keymap`, `focus` (which layer has a key),
`palette`, `shortcuts`, `settings`, `font_picker` (a font field and the installed families under it), `menu` (menus over the stage, and what a right-click opens), `rename`, `features`, `work`, `record`, `flows`, `sequence`, `capture`, `checkouts`, `review`, `comment_box` (the comment box under picked lines, the review's and the Code stage's), `history`, `sessions`, `transcript`, `projects`, `chime`, `views` (the agents' views panel: `blocks` draws a view's blocks and hands each to the module of its kind, `markdown`, `fields`, `table`, `list`, `stats`, `callout`, `code`, `diagram`, `card`, `grid`, `details`, `row`, `chart`, `progress`, `timeline`, `tree`, `graph` and `graph_layout`, `patch` (a diff block), `matrix`, `checklist`, `log`, `board` and `form`; beneath them `values` draws one value, with `refs` for those that point into Argus, `chip` and `viewlink`; `actions` draws the buttons an agent put in a view; a table's parts are `table_bar`, `table_details`, `table_sort` and `table_memory`; and `cell_text`, `copy` and `viewctx` serve them all), `extension_views`, `dialogs` (an extension's question), `middle` (a middle click closing the tab or row it lands on), with `keys`, `dom`, `state`, `edge` (a side column's edge, dragged),
`clock` and `tauri` beneath them, and `tickets` for a feature's ticket links),
compiled into `assets/js` and committed, so building the crate needs no Node. The events it reads
are `page.rs`'s types and the page and gesture types each surface's module keeps beside it
(`console`, `docks`, `capture_note`, `checkouts`, `code_stage`, `extension_views`, `extension_keys`, `comment_box`, …), written as TypeScript by `cargo test` into
`page/src/bindings`; CI fails when
either committed output differs from what its source makes. Its webview keeps its storage under the
instance's config directory, so a dev window never shares it with the installed one, and it logs to
`argus-desktop.log` beside the config, keeping the previous run's.

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
| `review/history_view` | a checkout's recent commits flattened into rows, each unfolding to the files it touched once they are asked for |
| `code` | a checkout's code as the Code stage reads it: which version, a file's lines and the ones that version changed, what the file defines, and where text occurs across the checkout |
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
| `agents/sessions` | a repository's agent sessions as listed: the harness and id that resume each, its checkout or the removed worktree it ran in, title, times and turns, and the pane running it |
| `agents/transcript` | what a past session said, in one shape whichever harness wrote it — its turns, replies and tool calls, its title and times — and whether its harness keeps a transcript at all |
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
| `state/clock` | what time it is for the durations the daemon decides on — a resume's grace, a restart loop's window, a quiet child's age |
| `state/agents` | what an agent reports about itself, and what it is told |
| `state/agent_launch` | an agent starting another: which checkout it lands in, which template it gets, and who may ask |
| `state/follow` | which checkout a pane belongs to: the one its agent works in, moved to when the agent says so or a status report says its turn ended in another worktree, and the primary checkout once its worktree is removed outside Argus |
| `state/viewers` | what each client is showing: the one pty size reconciled out of what they all ask for, and the view of the client the person was last at |
| `state/git_ops` | the writes to Git |
| `state/git_reads` | the port the tree's reads of Git go through — status, HEAD, worktrees, a root's repositories — so a test can script what Git answers |
| `state/sync` | the polls and watchers that keep the tree level with the disk |
| `state/panel` | the rows the user adds and removes by hand |
| `state/workspaces` | which scope is open, daemon-wide |
| `state/extensions` | what the running extensions publish — views, commands, open dialogs — which client a dialog is put to, and what the person does with them on its way back |
| `state/extension_consoles` | the consoles extensions' views run: started when a view is published, kept while it asks for the same run, ended with the view or the extension, and their ending recorded on the view |
| `state/extension_host` | running the configured extensions: each started with its own URL and token, started again when it exits, stopped with the daemon |
| `state/session` | what survives a daemon restart |
| `state/sessions` | a repository's agent sessions: its harnesses' transcripts for its checkouts joined to the agent panes running them, what one session said, and reopening one by its id |
| `state/inbox` | how what Argus says to an agent reaches it: through its harness's inbox when one is open, typed into its terminal otherwise |
| `state/tree` | finding your way around the tree |
| `state/board`, `state/board/features` | a feature board and who may change it; which feature a checkout is on, and the one apply a change to a feature takes, with the board's address and who asked as parameters |
| `state/board/tasks`, `state/board/decisions`, `state/board/diagrams`, `state/board/tracker_map` | a feature board's rows: how a task or a diagram is stored, a decision added, changed and read, and the tracker map written or drafted — each change one apply, whoever asks |
| `state/board/reconcile` | a feature's reconcile with its tickets: who may ask, propose, tick and report, side by side in one apply, and what the agent is told at each turn |
| `state/board/parts` | which feature a task or diagram request lands in, which rows it may touch, and which lists its change pushes |
| `state/board/address` | which board a request addresses, and who hears of it: an agent's pane or a client's checkout resolved once into the board's store key, its project and the repositories a push of it names |
| `state/board/asker` | who is asking for a change to a board — a person at a client, or an agent with its pane and session — and how a row an agent left names it |
| `state/views` | which views agents have shown, per pane, and which go when |
| `state/view_timers` | which views are being asked to refresh, and the task that asks |
| `state/saved_views` | the views agents kept: saving, marking stale, deleting, refreshing and pressing them, which of them an agent may reach, what each client is told as the rows or the tree change, and the staleness read off the tree |
| `connection` | one client connection: which task each message runs on, and the topics it has joined |
| `connection/topics` | what a connection is pushed: each topic's first send, what each change sends, and how a client that fell behind is brought level |
| `connection/dispatch` | what each client message does, and the one way its answer or refusal reaches the client that asked |
| `hook` | the loopback pane API: which daemon call each agent's or extension's request reaches, behind its token |
| `hook/http` | the pane API's HTTP: a request's line, headers and bounded body read off the socket, the answer written back, and an event stream held open with its heartbeat |
| `review`, `review/queue` | a review's diff, and when it is generated and which asks it answers: one at a time across the daemon, each client holding one place in that turn and read in the order it asked, an ask repeated while its twin waits answered by that one diff, asks for different checkouts, sides or commits each read on their own, and every ask answered |
| `review/diff` | a review's stable snapshot and its diff, and a commit's |
| `pty`, `pty/job`, `pty/console_host`, `pty/vt` | a pane's child process; launching it, bounding what it starts and ending all of it; which process is its Windows console server; and its terminal emulator with the translation of its screen |
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
| `git`, `git/browse` | the read-only questions asked of a repository: its status and worktrees, and what a checkout contains for the pickers |
| `git/watch` | when a repository's Git metadata changes, as it happens rather than on the next poll, and the one-file watch the `projects.toml` reload uses |
| `git/gitignore` | the repository-local excludes for generated Argus files |
| `syntax`, `syntax/grammar` | which tree-sitter grammar reads a file, chosen by its extension, so highlighting and outlines agree on its language |
| `syntax/highlight` | what each token of a diff's or a file's lines is, for highlighting |
| `code`, `code/search`, `code/symbols` | what the Code stage reads: a version's files and how much each changed, one file's lines with the ones it changed, where text occurs in the working tree, and what a file defines |
| `config` | `projects.toml`, which is read and never written |
| `system`, `system/daemon_lock`, `system/logging`, `system/command`, `system/paths`, `system/editor` | what the daemon asks of the machine it runs on: the one daemon allowed to own an instance, where its diagnostics go, a command run without a console window, whether two spellings name one place, and the person's editor |
| `system/pastes` | where a file pasted into a pane is kept on the pane's host, and how its path is spelled into the pane |
| `system/dll_search` | where the daemon loads libraries from on Windows: its own directory and System32, never the directory it was started in |
| `system/ctrl_c` | whether a pane's child can be interrupted on Windows: the daemon clears the ignore-Ctrl-C flag it was started with, so no child inherits it |

`argus` is a replaceable renderer over one model. `app` holds the state and never predicts the
result of a request; `ui` is a pure function of it.

| module | answers |
| --- | --- |
| `main`, `redraw`, `terminal`, `wire`, `launch` | the event loop, the screen and socket it runs over, and the daemon lifecycle command |
| `sibling` | starting another of Argus's executables, found beside this one, so it outlives this process |
| `desktop` | the `argus desktop` command: opening the desktop window beside this terminal client |
| `bridge` | this machine's daemon on stdin and stdout, for a client on another machine to reach over ssh |
| `focus` | the `argus focus` command: where the person is working, as JSON, once or on every change |
| `extension` | the `argus extension` command: the guide to building one, and the running extensions listed or one shown whole, as the daemon here or under `--host` describes them |
| `hosts` | the daemons this client is attached to, each with an app of its own, and which one is on screen |
| `remote` | reaching a daemon on another machine through ssh and its bridge, and saying why when ssh cannot |
| `app` | the model: the tree, the selection, and which modal is up |
| `app/rows`, `app/layout`, `app/modal` | naming a row independently of its index, where the last frame put things, and the layers that float over it |
| `app/nav`, `app/input`, `app/mouse`, `app/scroll` | what the operator's gestures mean |
| `app/rail` | what the rail lists, in what order, which repositories are open and which checkouts folded, and what choosing a row does |
| `app/mode` | which mode has the keys — the one answer key dispatch, the status bar and `?` all match on |
| `app/mode/keymap` | what each mode's keys are, said once: the keys, what `?` and the bar call them, and what they run |
| `app/actions`, `app/pickers` | what is asked of the daemon, and the modal layers that ask it |
| `app/awaited` | what the terminal client does with what the daemon made for it — focus the pane, select the row, start what was asked for in the new checkout — once `awaited` says the tree, or the consoles beside it, holds it |
| `app/consoles` | the shells that belong to no checkout: the daemon's list of them, which one the stage shows, and asking for another |
| `app/server` | what arrives back, and what it does to the selection |
| `app/agent_views` | the views agents showed as this client holds them, newest first: which is open, how far it is scrolled, and the close and refresh requests that change nothing until the daemon answers |
| `app/saved_views` | the saved views as the daemon sends them, which one the stage shows, the links offered from it and the trail of links followed to it, and the embeds drawn in place |
| `app/showing` | what the daemon is told this client shows |
| `app/views` | which surface the content area holds, and the fetches opening one makes |
| `app/feature` | where the Feature stage is — the index, a feature on a lens, the row under the cursor — and how the keys move between them |
| `app/board_writes` | what the Feature stage asks the daemon to change: the line or note being typed, and the message each key sends |
| `ui` | the frame, and where the cursor goes on it |
| `ui/card` | a card and the rows in it: its frame, one row, and how a list longer than the card scrolls |
| `ui/command_center` | the HTML-specified shell: where the rail and stage go, the stage heading, the first-run state, and the status vocabulary they share |
| `ui/command_center/rail` | drawing the rail — its heading, the rows `app/rail` lists, the divider between the active repositories and the quiet ones, and the consoles at its foot — and which row a click lands on |
| `ui/command_center/workspace`, `ui/command_center/feature`, `ui/command_center/checkouts`, `ui/command_center/agent_views` | one stage each, with the hit-test that shares its layout |
| `ui/command_center/feature/index`, `ui/command_center/feature/work`, `ui/command_center/feature/record`, `ui/command_center/feature/flows`, `ui/command_center/feature/capture` | what fills the Feature stage's frame: the index of features, each of the three lenses, and the note a feature is written as |
| `ui/agent_blocks` | an agent view's blocks as styled lines of a width, and which kinds are left to the desktop |
| `ui/agent_tone` | how an agent view's tones read as theme roles, and the chip drawn through them |
| `ui/agent_values` | what each kind of agent view value reads as in a terminal: styled text on a line, or wrapped lines for a long text |
| `ui/text` | fitting text to a width: ellipsis, wrapping a field or prose, one-lining |
| `ui/help`, `ui/prose` | the keymap window, and markdown styled where it stands |
| `ui/views` | the tab strip naming the four stages, and which tab a click lands on |
| `ui/review`, `ui/history`, `ui/status`, `ui/overlay`, `ui/modals`, `ui/term` | one drawn surface each |
| `brief`, `dirpicker` | the view state behind each overlay |
| `diagram` | the Mermaid sequence-diagram view and its responsive overlay layout |
| `selection`, `pty_input`, `paste`, `clipboard` | a pane's selected text, and the input primitives |
| `pty_input/keys` | a terminal key event as the protocol's key, and the leader chord that is never sent |
| `pty_input/mouse` | a terminal mouse event as the protocol's, relative to the pane's area |
| `dropped` | a file dropped on the terminal, recognized in what it pasted |
| `browser` | opening a ticket's URL in the person's browser, or copying it where no browser is at hand |
| `motion` | animation arithmetic: how far along a transition is at a given instant |
| `settings`, `theme`, `backend`, `herdr`, `profile` | preferences, palette, the ratatui backend, and what is reported outward |
| `fixtures` | the builders every test module shares: the tree, and a pane's view as the daemon's messages leave it |

`argus-desktop` is the desktop window. It holds what it was sent and draws it; everything it knows
of a pane, a checkout or the daemon came from the protocol, and the page only draws and reports.

| module | answers |
| --- | --- |
| `main` | startup: the runtime, the window, and running until it closes |
| `daemon` | reaching this machine's daemon through `argus bridge`: starting the bridge, greeting, and connecting again when it goes |
| `session` | what this window holds of the daemon and routes between its parts — the tree, the project and repository chosen on the rail — and which part each message, refusal and page call is for |
| `switcher` | what the project switcher offers beyond the tree's projects — the daemon's workspaces — and the project waiting on a yes to be removed |
| `add_project` | what the add-a-project dialog holds — the kind of add, the folder browsed and the daemon's listing of it, the repositories the daemon says it would add, the name and skips typed, and the add waiting on the daemon — what its gestures ask of the daemon, and the page and gestures it trades with the page, last sent while open |
| `status` | what the status bar says — the connection, its last failed try, an alarm about the daemon reached, whether the window chimes — and whether the page has heard it since it changed |
| `preferences` | the Settings dialog's choices as the window holds them: each change the page makes, held to what this machine can draw, and whether the page has heard them and the file has kept them |
| `notifications` | the system notification an agent coming to need you raises: whether a tree's changes raise one, what it says, and handing it to the system |
| `backdrop` | what shows through the window where its background is see-through: the backdrops this machine can draw, and putting the chosen one on the live window |
| `fonts` | which font families this machine has installed, by every name the webview might know each by, and which are monospaced, for the Settings dialog to offer |
| `shown` | the panes the window shows — the Workspace's grid, one per side of the console, and one per extension console view on screen — each side's subscription, size and view on this connection, which side has the keys, whether the Workspace's pane fills the window, and whether the leader waits on the next key |
| `shown/workspace_grid` | the Workspace's grid: which panes it holds — the Workspace's own first, then those split beside it — where each sits and how big, which pane may join it and the refusal for one that may not, and the page the rail carries of it, its sizes in thousandths |
| `shown/workspace_grid/split_grid` | where the Workspace's panes sit in relation to each other — rows, column fractions, row heights — and what adding, closing, swapping and moving a pane does to them, as arithmetic |
| `console` | where the console is — docked, the Console stage, fullscreen or hidden — the daemon's consoles and the tab chosen for each side of a split, the page it draws and the gestures it takes, and what the page last heard |
| `docks` | where each panel sits: which dock — left, right or bottom — holds it among its tabs, whether it is a page, or a dialog floating over the stage with where it sits, how big, and which is in front; which tab each dock shows of the panels that exist, whether it is open and how big, whether that changed since it was saved; what each panel that exists can do — float, take the stage, close for good — and which kind it is, told to the page so it never reads them from a panel's name; the page and gestures of the docks, and what the page last heard |
| `agents` | which agents the window offers to start: the daemon's templates, the one last started first while the daemon still offers it |
| `dialogs` | the review and history dialogs as the window holds them: which is open or parked, the daemon's answers and refusals routed to the one that asked whether open or parked, the layout the next review opens on, and what the page last heard of each |
| `sent` | what the page was last sent of something it draws only while open, so it hears each change once and a closing once, or of something always on screen, so it hears only what differs |
| `screen` | a pane's screen as the page draws it: the rows of its view changed since the last frame, as styled segments |
| `rail` | what the rail lists from the tree, and what a row says |
| `dragged_width` | how wide the person dragged a side column — the rail, an open feature's outline — held to its bounds, and whether that changed since it was saved |
| `review` | what the review dialog holds beyond its `review::session` — the side and layout chosen, its comment box, its go-to-file field — and the page it draws |
| `comment_box` | the comment box the review and the Code stage share: whether it is open, the agent it is lit on, the comment it sent and waits on the daemon to save or refuse, by its id, and what the page draws of it |
| `history` | what the history dialog holds — a checkout's newest commits, the ones unfolded to their files, the filter over them, and each opened commit's diff or why it could not be read — and what its gestures ask of the daemon |
| `sessions` | what the window has asked of a repository's agent sessions — the list, what one said, and how each harness resumes one — and what it holds of the answers, or why the daemon refused one, among them what each pane it resumed a session in was resumed from |
| `sessions_dialog` | what the recent-sessions dialog holds — whose sessions, the search and the harness and checkout filters, the session under the cursor, whether its full transcript is open in place of the list — and what each session offers: its pane, a resume by the exact command named, or only its transcript; the page it draws, last sent while open |
| `transcript_reader` | the full-transcript reader: the person's turns as a jump list and the one it is on, what find looks for and the match it is on, and the page it draws |
| `checkouts_stage` | what the Checkouts stage holds — whether branch rows show, the filter, the selected row, the question waiting on the person — which of its keys and buttons are the `tree::column::verbs` it sends, and the table the page last heard |
| `checkouts` | what the Checkouts stage lists from the chosen repository: its rows, what each says, and the selected checkout's detail, as the table the page draws |
| `code_stage` | the Code stage as the window holds it once opened — the checkout it reads, the version, that version's files and changes, the folders open, the file read and its cursor, the comment being written, the go-to-file field, the search, and the review it was opened from — the daemon's answers it takes, what its gestures ask of the daemon and leave to the window, and the page and gestures it trades with the page, last sent while it is up |
| `code_tree` | a version's flat file list as the tree the Code stage draws: the rows that show given the folders open, and which folders and files changed |
| `file_jump` | the go-to-file field the review and the Code stage share: what is typed, which match is lit, and the file Enter opens |
| `feature_stage` | what the Feature stage holds — the repository's board, the feature under the cursor and its tasks and flows, whether it is opened, the lens it is read through and each lens's cursor, each repository's layout, the filter and the fold — and what its messages and gestures change, and where each cursor steps, never onto a row the page does not draw |
| `features` | what the Feature stage's index shows from a board: which features it lists and in what order — the filter, the fold (held and archived on the list, archived on the board), the board's columns, the opened feature kept — its rows and a card's footer, the places a person may move each, and the peek with the features it may come after |
| `feature_view` | what a feature opened from the index shows: its header — how it stands, its branch, agent, brief, progress, tickets, when it was last reconciled and how much changed since — and the lens it is read through, and the rows each lens's cursor steps over |
| `work_lens` | the Work lens: tasks and decisions as `outline` reads them, with each task's tracker key and a mark on each row changed since the last reconcile, and the task or decision under the cursor in full |
| `link_dialog` | what the link dialog holds — the URL and role being typed and what the URL reads as, the linked ticket under the cursor, the tracker map being edited, a link sent and not yet on the board — what its gestures ask of the daemon, and the page and gestures it trades with the page |
| `reconcile_dialog` | what the reconcile dialog holds — the line under the cursor, the lines ticked and the words each was rewritten in, the agent an ask goes to — what its gestures ask of the daemon, and the page and gestures it trades with the page |
| `tracker_dialogs` | the link and reconcile dialogs as the window holds them: which of the two is open and on which feature, closing it when the board loses that feature, a refusal of the link the link dialog sent, and what the page last heard of each |
| `record_lens` | the Record lens: every decision in the order it was made, and a decision read as its record — what replaced it, what was turned down, and the ADR it copies as |
| `flows_lens` | the Flows lens: a feature's flows, the one under the cursor as steps, and the step under the cursor with what binds it and what carries it out |
| `sequence` | a flow's source as the sequence diagram the page draws: its lifelines in order, each step as an arrow between two of them, and the frames a branch draws |
| `feature_edits` | what an edit to an open feature asks of the daemon: each gesture as the message the terminal client sends for it — a drag on a flow as its source rewritten — checked against the protocol's bounds; a new row saved blank sends nothing |

| `capture_note` | a new feature being written as one note: what it reads as by `capture`'s grammar, whether the board holds the feature it was sent as yet, and the page and gestures it trades with the page, last sent while open |
| `settings` | what the window remembers between runs: each repository's index layout, the agent it last started, whether it chimes, the side and layout a review opens on, where each panel is docked and how big each dock is, and the Settings dialog's choices |
| `page` | what the page and the window's Rust say to each other: the events down its channel, and every call it makes as one `Call`; a surface that keeps what it last sent defines its own page and gestures, which these name |
| `commands` | the two commands the page invokes — `connect` for its channel, and `call` for every `page::Call` — and which part of the window answers each call |
| `shared` | what the page's calls, the daemon's messages and the frame tick share, and the order their effects leave in |
| `keymap` | the window's keys: each binding's keys, where it works, what it does and what it is called, read by dispatch in Rust and on the page, the commands the palette runs, and the order of the layers that hold the keys outside a pane |
| `palette` | what the command palette can go to: the project's panes and checkouts, and the chosen repository's features |
| `keys` | a key the page reports, as the protocol's key |
| `clipboard` | the desktop clipboard's text: what a dragged selection copies to, and what the paste key reads |
| `agent_views` | which views agents showed, as the window holds them, which one their panel shows, which name a feature, and which wait on a refresh |
| `saved_views` | the views agents saved as the window holds them, grouped by scope and told apart as here or elsewhere; each saved panel's trail of columns — a link followed keeps the columns up to it, a column closed or popped out into a panel of its own — and which saved views wait on a refresh or a press |
| `view_changes` | what changed in an agent's view since the person last had it in front of them, named by the paths the page draws the blocks at, and whether each change flashes now or waits unseen |
| `view_refs` | where a reference in an agent's view points — the checkout, repository or pane it names — found from the pane that showed the view, which paths it refuses, and a feature it named waiting on its board |
| `stage` | what fills the stage — one of the stages, or a page (an agent's view or an extension's) opened over one — where closing, docking, Escape or its view going takes the person back to, the Code stage's way back included, and whether the Workspace's panes are drawn under it |
| `extension_views` | which extension views the window holds, which are listed under the sidebar's Extensions where the person is, each one's selected row, and the question an action waits on; what a gesture in one sends; and the page of them all, drawn where the window says each one is, with the commands `extension_keys` lists beside them, and what the page last heard |
| `extension_keys` | the extensions' commands and keys as the window's: where each key is heard, what the palette lists, and a heading of the shortcuts sheet per extension, as the page draws them |
| `extension_dialog` | the questions extensions put to this window: the one open, those waiting behind it, the answer or cancellation each sends back, and the page last sent of them |
| `opener` | handing a link in a view to the system's opener, and only an `http` or `https` one |
| `logging` | where the window's own diagnostics go |
| `window_frame` | the window without the system's title bar, and the calls the page's strip makes to move, size and close it |
| `fixtures` | the tree builders the test modules share, and the commits and diffs the daemon answers about it |

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
chord reaches the list from inside a pane. Which mode has the keys is decided once (`app/mode`)
and key dispatch, the bar and `?` each match on that answer exhaustively, so a mode one of them
forgets does not compile — the three used to work it out separately, in different orders, and
disagreed. What each mode's keys *are* is decided once too (`app/mode/keymap`): a mode declares
its bindings — the keys, what `?` says of them, the bar's hint and how many narrower tiers keep it,
and the action — and dispatch looks the key up there, `?` lists it, and the bar is read off the
hints. A binding can be kept to `?`, or out of both, but only on purpose: a test fails when a key
a mode takes is on neither its bar nor in its `?`. The bars that depend on more than the mode —
the feature stage's per panel, the leader, a parked or fullscreen pane, a brief being written, and
the typing surfaces — stay written out in `ui/status`, and typed text and the leader chord are
still dispatched by hand in `app/input`, with the keymap only saying what `?` lists for them.

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
child that appeared meanwhile. An agent's job also has a 32 GiB committed-memory limit and a
256-process limit, and everything the agent starts stays in it. A shell's job is unbounded and lets
what the shell starts leave it silently: a shell is where a person starts what should outlive the
pane, and `scripts/reload.sh` schedules from one the restart that replaces the daemon, which the old
daemon's exit must not take down. Closing a pane or dropping its runtime terminates the job, the
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
requested that mode. A wheel over a pane whose child is on the alternate screen and has not asked
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
alone, in `state/inbox`; `PaneInfo::live` says one is open. opencode's plugin turns a message into
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
What each endpoint is posted and answers with is declared once, in `argus_protocol::hook::call`:
`argus-hook` makes its call through the declaration and the daemon answers through it. A body the
endpoint cannot read is refused with a `400` in serde's words, a refusal is a `409` carrying the
reason, and a body past the endpoint's bound (`Endpoint::max_body`) is refused by the helper before
it travels and by the daemon with a `413` naming the bound if it arrives.

When the person wants a task kept in a pane they can go back to, an agent starts another agent
through the daemon rather than as a subagent of its own turn. It is an ability, not how worktree
work is done: an agent still works in a worktree itself, or delegates to a subagent, unless asked.
`argus-hook agent [--template T] [--worktree B [--from C] | --in C] (--stdin | <prompt>)` posts an
`AgentLaunch` to `/pane/<id>/agent` (`agents/launch`, `state/agent_launch`). The new agent gets a pane
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
checkout of the agent's repository holding it (`state/follow`), discovering a new worktree the same
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
does not die with it: it moves to the repository's primary checkout (`state/follow`), and an
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
diff already on screen and its side. A refresh keeps the file the cursor was in, found by its path.
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
file loses its width. Search runs on Enter rather than on every letter — a search reads the whole
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
same `outline`, `standing` and `flow` (`feature_view` and a module per lens). Its index is a list
grouped Active, Queued, Done — held and archived folded behind a toggle (`z`) that on the board
folds the Archived column — beside a peek, or a board (`V`, remembered per repository in
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

The desktop window holds the saved views (`saved_views`) and gives each one a panel, `saved:<id>`
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
again the window names the blocks whose content changed (`agent_views`): they flash once when the
person is looking at that view, and otherwise stay marked, with a dot on its tab, until it is opened.
What the page alone remembers about a view — a chosen tab, an open details or tree node, a table's
selection, opened rows and filter, a form's draft — is kept by block path across those redraws. Pop
out opens a view as a stage page over whatever stage was up, and Dock, the views panel brought out
or the agent closing it goes back there; a view closed says so by the title its agent last gave it.
While a view is the page it is the one in front of the person: shown again it flashes there, and the
panel does not come out by itself beside it. A reference value opens in the window and never a
browser (`view_refs`): a path in the person's editor, a commit in History, a feature in the Feature
stage, a pane focused; each is read against the pane that showed the view, or the checkout on screen
for an extension's table, and a path that is absolute or climbs out of the checkout stays plain
text. Copy, on a code block or a table as TSV, goes through Rust's clipboard. A feature's ticket
links, in the index's peek and the open feature's heading, open the ticket in the browser, or, when a
view of that ticket waits, carry a dot and open the view instead; the page matches them by URL, or by
tracker and key. An open feature's header lists its tickets with their roles, when it was last reconciled and by whom,
and how many of its tasks and decisions changed here since; the Work lens dots each of those rows.
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
away. Every string is drawn as text, Markdown included; a link opens through `opener` in the
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
after its hook server binds (`state/extension_host`), handing it `ARGUS_HOOK_URL`
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
with its title and icon (`extension_views`). Nothing listed, the fold is not drawn, as the rail's
fold of quiet repositories is not; whether it is unfolded is kept in `desktop.json`. Each view is
a panel of its own (see the docks): it opens in its dock, or as a page filling the stage
(`Stage::Extension`), wherever its extension's `place` (`left`, `right` — or `panel`, the same —
`bottom` or `stage`) says the first time and wherever the person put it after: a docked view's
header offers Open as page and its dock menu, a page's Dock, also the keymap's `ExtensionPage` and
`ExtensionDock` (`>` and `<` while the view has the keys, from the palette on the view last used),
and the docks keep the choice in `desktop.json`, across its extension republishing or restarting.
Docking goes back to the stage the page opened over. A view has no digit, the digits being the
built-in stages': it is reached from the sidebar, its Tab and Enter included, from the palette's Go
to, and the shortcuts sheet says so under its extension's heading. Docked beside the agents' views
it is another tab of their dock, and an agent's view arriving never takes the dock from it; in a
dock of its own both are on screen, and a page beside views docked elsewhere too, each keeping its
own selected row. Either way it is drawn by the agent table's renderer (`table.ts`, `values.ts`), a
row's status leading it as a chip, with the selected row marked and its actions on a wrapping line
under it, view-wide actions in its header, and a column marked optional dropped when the table is
narrow. A double-click on a row, or Enter on the selected one, sends the view's `default` action
for it. A page has the keys while it is up; docked, a click in it gives
it them: `j`/`k` move the row, and each action's key runs it; each call the page makes about a
view names it, since several may be on screen at once. An action that asks first asks in the
dialog the Checkouts stage asks before a removal in, and an action is sent as it stands — the table
changes when the extension publishes again. A view taken away, or out of scope where the person now
is, closes: a page of it goes back to the stage it opened over and says so, a docked one is only no
longer drawn, keeping its dock, and a page left for another stage closes. A page opened over another
takes its place, and going back goes to what the first opened over. A reconnect closes a page
without a word: the next daemon publishes its own views. Commands (`extension_keys`) are in the palette with
their keys, and on the shortcuts sheet under a heading per extension beside the keymap's: a command
heard anywhere runs from wherever the page has the keys and, through the leader, from a pane; one
on a checkout from the Checkouts stage or a checkout picked on the rail; one on a pane from a pane
picked there or through the leader; one in a view while that view has the keys. Its context is the
rail's pick, else the Checkouts stage's row, else the pane on screen. A dialog (`extension_dialog`,
drawn by `dialogs.ts`) opens over the window's top without a scrim. One that answers a command run
in this window takes the keys; one an extension raised on its own does not — it waits for a click,
so a pane being typed in keeps every key — the simpler of the two choices weighed, against turning
it into a notice until the person left the pane, since nothing is typed into it by surprise either
way. Later ones wait behind the first, Escape or Cancel sends the cancellation, and the daemon
withdrawing one takes it down. The terminal client does not greet with `extensions`, so it is sent
none of this: agent views are the desktop's, and which of their blocks a terminal could draw is a
decision of its own.

A view that is a console (`ExtView.console`) has a terminal where the table would be, in its dock or
as a page, drawn by the console's own renderer with the same keys, wheel, selection and paste. The
window gives each such view on screen a screen of its own (`Side::View(n)` in `shown`, beside the
Workspace's and the console's two) and subscribes the pane the daemon names in
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
