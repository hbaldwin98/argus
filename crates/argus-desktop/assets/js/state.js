// What the page holds: the last of each event Rust sent, and the little
// view state that is the page's own — a menu open, a filter typed.
export const state = {
    status: { connection: 'connecting', detail: null, alarm: null },
    rail: {
        projects: [],
        project: null,
        needsYou: 0,
        working: 0,
        shown: null,
        loaded: false,
        repository: null,
        stage: 'workspace',
        filled: false,
    },
    features: null,
    keymap: [],
    menuOpen: false,
    filter: '',
    archivedOpen: false,
};
