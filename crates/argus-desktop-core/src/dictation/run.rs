//! One dictation, from the press to the paste: what it is speaking into, how it was started, and
//! where it stands (loading → listening → finishing → pasted, or held, empty, not set up, failed).
//!
//! The target is locked at the press, so a click on another pane mid-sentence never splits one
//! dictation between two agents; the panel names it so there is no guessing.

use argus_protocol::PaneId;

use super::model::Model;

/// How the run was started, which says how it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Push-to-talk: the chord held, and let go to paste.
    Hold,
    /// The leader and V, or the palette: open until asked again.
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A pane's program, which gets the text as one bracketed paste.
    Pane { pane: PaneId, name: String },
    /// A text field of the window's (the composer, a review comment, the palette), which gets it
    /// typed, as an input method's text is.
    Field,
}

impl Target {
    pub fn name(&self) -> &str {
        match self {
            Target::Pane { name, .. } => name,
            Target::Field => "the text field",
        }
    }
}

/// What has to be set up before dictation can work, said in the panel rather than a banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// Turned off in Settings.
    Off,
    /// No model is downloaded.
    NoModel,
    /// The chosen model is not downloaded, though another is.
    NotDownloaded(Model),
    NoMicrophone,
    /// The system refused the microphone (macOS's privacy setting, Windows' microphone access).
    Refused,
    /// The GPU was chosen and there is none.
    NoGpu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// The model is loading; what is said meanwhile is recorded and heard once it is up.
    Loading,
    Listening,
    /// Let go: the stream is being finished.
    Finishing,
    Pasted { text: String },
    /// The target went while it was spoken: the text waits for the person to say where it goes.
    Held { text: String },
    /// Nothing was heard worth pasting.
    Empty,
    NotSetUp(Problem),
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub id: u64,
    pub mode: Mode,
    /// None only for a run that never started, for want of a model or a key.
    pub target: Option<Target>,
    pub model: Model,
    pub phase: Phase,
    /// The words the model has settled on, and its guess at the rest, as heard so far.
    pub committed: String,
    pub tentative: String,
}

impl Run {
    pub fn started(id: u64, mode: Mode, target: Target, model: Model, loaded: bool) -> Run {
        let phase = if loaded { Phase::Listening } else { Phase::Loading };
        Run { id, mode, target: Some(target), model, phase, committed: String::new(), tentative: String::new() }
    }

    pub fn not_set_up(id: u64, mode: Mode, model: Model, problem: Problem) -> Run {
        Run { id, mode, target: None, model, phase: Phase::NotSetUp(problem), committed: String::new(), tentative: String::new() }
    }

    /// Still hearing the microphone.
    pub fn live(&self) -> bool {
        matches!(self.phase, Phase::Loading | Phase::Listening)
    }

    /// The engine still has it: live, or finishing.
    pub fn open(&self) -> bool {
        self.live() || self.phase == Phase::Finishing
    }

    pub fn held(&self) -> Option<&str> {
        match &self.phase {
            Phase::Held { text } => Some(text),
            _ => None,
        }
    }
}
