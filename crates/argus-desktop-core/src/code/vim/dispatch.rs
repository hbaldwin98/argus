//! How a key typed in normal, visual or operator-pending state is read:
//! `langmap` first, then a user mapping, then the command parser, which
//! either wants more keys, refuses them, or has a command to run.

use super::command::{self, Parse};
use super::key::{Code, Key};
use super::maps::{self, Hit};
use super::{Shared, Vim};

impl Vim {
    pub(super) fn command_key(&mut self, sh: &mut Shared, key: Key) -> bool {
        // `langmap` says what a typed character stands for, except after
        // `f` or `r`, which want the character itself.
        let key = match key.typed().or(key.ctrl_char()).and_then(|c| sh.options.keymap.get(&c)) {
            Some(&to) if !command::wants_literal(&self.pending, self.is_visual()) => Key { code: Code::Char(to), ..key },
            _ => key,
        };
        if self.remap {
            if let Some(handled) = self.mapped(sh, key) {
                return handled;
            }
        }
        self.pending.push(key);
        match command::parse(&self.pending, self.is_visual()) {
            Parse::Need => true,
            Parse::Bad => {
                // A chord or a key of the host's, met at the start of a
                // command, is the host's; any other key that begins nothing
                // is swallowed.
                let host = self.pending.len() == 1 && (key.ctrl || key.code == Code::Tab);
                self.pending.clear();
                !host
            }
            Parse::Done(cmd) => {
                self.pending.clear();
                self.run(sh, cmd);
                true
            }
        }
    }

    /// Whether a mapping takes the key: none, or whether the key is part of
    /// one. A mapping's keys read like a command's, so the count and the
    /// register before them are left in `pending` for the keys it types.
    fn mapped(&mut self, sh: &mut Shared, key: Key) -> Option<bool> {
        let mut keys = self.pending.clone();
        keys.push(key);
        let skip = command::prefix_len(&keys);
        if skip >= keys.len() {
            return None;
        }
        let mode = if self.is_visual() {
            maps::Mode::Visual
        } else if command::operator_pending(&keys) {
            maps::Mode::OperatorPending
        } else {
            maps::Mode::Normal
        };
        match sh.maps.lookup(&keys[skip..], mode) {
            Hit::Full(map) => {
                let map = map.clone();
                self.pending.truncate(skip);
                self.expand(sh, &map);
                Some(true)
            }
            Hit::Partial if !matches!(command::parse(&keys, self.is_visual()), Parse::Done(_)) => {
                self.pending = keys;
                Some(true)
            }
            _ => None,
        }
    }
}
