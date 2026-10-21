// Written by `cargo test -p argus-desktop`, from session.rs: every event a
// page is sent, for `npm run check` to hold to the bindings. Not edited by hand.

import type { Event } from '../../src/bindings/Event';

export const sent = [
{
  "commands": [
    {
      "action": "workspace",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "1",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Go to the Workspace",
      "place": "anywhere"
    },
    {
      "action": "feature",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "2",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Go to the Feature stage",
      "place": "anywhere"
    },
    {
      "action": "checkouts",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "3",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Go to the Checkouts stage",
      "place": "anywhere"
    },
    {
      "action": "console",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "4",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Go to the Console",
      "place": "anywhere"
    },
    {
      "action": "code",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "5",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Read the code of the checkout on screen",
      "place": "anywhere"
    },
    {
      "action": "toggleConsole",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "`",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Show or hide the console",
      "place": "anywhere"
    },
    {
      "action": "consoleFullscreen",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "z",
          "leader": true,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Console fullscreen beside the sidebar, and back",
      "place": "pane"
    },
    {
      "action": "newConsole",
      "chords": [],
      "label": "Start a console shell in the home directory",
      "place": null
    },
    {
      "action": "splitConsole",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "\\",
          "leader": true,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Split the console in two, and back",
      "place": "pane"
    },
    {
      "action": "dockLeft",
      "chords": [],
      "label": "Dock the console on the left",
      "place": null
    },
    {
      "action": "dockRight",
      "chords": [],
      "label": "Dock the console on the right",
      "place": null
    },
    {
      "action": "dockBottom",
      "chords": [],
      "label": "Dock the console at the bottom",
      "place": null
    },
    {
      "action": "otherSide",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "o",
          "leader": true,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Give the keys to the next pane on screen",
      "place": "pane"
    },
    {
      "action": "previousSide",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "O",
          "leader": true,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Give the keys to the previous pane on screen",
      "place": "pane"
    },
    {
      "action": "focusLeft",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "ArrowLeft",
          "leader": true,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Give the keys to the pane on the left",
      "place": "pane"
    },
    {
      "action": "focusRight",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "ArrowRight",
          "leader": true,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Give the keys to the pane on the right",
      "place": "pane"
    },
    {
      "action": "focusUp",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "ArrowUp",
          "leader": true,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Give the keys to the pane above",
      "place": "pane"
    },
    {
      "action": "focusDown",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "ArrowDown",
          "leader": true,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Give the keys to the pane below",
      "place": "pane"
    },
    {
      "action": "movePaneLeft",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "ArrowLeft",
          "leader": true,
          "meta": false,
          "shift": true
        }
      ],
      "label": "Move the pane left, trading places with its neighbour",
      "place": "pane"
    },
    {
      "action": "movePaneRight",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "ArrowRight",
          "leader": true,
          "meta": false,
          "shift": true
        }
      ],
      "label": "Move the pane right, trading places with its neighbour",
      "place": "pane"
    },
    {
      "action": "movePaneUp",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "ArrowUp",
          "leader": true,
          "meta": false,
          "shift": true
        }
      ],
      "label": "Move the pane up, trading places with its neighbour",
      "place": "pane"
    },
    {
      "action": "movePaneDown",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "ArrowDown",
          "leader": true,
          "meta": false,
          "shift": true
        }
      ],
      "label": "Move the pane down, trading places with its neighbour",
      "place": "pane"
    },
    {
      "action": "closeSplit",
      "chords": [],
      "label": "Take the pane out of the grid of panes side by side",
      "place": null
    },
    {
      "action": "compose",
      "chords": [
        {
          "alt": false,
          "ctrl": true,
          "key": "n",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Start a shell or an agent…",
      "place": "anywhere"
    },
    {
      "action": "shortcuts",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "?",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Show the keyboard shortcuts",
      "place": "anywhere"
    },
    {
      "action": "settings",
      "chords": [
        {
          "alt": false,
          "ctrl": true,
          "key": ",",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Open the settings",
      "place": "anywhere"
    },
    {
      "action": "zoomIn",
      "chords": [
        {
          "alt": false,
          "ctrl": true,
          "key": "=",
          "leader": false,
          "meta": false,
          "shift": false
        },
        {
          "alt": false,
          "ctrl": true,
          "key": "+",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Zoom in",
      "place": "window"
    },
    {
      "action": "zoomOut",
      "chords": [
        {
          "alt": false,
          "ctrl": true,
          "key": "-",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Zoom out",
      "place": "window"
    },
    {
      "action": "zoomReset",
      "chords": [
        {
          "alt": false,
          "ctrl": true,
          "key": "0",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Reset the zoom",
      "place": "window"
    },
    {
      "action": "fill",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "f",
          "leader": true,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Fill the window with the pane, and back",
      "place": "pane"
    },
    {
      "action": "close",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "x",
          "leader": true,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Close the pane, ending its program",
      "place": "pane"
    },
    {
      "action": "startShell",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "s",
          "leader": false,
          "meta": false,
          "shift": false
        },
        {
          "alt": false,
          "ctrl": false,
          "key": "S",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Start a shell on this checkout",
      "place": "checkouts"
    },
    {
      "action": "startAgent",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "a",
          "leader": false,
          "meta": false,
          "shift": false
        },
        {
          "alt": false,
          "ctrl": false,
          "key": "A",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Start the default agent on this checkout",
      "place": "checkouts"
    },
    {
      "action": "openReview",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "r",
          "leader": false,
          "meta": false,
          "shift": false
        },
        {
          "alt": false,
          "ctrl": false,
          "key": "R",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Review this checkout's changes",
      "place": "checkouts"
    },
    {
      "action": "openHistory",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "h",
          "leader": false,
          "meta": false,
          "shift": false
        },
        {
          "alt": false,
          "ctrl": false,
          "key": "H",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Read this checkout's history",
      "place": "checkouts"
    },
    {
      "action": "openSessions",
      "chords": [],
      "label": "Recent agent sessions of this repository",
      "place": null
    },
    {
      "action": "fetch",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "f",
          "leader": false,
          "meta": false,
          "shift": false
        },
        {
          "alt": false,
          "ctrl": false,
          "key": "F",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Fetch this checkout",
      "place": "checkouts"
    },
    {
      "action": "pull",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "p",
          "leader": false,
          "meta": false,
          "shift": false
        },
        {
          "alt": false,
          "ctrl": false,
          "key": "P",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Pull this checkout",
      "place": "checkouts"
    },
    {
      "action": "branches",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "b",
          "leader": false,
          "meta": false,
          "shift": false
        },
        {
          "alt": false,
          "ctrl": false,
          "key": "B",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Show or hide the branches no checkout is on",
      "place": "checkouts"
    },
    {
      "action": "files",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "e",
          "leader": false,
          "meta": false,
          "shift": false
        },
        {
          "alt": false,
          "ctrl": false,
          "key": "E",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Show or hide this checkout's files",
      "place": "checkouts"
    },
    {
      "action": "layout",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "v",
          "leader": false,
          "meta": false,
          "shift": false
        },
        {
          "alt": false,
          "ctrl": false,
          "key": "V",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Show the features as a list or a board",
      "place": "feature"
    },
    {
      "action": "folded",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "z",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Show the folded features — held and archived on the list, archived on the board — or fold them",
      "place": "feature"
    },
    {
      "action": "newFeature",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "N",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Write a new feature as one note",
      "place": "feature"
    },
    {
      "action": "extensionPage",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": ">",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Open the extension view as a page",
      "place": "extension"
    },
    {
      "action": "extensionDock",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "<",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Dock the extension view again",
      "place": "extension"
    },
    {
      "action": "linkTicket",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "T",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Link the open feature to a ticket…",
      "place": "outline"
    },
    {
      "action": "reconcile",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "R",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Reconcile the open feature with its tickets…",
      "place": "outline"
    },
    {
      "action": "trackerMap",
      "chords": [
        {
          "alt": false,
          "ctrl": false,
          "key": "M",
          "leader": false,
          "meta": false,
          "shift": false
        }
      ],
      "label": "Edit the repository's tracker map…",
      "place": "outline"
    }
  ],
  "groups": [
    {
      "bindings": [
        {
          "action": "workspace",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "1",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "Workspace"
        },
        {
          "action": "feature",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "2",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "Feature"
        },
        {
          "action": "checkouts",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "3",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "Checkouts"
        },
        {
          "action": "console",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "4",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "Console"
        },
        {
          "action": "code",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "5",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "Code"
        },
        {
          "action": "toggleConsole",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "`",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show or hide the console"
        },
        {
          "action": "compose",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "n",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "start a shell or an agent"
        },
        {
          "action": "closeLayer",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Escape",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "close the menu or dialog"
        },
        {
          "action": "palette",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "search everything"
        },
        {
          "action": "shortcuts",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "?",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "these shortcuts"
        },
        {
          "action": "settings",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": ",",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "settings"
        },
        {
          "action": "views",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": ".",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show or hide the views agents showed"
        }
      ],
      "place": "anywhere",
      "title": "Anywhere"
    },
    {
      "bindings": [
        {
          "action": "leader",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": " ",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "send ctrl-space to the program"
        },
        {
          "action": "fill",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "f",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "fill the window with the pane, and back"
        },
        {
          "action": "workspace",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "1",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "Workspace"
        },
        {
          "action": "feature",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "2",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "Feature"
        },
        {
          "action": "close",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "x",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "close the pane, ending its program"
        },
        {
          "action": "checkouts",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "3",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "Checkouts"
        },
        {
          "action": "console",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "4",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "Console"
        },
        {
          "action": "code",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "5",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "Code"
        },
        {
          "action": "toggleConsole",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "`",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show or hide the console"
        },
        {
          "action": "consoleFullscreen",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "z",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the console fills the window, and back"
        },
        {
          "action": "splitConsole",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "\\",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "split the console in two, and back"
        },
        {
          "action": "otherSide",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "o",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the keys to the next pane on screen"
        },
        {
          "action": "previousSide",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "O",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the keys to the previous pane on screen"
        },
        {
          "action": "focusLeft",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowLeft",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the keys to the pane on the left"
        },
        {
          "action": "focusRight",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowRight",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the keys to the pane on the right"
        },
        {
          "action": "focusUp",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the keys to the pane above"
        },
        {
          "action": "focusDown",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the keys to the pane below"
        },
        {
          "action": "movePaneLeft",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowLeft",
              "leader": true,
              "meta": false,
              "shift": true
            }
          ],
          "label": "move the pane left, trading places"
        },
        {
          "action": "movePaneRight",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowRight",
              "leader": true,
              "meta": false,
              "shift": true
            }
          ],
          "label": "move the pane right, trading places"
        },
        {
          "action": "movePaneUp",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": true,
              "meta": false,
              "shift": true
            }
          ],
          "label": "move the pane up, trading places"
        },
        {
          "action": "movePaneDown",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": true,
              "meta": false,
              "shift": true
            }
          ],
          "label": "move the pane down, trading places"
        },
        {
          "action": "compose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "n",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "start a shell or an agent"
        },
        {
          "action": "openReview",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Tab",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "review the pane's checkout"
        },
        {
          "action": "paste",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "v",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": true,
              "key": "V",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": true,
              "ctrl": false,
              "key": "v",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "paste"
        },
        {
          "action": "pageUp",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "PageUp",
              "leader": false,
              "meta": false,
              "shift": true
            }
          ],
          "label": "back through the history"
        },
        {
          "action": "pageDown",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "PageDown",
              "leader": false,
              "meta": false,
              "shift": true
            }
          ],
          "label": "forward to the live screen"
        },
        {
          "action": "palette",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "k",
              "leader": true,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "search everything"
        },
        {
          "action": "shortcuts",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "?",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "these shortcuts"
        },
        {
          "action": "settings",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": ",",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "settings"
        },
        {
          "action": "views",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "v",
              "leader": true,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show or hide the views agents showed"
        }
      ],
      "place": "pane",
      "title": "In a pane"
    },
    {
      "bindings": [
        {
          "action": "zoomIn",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "=",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": true,
              "key": "+",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "larger"
        },
        {
          "action": "zoomOut",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "-",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "smaller"
        },
        {
          "action": "zoomReset",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "0",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the usual size"
        }
      ],
      "place": "window",
      "title": "Window"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next checkout or pane"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous checkout or pane"
        },
        {
          "action": "startShell",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "s",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "S",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "start a shell on its checkout"
        },
        {
          "action": "startAgent",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "a",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "A",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "start the default agent on its checkout"
        },
        {
          "action": "close",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "x",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "X",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "close the pane, or remove the checkout"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open the checkout's + menu, or type into the pane"
        },
        {
          "action": "openReview",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "r",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "R",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "review its checkout's changes"
        },
        {
          "action": "openHistory",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "h",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "H",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "read its checkout's history"
        },
        {
          "action": "fold",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "z",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "fold its checkout's panes away, or unfold them"
        }
      ],
      "place": "rail",
      "title": "Sidebar"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next feature"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous feature"
        },
        {
          "action": "first",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Home",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "first feature"
        },
        {
          "action": "last",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "End",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "last feature"
        },
        {
          "action": "layout",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "v",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "V",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "list or board"
        },
        {
          "action": "folded",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "z",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show the folded features — held and archived on the list, archived on the board — or fold them"
        },
        {
          "action": "filter",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "/",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "filter by title or number"
        },
        {
          "action": "newFeature",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "N",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "write a new feature as one note"
        }
      ],
      "place": "feature",
      "title": "Feature"
    },
    {
      "bindings": [
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open it"
        },
        {
          "action": "move",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "m",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "M",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "move to…"
        },
        {
          "action": "hold",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "h",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "H",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "hold, or lift its hold"
        },
        {
          "action": "archive",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "A",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "archive, or reopen"
        },
        {
          "action": "goToPane",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "g",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "G",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "go to its agent's pane"
        }
      ],
      "place": "card",
      "title": "Board card"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next row"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous row"
        },
        {
          "action": "first",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Home",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "first row"
        },
        {
          "action": "last",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "End",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "last row"
        },
        {
          "action": "edit",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "e",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "E",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "edit the task, the decision as its record, or the flow as its source"
        },
        {
          "action": "brief",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "b",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "B",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "edit the brief"
        },
        {
          "action": "addTask",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "a",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "A",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "new task, or a flow on the Flows lens"
        },
        {
          "action": "under",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "s",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "S",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "a subtask or sub-decision under it"
        },
        {
          "action": "decide",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "d",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "D",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "raise a decision under the task, or bind the step to one"
        },
        {
          "action": "answer",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "o",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "O",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "answer the open decision in words"
        },
        {
          "action": "withdraw",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "x",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "X",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "withdraw the decision, or restore it"
        },
        {
          "action": "goToPane",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "g",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "G",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "go to its agent's pane"
        },
        {
          "action": "workLens",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "1",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "read it as work"
        },
        {
          "action": "recordLens",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "2",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "read it as a record of its decisions"
        },
        {
          "action": "flowsLens",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "3",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "read its flows"
        },
        {
          "action": "supersede",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "u",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "U",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "supersede the decision with a new one"
        },
        {
          "action": "copyRecord",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "y",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "Y",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "copy the decision as ADR markdown"
        },
        {
          "action": "expand",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "f",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "F",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "expand the flow's diagram"
        },
        {
          "action": "nextFlow",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "l",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowRight",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next flow"
        },
        {
          "action": "previousFlow",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "h",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowLeft",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous flow"
        },
        {
          "action": "linkTicket",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "T",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "link a ticket"
        },
        {
          "action": "openTicket",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "t",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open its ticket in your browser"
        },
        {
          "action": "reconcile",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "R",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "reconcile with its tickets"
        },
        {
          "action": "trackerMap",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "M",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the repository's tracker map"
        }
      ],
      "place": "outline",
      "title": "An open feature"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next step"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous step"
        },
        {
          "action": "nextFlow",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "l",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowRight",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next flow"
        },
        {
          "action": "previousFlow",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "h",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowLeft",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous flow"
        },
        {
          "action": "zoomIn",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "+",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "=",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "zoom the diagram in"
        },
        {
          "action": "zoomOut",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "-",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "zoom the diagram out"
        },
        {
          "action": "zoomReset",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "0",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the diagram at 100%"
        },
        {
          "action": "fit",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "f",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "F",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "fit the diagram to the window"
        },
        {
          "action": "stepsPanel",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "p",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "P",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show or hide the steps"
        },
        {
          "action": "edit",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "e",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "E",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "edit the step's flow as its source"
        },
        {
          "action": "decide",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "d",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "D",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "bind the step to a decision"
        },
        {
          "action": "earlier",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "K",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "move the step up the flow"
        },
        {
          "action": "later",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "J",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "move the step down the flow"
        }
      ],
      "place": "diagram",
      "title": "An expanded diagram"
    },
    {
      "bindings": [
        {
          "action": "save",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": true,
              "shift": false
            }
          ],
          "label": "create, send or save what is written"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "save a one-line field"
        },
        {
          "action": "closeLayer",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Escape",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "discard what was written"
        }
      ],
      "place": "writing",
      "title": "Writing in a field"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next row"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous row"
        },
        {
          "action": "first",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Home",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "first row"
        },
        {
          "action": "last",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "End",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "last row"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open"
        },
        {
          "action": "branches",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "b",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "B",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show branches"
        },
        {
          "action": "filter",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "/",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "filter"
        },
        {
          "action": "fetch",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "f",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "F",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "fetch"
        },
        {
          "action": "pull",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "p",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "P",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "pull"
        },
        {
          "action": "newWorktree",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "n",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "N",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "new worktree"
        },
        {
          "action": "delete",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "D",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "delete"
        },
        {
          "action": "openReview",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "r",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "R",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "review"
        },
        {
          "action": "openHistory",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "h",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "H",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "history"
        },
        {
          "action": "startShell",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "s",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "S",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "start a shell"
        },
        {
          "action": "startAgent",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "a",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "A",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "start the default agent"
        },
        {
          "action": "files",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "e",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "E",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show its files"
        }
      ],
      "place": "checkouts",
      "title": "Checkouts"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next row"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous row"
        },
        {
          "action": "first",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Home",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "first row"
        },
        {
          "action": "last",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "End",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "last row"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open the file in Code, or open or close the folder"
        },
        {
          "action": "openEditor",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "o",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "O",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open the file in your editor"
        },
        {
          "action": "openFolder",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "l",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowRight",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open the folder, or step into it"
        },
        {
          "action": "shutFolder",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "h",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowLeft",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "close the folder, or step out of it"
        },
        {
          "action": "filter",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "/",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "filter the files"
        },
        {
          "action": "closeLayer",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Escape",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "put the files away"
        }
      ],
      "place": "files",
      "title": "A checkout's files"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next line"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous line"
        },
        {
          "action": "first",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Home",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "first line"
        },
        {
          "action": "last",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "End",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "last line"
        },
        {
          "action": "nextFile",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "]",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next change"
        },
        {
          "action": "previousFile",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "[",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous change"
        },
        {
          "action": "filter",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "p",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "go to a file"
        },
        {
          "action": "search",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "f",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "search the checkout"
        },
        {
          "action": "openEditor",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "e",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "E",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open in your editor"
        },
        {
          "action": "pick",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "v",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "V",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "pick a range"
        },
        {
          "action": "comment",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "c",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "C",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "comment to an agent"
        },
        {
          "action": "refresh",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "r",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "R",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "read the files again"
        },
        {
          "action": "toggleOutline",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "o",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "O",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show or hide the outline"
        },
        {
          "action": "goBack",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "b",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "B",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "back to the review or history it was opened from"
        }
      ],
      "place": "code",
      "title": "Code"
    },
    {
      "bindings": [
        {
          "action": "startShell",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "s",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "S",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "start a shell"
        },
        {
          "action": "startAgent",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "a",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "A",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "start the default agent"
        },
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next item"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous item"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "start the highlighted one"
        }
      ],
      "place": "start",
      "title": "A checkout's + menu"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next item"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous item"
        }
      ],
      "place": "menu",
      "title": "A menu"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next line"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous line"
        },
        {
          "action": "first",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Home",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "first line"
        },
        {
          "action": "last",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "End",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "last line"
        },
        {
          "action": "nextFile",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "]",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next file"
        },
        {
          "action": "previousFile",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "[",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous file"
        },
        {
          "action": "pick",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "v",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "V",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "pick a range"
        },
        {
          "action": "comment",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "c",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "C",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "comment"
        },
        {
          "action": "side",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "b",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "B",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "staged or unstaged"
        },
        {
          "action": "split",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "s",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "S",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "split or unified"
        },
        {
          "action": "filter",
          "chords": [
            {
              "alt": false,
              "ctrl": true,
              "key": "p",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "jump to a file"
        },
        {
          "action": "openEditor",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "e",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "E",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open in your editor"
        },
        {
          "action": "unfold",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show more of the lines between hunks"
        },
        {
          "action": "unfoldAll",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "z",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "show all of them"
        },
        {
          "action": "openCode",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "o",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "O",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "read the file in Code, at this line"
        }
      ],
      "place": "review",
      "title": "Review"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": true,
              "key": "n",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next file"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": true,
              "key": "p",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous file"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open the file"
        },
        {
          "action": "closeLayer",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Escape",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "close the jump, keeping the review"
        }
      ],
      "place": "jump",
      "title": "Jumping to a file"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next row"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous row"
        },
        {
          "action": "first",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Home",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "newest"
        },
        {
          "action": "last",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "End",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "oldest"
        },
        {
          "action": "unfold",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "l",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowRight",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open a commit"
        },
        {
          "action": "fold",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "h",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowLeft",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "fold it"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open a commit, or a file as a review"
        },
        {
          "action": "asReview",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "o",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "O",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open as a review"
        },
        {
          "action": "openCode",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "c",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "C",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "read its files in Code"
        },
        {
          "action": "filter",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "/",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "filter commits"
        }
      ],
      "place": "history",
      "title": "History"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next session"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous session"
        },
        {
          "action": "first",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Home",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "first session"
        },
        {
          "action": "last",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "End",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "last session"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "go to its pane, resume it, or read it whole"
        },
        {
          "action": "resume",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "r",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "R",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "resume in a new pane"
        },
        {
          "action": "transcript",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "o",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "O",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open the full transcript"
        },
        {
          "action": "filter",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "/",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "search sessions"
        },
        {
          "action": "filterHarness",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "h",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "H",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "filter by the next harness"
        },
        {
          "action": "filterCheckout",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "c",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "C",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "filter by the next checkout"
        }
      ],
      "place": "sessions",
      "title": "Recent sessions"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "your next turn"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "your previous turn"
        },
        {
          "action": "first",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Home",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "your first turn"
        },
        {
          "action": "last",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "End",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "your last turn"
        },
        {
          "action": "filter",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "/",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": true,
              "key": "f",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "find in the transcript"
        },
        {
          "action": "nextMatch",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "n",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next match"
        },
        {
          "action": "previousMatch",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "N",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous match"
        },
        {
          "action": "copyId",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "y",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "Y",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "copy the session id"
        },
        {
          "action": "resume",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "r",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "R",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "resume in a new pane"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "go to its pane, while it runs"
        }
      ],
      "place": "transcript",
      "title": "A full transcript"
    },
    {
      "bindings": [
        {
          "action": "leave",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "Escape",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "back to the list, filtered"
        },
        {
          "action": "matchCase",
          "chords": [
            {
              "alt": true,
              "ctrl": false,
              "key": "c",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "in Code's search: case always matters"
        },
        {
          "action": "wholeWord",
          "chords": [
            {
              "alt": true,
              "ctrl": false,
              "key": "w",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "in Code's search: whole words only"
        },
        {
          "action": "useRegex",
          "chords": [
            {
              "alt": true,
              "ctrl": false,
              "key": "r",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "in Code's search: a regular expression"
        }
      ],
      "place": "filter",
      "title": "A filter or search field"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next result"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous result"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "run it, or go to it"
        },
        {
          "action": "closeLayer",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Escape",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "close the palette"
        }
      ],
      "place": "palette",
      "title": "Palette"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next row"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous row"
        },
        {
          "action": "first",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Home",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "first row"
        },
        {
          "action": "last",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "End",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "last row"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "the view's default action on the row"
        },
        {
          "action": "extensionPage",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": ">",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open it as a page"
        },
        {
          "action": "extensionDock",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "<",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "dock it again"
        }
      ],
      "place": "extension",
      "title": "An extension's view"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next linked ticket"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous linked ticket"
        },
        {
          "action": "choose",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "link the URL typed"
        },
        {
          "action": "openTicket",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "o",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "O",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "open the ticket in your browser"
        },
        {
          "action": "unlink",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "x",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "X",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "unlink the ticket"
        },
        {
          "action": "trackerMap",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "m",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "M",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "edit the tracker map"
        },
        {
          "action": "approve",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "A",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "approve the map's draft as it reads"
        },
        {
          "action": "discard",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "D",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "turn the map's draft down"
        }
      ],
      "place": "link",
      "title": "Linking a ticket"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "j",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next line"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "k",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous line"
        },
        {
          "action": "tick",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": " ",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "tick the line, or untick it"
        },
        {
          "action": "edit",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "e",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "E",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "edit the line's text before ticking it"
        },
        {
          "action": "carryOut",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "Enter",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "carry out the ticked lines"
        },
        {
          "action": "discard",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "D",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "discard the proposal"
        },
        {
          "action": "goToPane",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "g",
              "leader": false,
              "meta": false,
              "shift": false
            },
            {
              "alt": false,
              "ctrl": false,
              "key": "G",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "go to the agent's pane"
        }
      ],
      "place": "reconcile",
      "title": "Reconciling with the tickets"
    },
    {
      "bindings": [
        {
          "action": "next",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowDown",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "next section"
        },
        {
          "action": "previous",
          "chords": [
            {
              "alt": false,
              "ctrl": false,
              "key": "ArrowUp",
              "leader": false,
              "meta": false,
              "shift": false
            }
          ],
          "label": "previous section"
        }
      ],
      "place": "sheet",
      "title": "The shortcuts sheet"
    }
  ],
  "layers": [
    {
      "layer": "field",
      "places": null,
      "takesFromPane": false
    },
    {
      "layer": "extensionDialog",
      "places": null,
      "takesFromPane": false
    },
    {
      "layer": "settings",
      "places": null,
      "takesFromPane": false
    },
    {
      "layer": "sheet",
      "places": [
        "anywhere",
        "sheet"
      ],
      "takesFromPane": false
    },
    {
      "layer": "addProject",
      "places": null,
      "takesFromPane": true
    },
    {
      "layer": "link",
      "places": [
        "anywhere",
        "link"
      ],
      "takesFromPane": true
    },
    {
      "layer": "reconcile",
      "places": [
        "anywhere",
        "reconcile"
      ],
      "takesFromPane": true
    },
    {
      "layer": "review",
      "places": [
        "anywhere",
        "review"
      ],
      "takesFromPane": true
    },
    {
      "layer": "history",
      "places": [
        "anywhere",
        "history"
      ],
      "takesFromPane": true
    },
    {
      "layer": "transcript",
      "places": [
        "anywhere",
        "transcript"
      ],
      "takesFromPane": true
    },
    {
      "layer": "sessions",
      "places": [
        "anywhere",
        "sessions"
      ],
      "takesFromPane": true
    },
    {
      "layer": "start",
      "places": [
        "anywhere",
        "start"
      ],
      "takesFromPane": false
    },
    {
      "layer": "extension",
      "places": [
        "extension"
      ],
      "takesFromPane": false
    },
    {
      "layer": "rail",
      "places": [
        "anywhere",
        "rail"
      ],
      "takesFromPane": false
    },
    {
      "layer": "menu",
      "places": [
        "anywhere",
        "menu"
      ],
      "takesFromPane": false
    },
    {
      "layer": "capture",
      "places": [
        "anywhere"
      ],
      "takesFromPane": false
    },
    {
      "layer": "diagram",
      "places": [
        "diagram",
        "outline",
        "anywhere"
      ],
      "takesFromPane": false
    },
    {
      "layer": "outline",
      "places": [
        "outline",
        "anywhere"
      ],
      "takesFromPane": false
    },
    {
      "layer": "features",
      "places": [
        "anywhere",
        "feature",
        "card"
      ],
      "takesFromPane": false
    },
    {
      "layer": "files",
      "places": [
        "files"
      ],
      "takesFromPane": false
    },
    {
      "layer": "checkouts",
      "places": [
        "anywhere",
        "checkouts"
      ],
      "takesFromPane": false
    },
    {
      "layer": "code",
      "places": [
        "anywhere",
        "code"
      ],
      "takesFromPane": false
    },
    {
      "layer": "extensionPage",
      "places": [
        "anywhere",
        "extension"
      ],
      "takesFromPane": false
    },
    {
      "layer": "stage",
      "places": [
        "anywhere"
      ],
      "takesFromPane": false
    }
  ],
  "type": "keymap"
},
{
  "alarm": null,
  "chime": true,
  "connection": "connected",
  "detail": null,
  "host": null,
  "type": "status"
},
{
  "bottom": {
    "shown": null,
    "size": 280,
    "tabs": [
      {
        "panel": "console",
        "title": "Console"
      }
    ]
  },
  "floating": [],
  "left": {
    "shown": null,
    "size": 560,
    "tabs": []
  },
  "present": [
    {
      "closes": false,
      "floats": false,
      "kind": "console",
      "panel": "console",
      "stage": true
    }
  ],
  "right": {
    "shown": null,
    "size": 560,
    "tabs": []
  },
  "type": "docks"
},
{
  "beside": null,
  "canSpawn": true,
  "place": "hidden",
  "showing": null,
  "tabs": [],
  "type": "console",
  "typing": null
},
{
  "briefHeight": null,
  "codeOutlineWidth": null,
  "codeTreeWidth": null,
  "filled": false,
  "hosts": [],
  "loaded": true,
  "needsYou": 4,
  "outlineWidth": null,
  "project": {
    "id": 1,
    "name": "src",
    "needsYou": 4,
    "repositories": [
      {
        "checkouts": [
          {
            "branch": "main",
            "clean": true,
            "count": 7,
            "git": "clean",
            "id": 100,
            "kind": "main",
            "name": "main",
            "open": true,
            "panes": [
              {
                "agent": false,
                "id": 1,
                "note": null,
                "on": false,
                "status": "idle",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 2,
                "note": null,
                "on": false,
                "status": "working",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 3,
                "note": null,
                "on": false,
                "status": "waiting",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 4,
                "note": null,
                "on": false,
                "status": "needs-review",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 5,
                "note": null,
                "on": false,
                "status": "done",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 6,
                "note": null,
                "on": false,
                "status": "failed",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 7,
                "note": null,
                "on": false,
                "status": "exited",
                "title": "agent",
                "views": 0
              }
            ],
            "shared": 5,
            "status": "waiting"
          },
          {
            "branch": "topic",
            "clean": true,
            "count": 1,
            "git": "clean",
            "id": 101,
            "kind": "worktree",
            "name": "topic",
            "open": true,
            "panes": [
              {
                "agent": true,
                "id": 20,
                "note": null,
                "on": false,
                "status": "waiting",
                "title": "claude",
                "views": 0
              }
            ],
            "shared": null,
            "status": "waiting"
          }
        ],
        "divider": false,
        "id": 10,
        "name": "argus",
        "open": true,
        "panes": 8,
        "quiet": false,
        "status": "waiting",
        "used": null
      }
    ],
    "running": [
      {
        "agent": false,
        "checkout": "main",
        "id": 1,
        "note": null,
        "repository": "argus",
        "status": "idle",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 2,
        "note": null,
        "repository": "argus",
        "status": "working",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 3,
        "note": null,
        "repository": "argus",
        "status": "waiting",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 4,
        "note": null,
        "repository": "argus",
        "status": "needs-review",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 5,
        "note": null,
        "repository": "argus",
        "status": "done",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 6,
        "note": null,
        "repository": "argus",
        "status": "failed",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 7,
        "note": null,
        "repository": "argus",
        "status": "exited",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "topic",
        "id": 20,
        "note": null,
        "repository": "argus",
        "status": "waiting",
        "title": "claude",
        "used": null
      }
    ]
  },
  "projects": [
    {
      "id": 1,
      "name": "src",
      "needsYou": 4,
      "working": 1
    }
  ],
  "removing": null,
  "repository": 10,
  "row": {
    "beside": [],
    "grid": [
      [
        0
      ]
    ],
    "heights": [
      1000
    ],
    "keyed": 0,
    "minCols": 20,
    "minRows": 6,
    "widths": [
      [
        1000
      ]
    ]
  },
  "shown": null,
  "stage": "feature",
  "templates": [],
  "type": "rail",
  "width": null,
  "working": 1,
  "workspaceShown": false,
  "workspaces": []
},
{
  "active": 1,
  "archived": 1,
  "done": 1,
  "filter": "",
  "folded": 2,
  "held": 1,
  "layout": "list",
  "loaded": true,
  "open": null,
  "peek": {
    "after": [
      {
        "number": 5,
        "slug": "mouse",
        "title": "Mouse selection",
        "waits": false
      },
      {
        "number": 6,
        "slug": "ctrlc",
        "title": "Ctrl-C on Windows",
        "waits": false
      },
      {
        "number": 8,
        "slug": "bump",
        "title": "The ratatui bump",
        "waits": false
      },
      {
        "number": 10,
        "slug": "desktop",
        "title": "Desktop client",
        "waits": false
      }
    ],
    "branch": "main",
    "brief": "Why Trim context injection matters.",
    "held": null,
    "inFlight": [],
    "lastDecided": {
      "aside": null,
      "label": "D1",
      "text": "Name the feature"
    },
    "links": [
      {
        "key": "ENG-88",
        "role": "the work",
        "tracker": "Linear",
        "url": "https://linear.app/x/ENG-88"
      }
    ],
    "needsYou": [
      {
        "aside": null,
        "label": "D2",
        "text": "How much history?"
      },
      {
        "aside": "waiting",
        "label": "◆",
        "text": "agent"
      },
      {
        "aside": "needs-review",
        "label": "◆",
        "text": "agent"
      },
      {
        "aside": "failed",
        "label": "◆",
        "text": "agent"
      }
    ],
    "nextUp": null,
    "number": 7,
    "pane": 3,
    "slug": "trim",
    "standing": "active",
    "title": "Trim context injection"
  },
  "queued": 2,
  "repository": "argus",
  "rows": [
    {
      "after": [],
      "agent": {
        "needsYou": true,
        "pane": 3,
        "status": "waiting",
        "title": "agent"
      },
      "branch": "main",
      "decided": 1,
      "footer": "agent is on it · 1m ago",
      "group": "active",
      "held": null,
      "moves": [
        {
          "disabled": "follows the work",
          "label": "Queued",
          "to": "queued"
        },
        {
          "disabled": "here now",
          "label": "Active",
          "to": "active"
        },
        {
          "disabled": "when its 4 open tasks are done",
          "label": "Done",
          "to": "done"
        },
        {
          "disabled": null,
          "label": "Held…",
          "to": "held"
        },
        {
          "disabled": null,
          "label": "Archived",
          "to": "archived"
        }
      ],
      "next": null,
      "number": 7,
      "open": 1,
      "peeked": true,
      "slug": "trim",
      "standing": "active",
      "tasksDone": 3,
      "tasksTotal": 7,
      "title": "Trim context injection"
    },
    {
      "after": [
        7
      ],
      "agent": null,
      "branch": null,
      "decided": 0,
      "footer": "after #7 · 1m ago",
      "group": "queued",
      "held": null,
      "moves": [
        {
          "disabled": "here now",
          "label": "Queued",
          "to": "queued"
        },
        {
          "disabled": "when a task starts or an agent arrives",
          "label": "Active",
          "to": "active"
        },
        {
          "disabled": "when its 5 open tasks are done",
          "label": "Done",
          "to": "done"
        },
        {
          "disabled": null,
          "label": "Held…",
          "to": "held"
        },
        {
          "disabled": null,
          "label": "Archived",
          "to": "archived"
        }
      ],
      "next": null,
      "number": 8,
      "open": 0,
      "peeked": false,
      "slug": "bump",
      "standing": "queued",
      "tasksDone": 0,
      "tasksTotal": 5,
      "title": "The ratatui bump"
    },
    {
      "after": [],
      "agent": null,
      "branch": null,
      "decided": 0,
      "footer": "written down, nothing in it yet · 1m ago",
      "group": "queued",
      "held": null,
      "moves": [
        {
          "disabled": "here now",
          "label": "Queued",
          "to": "queued"
        },
        {
          "disabled": "when a task starts or an agent arrives",
          "label": "Active",
          "to": "active"
        },
        {
          "disabled": "when it has tasks and every one is done",
          "label": "Done",
          "to": "done"
        },
        {
          "disabled": null,
          "label": "Held…",
          "to": "held"
        },
        {
          "disabled": null,
          "label": "Archived",
          "to": "archived"
        }
      ],
      "next": null,
      "number": 10,
      "open": 0,
      "peeked": false,
      "slug": "desktop",
      "standing": "new",
      "tasksDone": 0,
      "tasksTotal": 0,
      "title": "Desktop client"
    },
    {
      "after": [],
      "agent": null,
      "branch": null,
      "decided": 0,
      "footer": "every task done · 1m ago",
      "group": "done",
      "held": null,
      "moves": [
        {
          "disabled": "follows the work",
          "label": "Queued",
          "to": "queued"
        },
        {
          "disabled": "when a task starts or an agent arrives",
          "label": "Active",
          "to": "active"
        },
        {
          "disabled": "here now",
          "label": "Done",
          "to": "done"
        },
        {
          "disabled": null,
          "label": "Held…",
          "to": "held"
        },
        {
          "disabled": null,
          "label": "Archived",
          "to": "archived"
        }
      ],
      "next": null,
      "number": 6,
      "open": 0,
      "peeked": false,
      "slug": "ctrlc",
      "standing": "done",
      "tasksDone": 4,
      "tasksTotal": 4,
      "title": "Ctrl-C on Windows"
    }
  ],
  "showFolded": false,
  "type": "features"
},
{
  "canPress": true,
  "panels": [],
  "type": "savedViews",
  "views": []
},
{
  "branches": 0,
  "branchesShown": false,
  "checkouts": 2,
  "confirm": null,
  "detail": {
    "changes": "no changes",
    "name": "main",
    "path": "/src/main",
    "upstream": "none"
  },
  "files": null,
  "filesOffered": true,
  "filter": "",
  "main": null,
  "repository": "argus",
  "rows": [
    {
      "branch": "main",
      "checkout": 100,
      "clean": true,
      "feature": 7,
      "git": "clean",
      "key": "c100",
      "last": null,
      "name": "main",
      "note": null,
      "primary": true,
      "running": [
        {
          "agent": false,
          "status": "idle"
        },
        {
          "agent": true,
          "status": "working"
        },
        {
          "agent": true,
          "status": "waiting"
        },
        {
          "agent": true,
          "status": "needs-review"
        },
        {
          "agent": true,
          "status": "done"
        },
        {
          "agent": true,
          "status": "failed"
        },
        {
          "agent": true,
          "status": "exited"
        }
      ],
      "shared": 5,
      "viewed": true
    },
    {
      "branch": "topic",
      "checkout": 101,
      "clean": true,
      "feature": null,
      "git": "clean",
      "key": "c101",
      "last": null,
      "name": "topic",
      "note": null,
      "primary": false,
      "running": [
        {
          "agent": true,
          "status": "waiting"
        }
      ],
      "shared": null,
      "viewed": false
    }
  ],
  "selected": "c100",
  "type": "checkouts"
},
{
  "briefHeight": null,
  "codeOutlineWidth": null,
  "codeTreeWidth": null,
  "filled": false,
  "hosts": [],
  "loaded": true,
  "needsYou": 4,
  "outlineWidth": null,
  "project": {
    "id": 1,
    "name": "src",
    "needsYou": 4,
    "repositories": [
      {
        "checkouts": [
          {
            "branch": "main",
            "clean": true,
            "count": 7,
            "git": "clean",
            "id": 100,
            "kind": "main",
            "name": "main",
            "open": true,
            "panes": [
              {
                "agent": false,
                "id": 1,
                "note": null,
                "on": false,
                "status": "idle",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 2,
                "note": null,
                "on": false,
                "status": "working",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 3,
                "note": null,
                "on": false,
                "status": "waiting",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 4,
                "note": null,
                "on": false,
                "status": "needs-review",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 5,
                "note": null,
                "on": false,
                "status": "done",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 6,
                "note": null,
                "on": false,
                "status": "failed",
                "title": "agent",
                "views": 0
              },
              {
                "agent": true,
                "id": 7,
                "note": null,
                "on": false,
                "status": "exited",
                "title": "agent",
                "views": 0
              }
            ],
            "shared": 5,
            "status": "waiting"
          },
          {
            "branch": "topic",
            "clean": true,
            "count": 1,
            "git": "clean",
            "id": 101,
            "kind": "worktree",
            "name": "topic",
            "open": true,
            "panes": [
              {
                "agent": true,
                "id": 20,
                "note": null,
                "on": false,
                "status": "waiting",
                "title": "claude",
                "views": 0
              }
            ],
            "shared": null,
            "status": "waiting"
          }
        ],
        "divider": false,
        "id": 10,
        "name": "argus",
        "open": true,
        "panes": 8,
        "quiet": false,
        "status": "waiting",
        "used": null
      }
    ],
    "running": [
      {
        "agent": false,
        "checkout": "main",
        "id": 1,
        "note": null,
        "repository": "argus",
        "status": "idle",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 2,
        "note": null,
        "repository": "argus",
        "status": "working",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 3,
        "note": null,
        "repository": "argus",
        "status": "waiting",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 4,
        "note": null,
        "repository": "argus",
        "status": "needs-review",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 5,
        "note": null,
        "repository": "argus",
        "status": "done",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 6,
        "note": null,
        "repository": "argus",
        "status": "failed",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "main",
        "id": 7,
        "note": null,
        "repository": "argus",
        "status": "exited",
        "title": "agent",
        "used": null
      },
      {
        "agent": true,
        "checkout": "topic",
        "id": 20,
        "note": null,
        "repository": "argus",
        "status": "waiting",
        "title": "claude",
        "used": null
      }
    ]
  },
  "projects": [
    {
      "id": 1,
      "name": "src",
      "needsYou": 4,
      "working": 1
    }
  ],
  "removing": null,
  "repository": 10,
  "row": {
    "beside": [],
    "grid": [
      [
        0
      ]
    ],
    "heights": [
      1000
    ],
    "keyed": 0,
    "minCols": 20,
    "minRows": 6,
    "widths": [
      [
        1000
      ]
    ]
  },
  "shown": null,
  "stage": "checkouts",
  "templates": [],
  "type": "rail",
  "width": null,
  "working": 1,
  "workspaceShown": false,
  "workspaces": []
},
{
  "briefHeight": null,
  "codeOutlineWidth": null,
  "codeTreeWidth": null,
  "filled": false,
  "hosts": [],
  "loaded": true,
  "needsYou": 0,
  "outlineWidth": null,
  "project": {
    "id": 1,
    "name": "src",
    "needsYou": 0,
    "repositories": [
      {
        "checkouts": [
          {
            "branch": "main",
            "clean": true,
            "count": 2,
            "git": "clean",
            "id": 100,
            "kind": "main",
            "name": "main",
            "open": true,
            "panes": [
              {
                "agent": true,
                "id": 1,
                "note": null,
                "on": false,
                "status": "working",
                "title": "claude",
                "views": 0
              },
              {
                "agent": false,
                "id": 2,
                "note": null,
                "on": false,
                "status": "idle",
                "title": "zsh",
                "views": 0
              }
            ],
            "shared": null,
            "status": "working"
          }
        ],
        "divider": false,
        "id": 10,
        "name": "argus",
        "open": true,
        "panes": 2,
        "quiet": false,
        "status": "working",
        "used": null
      }
    ],
    "running": [
      {
        "agent": true,
        "checkout": "main",
        "id": 1,
        "note": null,
        "repository": "argus",
        "status": "working",
        "title": "claude",
        "used": null
      },
      {
        "agent": false,
        "checkout": "main",
        "id": 2,
        "note": null,
        "repository": "argus",
        "status": "idle",
        "title": "zsh",
        "used": null
      }
    ]
  },
  "projects": [
    {
      "id": 1,
      "name": "src",
      "needsYou": 0,
      "working": 1
    },
    {
      "id": 2,
      "name": "work",
      "needsYou": 0,
      "working": 0
    }
  ],
  "removing": null,
  "repository": 10,
  "row": {
    "beside": [],
    "grid": [
      [
        0
      ]
    ],
    "heights": [
      1000
    ],
    "keyed": 0,
    "minCols": 20,
    "minRows": 6,
    "widths": [
      [
        1000
      ]
    ]
  },
  "shown": null,
  "stage": "feature",
  "templates": [],
  "type": "rail",
  "width": null,
  "working": 1,
  "workspaceShown": false,
  "workspaces": []
},
{
  "active": 1,
  "archived": 1,
  "done": 1,
  "filter": "",
  "folded": 2,
  "held": 1,
  "layout": "list",
  "loaded": true,
  "open": {
    "agent": {
      "needsYou": false,
      "pane": 1,
      "status": "working",
      "title": "claude"
    },
    "branch": "main",
    "brief": "Why Trim context injection matters.",
    "changes": null,
    "decided": 1,
    "decision": {
      "adr": "# D1 Name the feature\n\nStatus: accepted\n",
      "because": null,
      "by": "by you",
      "carriedOutBy": [],
      "chose": "Name the feature",
      "consequences": [],
      "considered": [],
      "context": null,
      "detail": null,
      "document": "Name the feature\n\n## Context\n\n\n## Decision\n\n\n## Why\n\n\n## Consequences\n\n\n## Considered",
      "id": 1,
      "label": "D1",
      "options": [],
      "over": null,
      "raisedUnder": null,
      "state": "accepted",
      "supersededBy": null,
      "withdrawn": false
    },
    "flows": null,
    "lens": "work",
    "links": [
      {
        "key": "ENG-88",
        "role": "the work",
        "tracker": "Linear",
        "url": "https://linear.app/x/ENG-88"
      }
    ],
    "loaded": false,
    "number": 7,
    "open": 1,
    "reconcile": null,
    "reconciled": null,
    "record": null,
    "rows": [
      {
        "at": {
          "id": 1,
          "kind": "decision"
        },
        "changed": false,
        "depth": 0,
        "key": null,
        "label": "D1",
        "on": true,
        "state": "accepted",
        "text": "Name the feature",
        "waitsOn": null
      },
      {
        "at": {
          "id": 2,
          "kind": "decision"
        },
        "changed": false,
        "depth": 0,
        "key": null,
        "label": "D2",
        "on": false,
        "state": "open",
        "text": "How much history?",
        "waitsOn": null
      }
    ],
    "slug": "trim",
    "standing": "active",
    "task": null,
    "tasksDone": 3,
    "tasksTotal": 7,
    "title": "Trim context injection",
    "views": []
  },
  "peek": {
    "after": [
      {
        "number": 5,
        "slug": "mouse",
        "title": "Mouse selection",
        "waits": false
      },
      {
        "number": 6,
        "slug": "ctrlc",
        "title": "Ctrl-C on Windows",
        "waits": false
      },
      {
        "number": 8,
        "slug": "bump",
        "title": "The ratatui bump",
        "waits": false
      },
      {
        "number": 10,
        "slug": "desktop",
        "title": "Desktop client",
        "waits": false
      }
    ],
    "branch": "main",
    "brief": "Why Trim context injection matters.",
    "held": null,
    "inFlight": [],
    "lastDecided": {
      "aside": null,
      "label": "D1",
      "text": "Name the feature"
    },
    "links": [
      {
        "key": "ENG-88",
        "role": "the work",
        "tracker": "Linear",
        "url": "https://linear.app/x/ENG-88"
      }
    ],
    "needsYou": [
      {
        "aside": null,
        "label": "D2",
        "text": "How much history?"
      }
    ],
    "nextUp": null,
    "number": 7,
    "pane": 1,
    "slug": "trim",
    "standing": "active",
    "title": "Trim context injection"
  },
  "queued": 2,
  "repository": "argus",
  "rows": [
    {
      "after": [],
      "agent": {
        "needsYou": false,
        "pane": 1,
        "status": "working",
        "title": "claude"
      },
      "branch": "main",
      "decided": 1,
      "footer": "claude is on it · 1m ago",
      "group": "active",
      "held": null,
      "moves": [
        {
          "disabled": "follows the work",
          "label": "Queued",
          "to": "queued"
        },
        {
          "disabled": "here now",
          "label": "Active",
          "to": "active"
        },
        {
          "disabled": "when its 4 open tasks are done",
          "label": "Done",
          "to": "done"
        },
        {
          "disabled": null,
          "label": "Held…",
          "to": "held"
        },
        {
          "disabled": null,
          "label": "Archived",
          "to": "archived"
        }
      ],
      "next": null,
      "number": 7,
      "open": 1,
      "peeked": true,
      "slug": "trim",
      "standing": "active",
      "tasksDone": 3,
      "tasksTotal": 7,
      "title": "Trim context injection"
    },
    {
      "after": [
        7
      ],
      "agent": null,
      "branch": null,
      "decided": 0,
      "footer": "after #7 · 1m ago",
      "group": "queued",
      "held": null,
      "moves": [
        {
          "disabled": "here now",
          "label": "Queued",
          "to": "queued"
        },
        {
          "disabled": "when a task starts or an agent arrives",
          "label": "Active",
          "to": "active"
        },
        {
          "disabled": "when its 5 open tasks are done",
          "label": "Done",
          "to": "done"
        },
        {
          "disabled": null,
          "label": "Held…",
          "to": "held"
        },
        {
          "disabled": null,
          "label": "Archived",
          "to": "archived"
        }
      ],
      "next": null,
      "number": 8,
      "open": 0,
      "peeked": false,
      "slug": "bump",
      "standing": "queued",
      "tasksDone": 0,
      "tasksTotal": 5,
      "title": "The ratatui bump"
    },
    {
      "after": [],
      "agent": null,
      "branch": null,
      "decided": 0,
      "footer": "written down, nothing in it yet · 1m ago",
      "group": "queued",
      "held": null,
      "moves": [
        {
          "disabled": "here now",
          "label": "Queued",
          "to": "queued"
        },
        {
          "disabled": "when a task starts or an agent arrives",
          "label": "Active",
          "to": "active"
        },
        {
          "disabled": "when it has tasks and every one is done",
          "label": "Done",
          "to": "done"
        },
        {
          "disabled": null,
          "label": "Held…",
          "to": "held"
        },
        {
          "disabled": null,
          "label": "Archived",
          "to": "archived"
        }
      ],
      "next": null,
      "number": 10,
      "open": 0,
      "peeked": false,
      "slug": "desktop",
      "standing": "new",
      "tasksDone": 0,
      "tasksTotal": 0,
      "title": "Desktop client"
    },
    {
      "after": [],
      "agent": null,
      "branch": null,
      "decided": 0,
      "footer": "every task done · 1m ago",
      "group": "done",
      "held": null,
      "moves": [
        {
          "disabled": "follows the work",
          "label": "Queued",
          "to": "queued"
        },
        {
          "disabled": "when a task starts or an agent arrives",
          "label": "Active",
          "to": "active"
        },
        {
          "disabled": "here now",
          "label": "Done",
          "to": "done"
        },
        {
          "disabled": null,
          "label": "Held…",
          "to": "held"
        },
        {
          "disabled": null,
          "label": "Archived",
          "to": "archived"
        }
      ],
      "next": null,
      "number": 6,
      "open": 0,
      "peeked": false,
      "slug": "ctrlc",
      "standing": "done",
      "tasksDone": 4,
      "tasksTotal": 4,
      "title": "Ctrl-C on Windows"
    }
  ],
  "showFolded": false,
  "type": "features"
},
{
  "blocked": "a feature has to have a title",
  "brief": null,
  "creating": false,
  "decisions": [],
  "note": "",
  "open": true,
  "repository": "argus",
  "tasks": [],
  "title": "",
  "type": "capture",
  "unread": []
},
{
  "editingMap": null,
  "linked": [
    {
      "link": {
        "key": "ENG-88",
        "role": "the work",
        "tracker": "Linear",
        "url": "https://linear.app/x/ENG-88"
      },
      "on": true
    }
  ],
  "map": {
    "body": "",
    "draft": null,
    "draftBy": null
  },
  "number": 7,
  "open": true,
  "pending": false,
  "read": null,
  "refusal": null,
  "role": "",
  "title": "Trim context injection",
  "type": "link",
  "url": ""
},
{
  "agents": [
    {
      "chosen": true,
      "last": false,
      "pane": 1,
      "title": "main · claude #1"
    }
  ],
  "ago": null,
  "asked": "",
  "by": null,
  "groups": [],
  "noticed": [],
  "number": 7,
  "open": true,
  "reconciled": null,
  "selected": null,
  "stage": "none",
  "ticked": 0,
  "tickets": "Linear ENG-88",
  "title": "Trim context injection",
  "total": 0,
  "type": "reconcile"
},
{
  "briefHeight": null,
  "codeOutlineWidth": null,
  "codeTreeWidth": null,
  "filled": false,
  "hosts": [],
  "loaded": true,
  "needsYou": 0,
  "outlineWidth": null,
  "project": {
    "id": 1,
    "name": "src",
    "needsYou": 0,
    "repositories": [
      {
        "checkouts": [
          {
            "branch": "main",
            "clean": true,
            "count": 2,
            "git": "clean",
            "id": 100,
            "kind": "main",
            "name": "main",
            "open": true,
            "panes": [
              {
                "agent": true,
                "id": 1,
                "note": null,
                "on": false,
                "status": "working",
                "title": "claude",
                "views": 0
              },
              {
                "agent": false,
                "id": 2,
                "note": null,
                "on": false,
                "status": "idle",
                "title": "zsh",
                "views": 0
              }
            ],
            "shared": null,
            "status": "working"
          }
        ],
        "divider": false,
        "id": 10,
        "name": "argus",
        "open": true,
        "panes": 2,
        "quiet": false,
        "status": "working",
        "used": null
      }
    ],
    "running": [
      {
        "agent": true,
        "checkout": "main",
        "id": 1,
        "note": null,
        "repository": "argus",
        "status": "working",
        "title": "claude",
        "used": null
      },
      {
        "agent": false,
        "checkout": "main",
        "id": 2,
        "note": null,
        "repository": "argus",
        "status": "idle",
        "title": "zsh",
        "used": null
      }
    ]
  },
  "projects": [
    {
      "id": 1,
      "name": "src",
      "needsYou": 0,
      "working": 1
    },
    {
      "id": 2,
      "name": "work",
      "needsYou": 0,
      "working": 0
    }
  ],
  "removing": null,
  "repository": 10,
  "row": {
    "beside": [],
    "grid": [
      [
        0
      ]
    ],
    "heights": [
      1000
    ],
    "keyed": 0,
    "minCols": 20,
    "minRows": 6,
    "widths": [
      [
        1000
      ]
    ]
  },
  "shown": null,
  "stage": "workspace",
  "templates": [],
  "type": "rail",
  "width": null,
  "working": 1,
  "workspaceShown": true,
  "workspaces": []
},
{
  "active": 0,
  "archived": 0,
  "done": 0,
  "filter": "",
  "folded": 0,
  "held": 0,
  "layout": "list",
  "loaded": false,
  "open": null,
  "peek": null,
  "queued": 0,
  "repository": "argus",
  "rows": [],
  "showFolded": false,
  "type": "features"
},
{
  "checkout": null,
  "checkouts": [],
  "empty": null,
  "harness": null,
  "harnesses": [],
  "loading": true,
  "open": true,
  "preview": null,
  "reader": null,
  "repository": "argus",
  "rows": [],
  "search": "",
  "type": "sessions"
},
{
  "adding": false,
  "alwaysSkipped": [],
  "blocked": "pick a folder",
  "browsing": true,
  "defaultName": "",
  "depth": null,
  "entries": [],
  "folder": "",
  "kind": "folder",
  "listingError": null,
  "name": "",
  "open": true,
  "parent": null,
  "path": null,
  "previewing": false,
  "refusal": null,
  "repositories": [],
  "skip": "",
  "target": null,
  "type": "addProject"
},
{
  "added": 0,
  "base": "unstaged",
  "branch": "main",
  "checkout": 100,
  "comment": null,
  "commit": null,
  "empty": null,
  "file": null,
  "files": [],
  "jump": null,
  "loading": true,
  "name": "main",
  "open": true,
  "pick": null,
  "removed": 0,
  "rows": [],
  "split": false,
  "staged": null,
  "type": "review",
  "unstaged": null
},
{
  "branch": "main",
  "commit": null,
  "diff": [],
  "diffEmpty": null,
  "diffLoading": false,
  "empty": null,
  "files": 0,
  "filter": "",
  "loading": true,
  "name": "main",
  "open": true,
  "path": null,
  "rows": [],
  "type": "history"
},
{
  "dialog": {
    "answers": null,
    "kind": {
      "confirm": {
        "message": "sure?",
        "yes": null
      }
    },
    "title": "Restart"
  },
  "extension": "docker",
  "id": 9,
  "open": true,
  "prompted": false,
  "type": "extensionDialog",
  "waiting": 0
},
{
  "backdrops": [
    {
      "backdrop": "mica",
      "unavailable": "Mica needs Windows 11"
    }
  ],
  "chosen": {
    "backdrop": "clear",
    "editor": "floating",
    "editorCommand": "",
    "featureLayout": "list",
    "font": "",
    "notify": true,
    "notifyInBackground": true,
    "opacity": 100,
    "savedLinks": "dialog",
    "theme": "system",
    "uiFont": "",
    "zoom": 100
  },
  "fonts": [
    {
      "mono": true,
      "name": "Cascadia Mono"
    }
  ],
  "minOpacity": 30,
  "type": "settings",
  "version": "0.0.0"
}
] satisfies Event[];
