//! The menu that binds a step to a decision (BOARD-FLOWS-8): one choice for each decision a step can be
//! bound to, the ones it is bound to already held off with the word `bound`, and what to say when there
//! is none. Choosing sends `BindStep`, which writes the decision's comment under the step in the flow's
//! source; the step shows it when the pushed flow does.
//!
//! Core says which decisions and whether each is bound (`FlowsLens::choices`), with the id a binding
//! writes: the window does not read an id back out of a label.

use argus_desktop_core::connection::page::{BindChoice, FeatureEdit, FlowDetail};

use crate::kit::menu::item::MenuItem;

/// What choosing an item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binds {
    pub diagram: i64,
    pub step: usize,
    pub decision: i64,
}

impl Binds {
    pub fn edit(&self) -> FeatureEdit {
        FeatureEdit::BindStep { diagram: self.diagram, step: self.step, decision: self.decision }
    }
}

/// The heading and items for the lit step of `flow`; none when no step is lit.
pub fn items(flow: &FlowDetail, choices: &[BindChoice]) -> Option<Vec<MenuItem<Binds>>> {
    let step = flow.step.as_ref()?;
    let mut items = vec![MenuItem::heading(format!("Bind step {} to", step.number))];
    if choices.is_empty() {
        items.push(MenuItem::choice("No decision to bind it to yet.", Binds { diagram: flow.id, step: step.number, decision: 0 }).refused(""));
    }
    items.extend(choices.iter().map(|choice| {
        let item = MenuItem::choice(format!("{}  {}", choice.label, choice.text), Binds { diagram: flow.id, step: step.number, decision: choice.id });
        item.refused_when(choice.bound, "bound")
    }));
    Some(items)
}

#[cfg(test)]
mod tests;
