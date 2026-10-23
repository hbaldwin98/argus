# Controls

## Terminal client

`argus` opens the terminal client. Press `q` on the rail to detach; panes keep running. `?` lists the
keys for wherever you are, and the status bar always ends with it.

In the directory browser, use Up and Down to select a folder, Enter to open it, and Left to return
to its parent. Enter on the `add this directory` or `make it in this directory` row confirms the
location. Typing filters the folders in the current directory.

### Navigation

| Key | Action |
| --- | --- |
| `j` / `k`, arrows | Move along the rail — the project, its repositories, the open repositories' checkouts and panes, then the consoles at its foot — in the order it is drawn |
| `l`, Right, Enter | Open the repository and move into it, go to the checkout's first pane (unfolding it), or type into the pane or console |
| `h`, Left, Escape | Up to the row this one hangs under; on an open repository, fold it first |
| `z` | Fold the repository or checkout under the cursor — on a pane, its checkout — or unfold it. A folded checkout keeps its row, with how many panes it hides coloured by the loudest of them; a second click on a checkout does the same, and in the desktop window so do the chevron at its left and its context menu |
| `s` | Start a shell — on a branch row, in a new worktree for that branch; on a console, another console |
| `c` | Start a console: a shell in your home directory that belongs to no checkout, listed under *consoles* at the foot of the rail once the daemon runs it, and typed into straight away. A daemon from before consoles draws no such section, and `c` says to restart it |
| `a` | Choose and start an agent — on a branch row, in a new worktree for that branch |
| `n` | Add a project on the project heading, a repository on a repository row, or a worktree on a checkout — or, in the Checkouts stage, give a branch row a worktree |
| `i` | On a repository row, make a repository that does not exist yet: browse to where it should go, name it, and Argus creates the directory, runs `git init` in it, and adds it to the project. An empty name uses the chosen directory itself, which is how a folder that is already there gets initialized |
| `D` | Remove what the row is, after confirmation: a project or repository (out of the panel only — nothing on disk is touched), a linked worktree (deleted), or a branch row (the local branch, deleted; the remote is untouched). An unmerged branch is refused and asked about again, and confirming that second prompt force-deletes it |
| `w` | Switch workspace |
| `o` | Switch project (command center rail; also click the project name) |
| `b` | Open the branch picker |
| `B` | In the Checkouts stage, show or hide the branches no checkout is sitting on — the main branch's offer first, and the ones only a remote has last |
| `F` | Fetch every remote (`--prune`), which is what makes the remote's branches appear as rows |
| `P` | Pull the selected checkout, fast-forward only |
| `f` | Open the file picker |
| `R` / Tab | Open review, from the rail or the Checkouts stage |
| `H` | Open the checkout's commit history, from the rail or the Checkouts stage |
| `E` | In the Checkouts stage, show the selected checkout's files beside the table, or put them away. In the panel: `j`/`k` move, `Enter` opens a file in Code or a folder, `O` opens a file in your editor, `→`/`←` open and close folders, `/` filters, `Esc` closes |
| `N` | Jump to the next pane, or parent of a child, waiting, failed, or ready for review |
| `S` | Open settings |
| `t` | Choose a theme for this client process |
| `x` | Kill the selected pane or console |
| `q` | Detach the client |

### Views

The stage beside the rail holds one view at a time: `1` Workspace (the selected pane's terminal),
`2` Feature, `3` Checkouts, and `4` Views. The tab strip along the top names them and carries the
digit that opens each; clicking a tab does the same. Switching views never stops a pane.
Drag the rail's right border to widen or narrow it; the width is saved to `client.toml` as
`rail_width`.

The Views stage lists what agents showed, then the saved views, one list. `j`/`k` scroll the open
view, `n`/`p` move along the list, and `x` closes a shown view. A saved view is drawn with its
scope, a `stale` mark and when it was built; `l` lists the views it links to, `Enter` opens the one
under the cursor, and `Backspace` or `h` goes back along the links followed. `r` asks the agent that
saved it to refresh it, and `s` marks it out of date.

The Feature view opens on an index of the selected repository's features, grouped by how they
stand — active, queued, done — with the one under the cursor peeked at beside the list. Held and
archived features are folded away until `z`. `Enter` opens a feature, which is read through three
lenses: `1` work, the tasks and the decisions they raised as one outline; `2` record, every
decision as a tree with the selected one read as an ADR; and `3` flows, its sequence diagrams step
by step. A breadcrumb says where you are and `Esc` climbs it one level at a time. Features belong
to the repository rather than a branch, so they remain available after a feature worktree is
removed.

| Key | Action |
| --- | --- |
| `j` / `k` | Move in whatever has the keys; PageUp/PageDown ten at a time, Home/End for the ends |
| Enter | Open it: a feature, a task's brief, a decision's record, a flow |
| `1` `2` `3` | Inside a feature: read it as work, as a record, as flows |
| Tab | Into the detail beside the list, and back |
| `a` | A new feature written as one note; inside one, a root task, or a flow on the flows lens |
| `i` | Write more of the open feature as one note |
| `s` | A subtask under the selected task, or a sub-decision under the selected decision |
| `d` | A decision, raised under the task the cursor is on. Start it with `?` to leave it open. Flows: name the decision binding the step, `D2.1` or `#id` |
| `u` / `o` / `y` | Supersede the decision, answer an open one in words, copy it as ADR markdown |
| `c` / `n` | A done-when criterion for the selected task, or a note on it |
| Space | Mark the task done or not; in the detail, check a criterion off |
| `e` | Rewrite it: the brief, the task's title, the decision's record, the flow's Mermaid source |
| `h` / `l` | Work: fold or unfold the task. Flows: the flow before or after |
| `H` / `L`, `J` / `K`, `>` / `<` | Move the task along todo, doing, done; among its siblings; in or out a level |
| `g` | Go to the pane working the task or the feature |
| `R` | Rename the feature — the title only; its slug is frozen so nothing filed under it is orphaned |
| `x` | Remove the feature or task. A decision is withdrawn instead, and `x` again restores it |
| `v` / `z` | Archive the feature or bring it back; show or fold the held and archived ones |
| `m` / `p` / `w` | Move the feature to another checkout, hold it with a reason, choose what it comes after |
| `t` / `T` | Open the feature's ticket in the browser (over SSH, copy its URL); link it to a ticket by pasting the URL |
| `M` | On the index: the repository's tracker map, edited like a brief; when an agent has drafted one, the draft, which `A` approves as it reads and `D` turns down |
| `r` | Re-ask the daemon for all of it |
| Escape | Up one level; `q` goes straight back to the workspace |

In the note a feature is written as, the first line is its title and the prose under it its brief;
a `-` line is a task and an indented one a subtask, a `?` line an open decision with its options
indented beneath, and a `!` line a decision already made with the reason beneath. `Ctrl-S` creates
it, Tab and Shift-Tab indent, Escape throws it away.

On the flows lens each step names the decisions and tasks bound to it, the selected step is lit in
the diagram, and the decisions binding it are listed under the steps with the tasks that carry
them out. A binding is a Mermaid comment under the step's message, `%% decision #4 task #9`, which
`d` writes for you. `Enter` opens the selected Mermaid sequence diagram whole. `j`/`k`
scroll vertically and `h`/`l` (or the arrows) pan horizontally when the diagram is wider than the
overlay; `q` or Escape closes it.

### Terminal panes

`Ctrl-Space` is the leader key:

| Chord | Action |
| --- | --- |
| `Ctrl-Space`, Escape | Leave terminal input or close a floating pane |
| `Ctrl-Space`, `f` | Toggle fullscreen for the selected pane |
| `Ctrl-Space`, `x` | Kill the pane |
| `Ctrl-Space`, Tab | Open review |
| `Ctrl-Space`, `N` | Jump to the next pane waiting, failed, or ready for review |
| `Ctrl-Space`, `1` / `2` / `3` | Open a view — the digits belong to the child while you are typing in a pane |
| F12 | Emergency close for a floating window |
| `Ctrl-V` or `Alt-V` | Paste the clipboard; an image on it, or an image file dropped on the terminal, reaches the pane as a file even when the daemon is on another machine |

Other supported keys are forwarded to the child PTY. Use `Alt-V` in Windows Terminal, which keeps
`Ctrl-V` for itself. Images and dropped files cross only from where the client runs: in an ssh
session the client has no clipboard, so run `argus --host <machine>` on the desktop instead.

### Review

| Key | Action |
| --- | --- |
| `j` / `k`, arrows | Move through changed lines |
| `d` / `u`, Page Down / Page Up | Move ten lines |
| `]` / `[` | Next or previous changed file |
| `g` / `G`, Home / End | First or last changed line |
| Enter / `z` | On a fold, the lines the diff left out: show twenty more of them, or all |
| `v` / `V` | Start or clear a line-range selection |
| `f` | Open the changed-file picker |
| `c` | Send a comment to an agent in the checkout; choose one when several are running |
| `e` | Open the selected line in the editor |
| `b` | Toggle between staged and unstaged changes; an empty side leaves the diff on screen |
| `s` | Toggle between the unified and split diff |
| `r` / `R` | Refresh |
| `h`, Left, Escape, `q` | Close review |

The split view puts each removal beside what replaced it, numbered from the old file on the left
and the new one on the right; where one side has no counterpart it is left empty. It wants width,
so it pairs well with a fullscreen pane. A comment means the same thing in either view, because a
comment already records both line numbers.

The chosen side and the chosen view both persist across reopens. Review covers uncommitted work only; comparing against
committed history is left to a dedicated Git tool.

Diffs are syntax highlighted for Rust, TypeScript, TSX and JavaScript, Python, C#, CSS, YAML, TOML,
JSON, and Markdown, picked by file extension; anything else is shown as plain text. Added and
removed lines are marked by a background wash and by their `+` and `-` markers, leaving the
foreground to the syntax colours.

In the desktop window, `o` reads the file under the cursor whole in the Code stage, at that line.

The desktop window's review of working changes also stages and commits, against a daemon that
can (0.41 or later):

- **Lists:** Staged, Changes and Untracked are listed together. Double-click a file, press its
  arrow, or drag it onto the other list to stage or unstage it; each list's button moves all of it.
  A file that is partly staged is in both lists, marked as such.
- **Lines:** every added or removed line has a box, ticked when the line is staged. Click one, or
  Shift-click to take every changed line since the last one; a hunk's button takes the whole hunk.
  Staging happens at once, and the boxes change when Git says the index did.
- **Commit:** the box under the lists runs `git commit` with your message and your hooks.
- **Drafts:** with a review of working changes, the comment box says **Add to review**: comments
  are held as drafts, shown under their lines, until the **Review** button sends them all to the
  chosen agent as one message, with an optional summary, or discards them.

Comments are stored in `runtime.db` before Argus notifies the selected live agent. They remain
visible to every live agent in that checkout through `argus-hook comments`; the command returns the
newest 100 comments in order. If terminal delivery fails after the save, the comment remains stored.

## Desktop window

`argus desktop` opens the window beside the terminal client, on the same daemon, and returns at
once. Its keys are the terminal client's where the two share a surface. `?` (from a pane, the leader
then `?`) opens the shortcuts sheet for wherever you are, and `Ctrl-K` the palette: one search over
commands, panes, checkouts and features (`>` for commands only, `#` for features only).

- **In a pane** every key goes to the program except the zoom keys and the leader, `Ctrl-Space`.
  After the leader: `f` fills the window with the pane, `x` closes it, `Tab` reviews its checkout,
  `n` opens the composer, a digit opens a stage, `o` hands the keys to the next screen, and a second
  `Ctrl-Space` sends one through. `Ctrl-V`, `Ctrl-Shift-V` and `Alt-V` paste; Shift-PageUp/PageDown
  page through history.
- **Stages**: `1` Workspace, `2` Feature, `3` Checkouts, `4` Console, `5` Code, `6` Browsers. The
  sidebar lists extension views and saved views after them. Drag the sidebar's **Browsers** item
  onto a pane to watch every agent's browser beside it, in a cell of the grid; with none open yet,
  the cell waits for the next. Its header moves it like a pane's, and its close takes it out.
- **New** (`Ctrl-N`) opens the composer: a repository, checkout and agent, and the agent's first
  message; `Ctrl-Enter` starts it, or **Shell** starts a shell. `+` on a checkout starts a shell (`S`)
  or an agent (`A`). Drag a pane's row onto another checkout to move it there.
- **The project heading** opens the switcher: projects, Add a project…, Switch workspace…, Connect
  to a host… (from `~/.ssh/config` and hosts used before; keys or ssh-agent only, argus already
  installed there), and Remove.
- **Checkouts** (`3`) lists the repository's checkouts, and with `B` its branches: Enter opens or
  switches, `F` fetches, `P` pulls, `S`/`A` start a shell or agent, `N` makes a worktree, `D` deletes
  a branch or removes a worktree, `R` reviews, `H` opens history, `E` shows its files.
- **Console** (`4`, or `` ` `` to dock or hide it) holds shells that belong to no checkout. It docks
  at the bottom until moved, splits in two (the leader then `\`), and the leader then `z` fills the
  window with it.
- **Git** opens the Changes pane in place of the pane, with History beside it. Its **Back to the
  side pane** button docks it on the right (or where it last sat); from there its dock menu moves
  it to another dock or floats it, dragging its tab onto a cell of the grid puts it beside an
  agent's pane, and **Expand** puts it back on the stage. Dragging the sidebar's **Git** item onto
  a pane opens the review there directly. Beside the work it stays open while you go elsewhere, and
  Git brings it in sight. A file's **Open** reads it in the Code stage at the cursor's line; beside
  the work the Changes pane stays where it is. Narrower than 760px it shows the files first, and a
  file's diff once you open one. Expand works the same way for any panel that can fill the stage,
  such as the browsers and extensions' views.
- **Recent sessions**: the clock on a repository's rail row lists its agent sessions across every
  checkout, with each transcript beside it. Enter goes to a live one or resumes an ended one; `O`
  reads the whole transcript.
- `Ctrl-=` / `Ctrl--` / `Ctrl-0` zoom (`Cmd` on a Mac). `Ctrl-,` opens the settings.

### Code stage

The Code stage (`5`) reads a checkout's working tree rather than its diff: a tree of its files with
each changed file marked and counted, one file whole with the lines it changed washed as a review washes them and a mark where lines were removed, a
ruler beside it, and beside the editor, marking where in the whole file it changed, and that file's outline with the
changes in it listed. A file whose only change is its line endings is not counted as changed.
Opened from a staged review it reads the index, and from the history a commit, and says so in its
header. `e` opens the file in your editor.

A working-tree file opens in an editor, with Vim's keys unless **Settings → Editor → Vim keys**
turns them off; a commit or the index is read as below. What you type is written as you type, once
you pause; when an agent writes the same file, its change appears in the editor as it lands, lit
for a moment, with your cursor and your unwritten typing kept. If the agent changed the very
characters you were typing, its text is shown beside yours and nothing is written until you choose
**Keep mine** or **Take the agent's**. Undo undoes only your own typing. The gutter marks the lines
the working tree changed. In the editor: `:w` or Ctrl S writes at once, `]c` / `[c` (Alt ] / Alt [
without Vim) step between changes, `/` or Ctrl F searches the file, Alt C comments to an agent on
the selected lines, Ctrl P goes to a file and Ctrl Shift F searches the checkout.

Colours come from tree-sitter grammars, which install the first time you open a file of their
language, after you say yes in the bar over the editor (or without asking, with **Settings →
Editor → Install grammars without asking**). Argus pins 23 languages' grammars to a version and
checksum; for any other language it searches npm and says it found one it has not checked.

The keys below are the read view's, a commit's or the index's:

| Key | Action |
| --- | --- |
| `j` / `k`, arrows | Move through the file's lines |
| Home / End | First or last line |
| `]` / `[` | The next or previous change |
| Ctrl P | Go to a file |
| Ctrl F, or **Search** above the tree | Search the checkout; Enter searches, then reads the match picked. Alt C makes case always matter, Alt W matches whole words only, Alt R reads the query as a regular expression, and a second field takes globs of the files to include (`src/**/*.rs`, `!*.lock`) |
| `o` / `O` | Show or hide the outline |
| `v` / `V` | Start or clear a line-range selection |
| `c` | Comment on the picked lines to an agent in the checkout, as a review comment |
| `e` | Open the file in your editor at the cursor's line |
| `r` / `R` | Read the files again |
| Escape | Close what is over the file — a comment, the search, go to file, a picked range — and stay on the file |
| `b` / `B` | Back to the review or history it was opened from, as it was |

The outline lists what the file defines for Rust, TypeScript and JavaScript, Python and C#. In the
history, `c` reads the commit under the cursor in Code, at the file the cursor is on.