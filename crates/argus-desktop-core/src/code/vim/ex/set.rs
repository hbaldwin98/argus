//! `:set`, `:setlocal` and `:setglobal`: read or write an option. `name?`
//! shows it, `name!` flips a boolean, `noname` turns one off, `name=value`
//! gives it a value, and a bare `name` shows a number or text option and
//! sets a boolean. An option that is not one of the library's five is an
//! error, which is why `:set number` and `:set ignorecase` say so.

use crate::code::vim::options::{is_boolean, Given, Scope, Value};

use super::range::Parsed;
use super::super::{Shared, Vim};

impl Vim {
    pub(super) fn ex_set(&mut self, sh: &mut Shared, p: &Parsed, which: &str, input: &str) {
        let scope = match which {
            "setlocal" => Scope::Local,
            "setglobal" => Scope::Global,
            _ => Scope::Both,
        };
        let args = p.args();
        let Some(first) = args.first() else {
            self.message = Some(format!("Invalid mapping: {input}"));
            return;
        };
        let (mut name, mut value) = match first.split_once('=') {
            Some((n, v)) => (n.to_owned(), Some(v.to_owned())),
            None => ((*first).to_owned(), None),
        };
        let (mut force_get, mut force_toggle) = (false, false);
        if name.ends_with('?') {
            if value.as_deref().is_some_and(|v| !v.is_empty()) {
                self.message = Some(format!("Trailing characters: {}", p.arg.as_deref().unwrap_or_default()));
                return;
            }
            name.pop();
            force_get = true;
        } else if name.ends_with('!') {
            name.pop();
            force_toggle = true;
        }
        let mut off = false;
        if value.is_none() && name.starts_with("no") {
            name.drain(..2);
            off = true;
        }
        let boolean = is_boolean(&name);
        let current = sh.options.get(&name, scope, &self.locals);
        if boolean && force_toggle {
            let now = matches!(current, Ok(Some(Value::Bool(true))));
            off = now;
            value = None;
        }
        let given = match (off, value) {
            (true, _) => Given::Off,
            (false, Some(v)) => Given::Text(v),
            (false, None) => Given::Unset,
        };
        if (!boolean && given == Given::Unset) || force_get {
            self.message = Some(match current {
                Err(e) => e,
                Ok(Some(Value::Bool(b))) => format!(" {}{name}", if b { "" } else { "no" }),
                Ok(Some(v)) => format!("  {name}={v}"),
                Ok(None) => format!("  {name}=undefined"),
            });
            return;
        }
        if let Err(e) = sh.options.set(&name, given, scope, &mut self.locals) {
            self.message = Some(e);
        }
    }
}

#[cfg(test)]
mod tests;
