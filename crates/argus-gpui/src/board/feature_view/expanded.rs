//! What the expanded diagram holds of its own (BOARD-SEQ-7, -9): its zoom, whether it is fitted to its
//! canvas, and whether the steps panel is shown. The page keeps it as `state.expanded`, lost with the
//! page; here it is the feature view's, and gone when the diagram collapses.
//!
//! Zooming by hand turns fit off and fit sets the zoom itself; every transition is a pure function of
//! what is held, so each rule is tested without a window.

use argus_desktop_core::board::lenses::picture::{fit_zoom, zoom_by};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Expanded {
    pub zoom: f32,
    pub fit: bool,
    pub panel: bool,
}

impl Default for Expanded {
    /// Open at 100%, fitted, with the steps beside it.
    fn default() -> Self {
        Self { zoom: 1.0, fit: true, panel: true }
    }
}

impl Expanded {
    /// Zoomed by hand by `by`: fit is off.
    pub fn zoomed(self, by: f32) -> Self {
        Self { zoom: zoom_by(self.zoom, by), fit: false, ..self }
    }

    /// At 100%: fit is off.
    pub fn actual(self) -> Self {
        Self { zoom: 1.0, fit: false, ..self }
    }

    /// Fitted to the canvas again.
    pub fn fitted(self) -> Self {
        Self { fit: true, ..self }
    }

    /// The steps panel shown or hidden.
    pub fn panelled(self) -> Self {
        Self { panel: !self.panel, ..self }
    }

    /// With the panel shown (editing the source opens it).
    pub fn with_panel(self) -> Self {
        Self { panel: true, ..self }
    }

    /// The zoom to draw at: the fit of `diagram` in `canvas` while fitted, else the held zoom.
    pub fn zoom_in(self, canvas: (f32, f32), diagram: (f32, f32)) -> f32 {
        if self.fit && canvas.0 > 0.0 && canvas.1 > 0.0 {
            fit_zoom(canvas, diagram)
        } else {
            self.zoom
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // BOARD-SEQ-7: it opens at 100%, fitted, with the steps.
    #[test]
    fn it_opens_fitted_at_a_hundred_percent_with_its_steps() {
        assert_eq!(Expanded::default(), Expanded { zoom: 1.0, fit: true, panel: true });
    }

    // BOARD-SEQ-9: a manual zoom turns fit off; the percent button is 100% and also turns it off.
    #[test]
    fn zooming_by_hand_turns_fit_off_and_the_percent_goes_back_to_a_hundred() {
        let zoomed = Expanded::default().zoomed(0.1);
        assert_eq!((zoomed.zoom, zoomed.fit), (1.1, false));
        assert_eq!(zoomed.zoomed(-0.1).zoomed(-0.1).zoom, 0.9);
        let actual = zoomed.actual();
        assert_eq!((actual.zoom, actual.fit), (1.0, false));
        assert!(actual.fitted().fit, "Fit turns it back on");
    }

    #[test]
    fn the_steps_are_shown_or_hidden_and_editing_the_source_shows_them() {
        let hidden = Expanded::default().panelled();
        assert!(!hidden.panel && hidden.panelled().panel);
        assert!(hidden.with_panel().panel);
        assert!(Expanded::default().with_panel().panel, "already shown stays shown");
    }

    #[test]
    fn a_fitted_diagram_is_drawn_at_the_fit_and_a_held_one_at_its_zoom() {
        let fitted = Expanded::default();
        assert_eq!(fitted.zoom_in((832.0, 632.0), (400.0, 300.0)), 2.0);
        assert_eq!(fitted.zoom_in((0.0, 0.0), (400.0, 300.0)), 1.0, "a canvas not measured yet leaves the zoom");
        assert_eq!(Expanded::default().zoomed(0.5).zoom_in((832.0, 632.0), (400.0, 300.0)), 1.5);
    }
}
