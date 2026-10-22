//! Everything the editors share: the registers, the last `f` and `t`, the
//! search, the macros, the options, the mappings and the lines the prompts
//! remember. The window holds one and passes it to every `Vim::key`, so a
//! yank in one file is pasted in another and `n` goes on in the next
//! (EDITOR-VIM-17).

use super::macros::Macros;
use super::maps::Maps;
use super::motion::Find;
use super::options::Options;
use super::recall::Recall;
use super::registers::Registers;
use super::search::Search;

#[derive(Debug, Default)]
pub struct Shared {
    pub regs: Registers,
    /// The last `f`, `F`, `t` or `T`, which `;` and `,` repeat.
    pub find: Option<(Find, char)>,
    pub search: Search,
    pub macros: Macros,
    pub options: Options,
    pub maps: Maps,
    pub search_history: Recall,
    pub ex_history: Recall,
}

impl Shared {
    pub fn with_regs(regs: Registers) -> Shared {
        Shared { regs, ..Shared::default() }
    }
}
