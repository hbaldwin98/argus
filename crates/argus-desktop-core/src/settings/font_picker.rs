//! What a font field's list does, with no widget in it: a family typed or picked from the installed
//! ones, the list narrowing as it is typed, and when a name is chosen.
//!
//! A name is chosen when it is picked, on Enter or when the field is left, never half-typed, so a
//! name in progress cannot redraw the window in the fallback. The page's own picker
//! (`settings/font_picker.ts`) is what this was written from; a window draws the rows and says the
//! keys and the text, and this answers which row is active and which family, if any, was chosen.

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// A family to pick, drawn in its own face; the empty family is the default's row.
    Family { family: String, label: String },
    /// A line that cannot be picked.
    Note(String),
}

pub struct FontPicker {
    families: Vec<String>,
    chosen: String,
    /// What an empty field draws, which names the list's first row.
    default_name: String,
    open: bool,
    /// Whether the list is narrowed by `text`, rather than shown whole around the chosen name.
    typed: bool,
    text: String,
    active: usize,
}

impl FontPicker {
    pub fn new(default_name: &str) -> Self {
        Self { families: Vec::new(), chosen: String::new(), default_name: default_name.to_owned(), open: false, typed: false, text: String::new(), active: 0 }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn chosen(&self) -> &str {
        &self.chosen
    }

    pub fn default_name(&self) -> &str {
        &self.default_name
    }

    /// What the choices now say: the families to offer and the one chosen.
    pub fn update(&mut self, families: Vec<String>, chosen: &str) {
        self.chosen = chosen.to_owned();
        self.families = families;
    }

    /// A theme changed what an empty field draws.
    pub fn set_default_name(&mut self, name: &str) {
        if self.default_name != name {
            self.default_name = name.to_owned();
        }
    }

    /// The rows the list shows now. With nothing typed the default's row leads and every family
    /// follows; typing keeps the families that contain it, without regard to case.
    pub fn rows(&self) -> Vec<Row> {
        let query = if self.typed { self.text.trim().to_lowercase() } else { String::new() };
        let matching: Vec<&String> = self.families.iter().filter(|family| family.to_lowercase().contains(&query)).collect();
        let mut rows = Vec::new();
        if query.is_empty() {
            rows.push(Row::Family { family: String::new(), label: self.default_name.clone() });
        }
        rows.extend(matching.iter().map(|family| Row::Family { family: (*family).clone(), label: (*family).clone() }));
        if self.families.is_empty() {
            rows.push(Row::Note("Reading the installed fonts\u{2026}".to_owned()));
        } else if matching.is_empty() {
            rows.push(Row::Note("None installed matches; Enter uses it as typed".to_owned()));
        }
        rows
    }

    /// The row that Enter would pick, held inside the list.
    pub fn active(&self) -> usize {
        self.active.min(self.rows().len().saturating_sub(1))
    }

    /// Opens the list on the chosen family, so it starts where the person is. One already open stays.
    pub fn open(&mut self) {
        if self.open {
            return;
        }
        self.typed = false;
        self.open = true;
        let at = self.rows().iter().position(|row| matches!(row, Row::Family { family, .. } if *family == self.chosen));
        self.move_to(at.unwrap_or(0));
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// The field's text changed by typing: the list opens if it was not, narrows, and starts on its first row.
    pub fn typed(&mut self, text: &str) {
        if !self.open {
            self.open();
        }
        self.typed = true;
        self.text = text.to_owned();
        self.active = 0;
    }

    /// Down or Up: opens a closed list, else moves one row, stopping at the first and last family.
    pub fn step(&mut self, down: bool) {
        if !self.open {
            self.open();
            return;
        }
        let at = self.active();
        self.move_to(if down { at + 1 } else { at.saturating_sub(1) });
    }

    /// The nearest choosable row to `to`, so a note is never the active one.
    fn move_to(&mut self, to: usize) {
        let choosable: Vec<usize> = self.rows().iter().enumerate().filter_map(|(at, row)| matches!(row, Row::Family { .. }).then_some(at)).collect();
        if let (Some(first), Some(last)) = (choosable.first(), choosable.last()) {
            self.active = to.clamp(*first, *last);
        }
    }

    /// A row picked by the pointer or by Enter: the list closes, and the family if it is not the one chosen.
    pub fn pick(&mut self, family: &str) -> Option<String> {
        self.open = false;
        self.typed = false;
        (family != self.chosen).then(|| family.to_owned())
    }

    /// Enter: picks the active row while the list is open, else takes the field's text as typed.
    pub fn enter(&mut self) -> Option<String> {
        let row = self.rows().get(self.active()).cloned();
        match row {
            Some(Row::Family { family, .. }) if self.open => self.pick(&family),
            _ => {
                let said = self.commit();
                self.open = false;
                said
            }
        }
    }

    /// The field was left: a name typed and not picked is chosen as typed, and the list closes.
    pub fn blur(&mut self) -> Option<String> {
        let said = self.commit();
        self.open = false;
        said
    }

    /// Escape closes the list and gives the typed name up, and only while the list is open: the
    /// next Escape belongs to the dialog.
    pub fn escape(&mut self) -> bool {
        if !self.open {
            return false;
        }
        self.typed = false;
        self.open = false;
        true
    }

    fn commit(&mut self) -> Option<String> {
        let family = self.text.trim().to_owned();
        let said = (self.typed && family != self.chosen).then_some(family);
        self.typed = false;
        said
    }
}

#[cfg(test)]
mod tests;
