//! The browsers agents run, as this window draws them: decoding a browser's pictures and holding
//! one shared image of each, a picture drawn in a place, the Browsers stage that tiles every
//! browser, the browser pane's cell in the grid, and the words it says with none open.

pub mod decode;
pub mod decoding;
pub mod empty;
pub mod pane;
pub mod picture;
pub mod pictures;
pub mod stage;
pub mod tile;

#[cfg(test)]
mod tests;
