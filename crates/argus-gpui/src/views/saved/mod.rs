//! A saved view as the GPUI window draws it: the panel `saved:<id>` with its trail of columns, each
//! column's header, the Details fold, the delete question, and the one element each panel is
//! wherever it is shown.

pub mod column;
pub mod delete;
pub mod details;
pub mod head;
pub mod header;
pub mod panel;
pub mod registry;
