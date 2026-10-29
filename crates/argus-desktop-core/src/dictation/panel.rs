//! What the panel over the target says for a run, and what the titlebar chip says of dictation:
//! the state, the pane the text goes to, the text so far, and the keys that answer it now.
//!
//! The words are the design page's (the Argus Dictation scope): errors stay in the panel rather
//! than becoming a banner, and a problem with the setup says what to do about it.

use super::machine::{megabytes, Engine, Machine};
use super::model::{Model, MODELS};
use super::preferences::{DictationPreferences, Device};
use super::run::{Mode, Phase, Problem, Run, Target};

/// The colour a state is said in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Plain,
    Ok,
    Warn,
    Bad,
}

/// The level bars beside the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Meter {
    /// The microphone's level as it is.
    Live,
    /// A still ramp: listening, but nothing is heard by the model yet.
    Loading,
    /// Flat and grey: no longer listening.
    Flat,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelPage {
    pub run: u64,
    pub state: String,
    pub tone: Tone,
    /// `to`, `into` or `was`, and the target's name.
    pub target: Option<(String, String)>,
    pub committed: String,
    pub tentative: String,
    /// Said, faint and slanted, in place of the text while there is none.
    pub placeholder: Option<String>,
    /// Said plainly in place of the text: what to do about a problem.
    pub note: Option<String>,
    pub meter: Option<Meter>,
    /// Each key that answers the panel now, as written, and what it does.
    pub keys: Vec<(String, String)>,
    /// Still hearing the microphone or finishing: Escape discards it.
    pub open: bool,
    /// Text waiting for a place to go: Enter pastes it, C copies it, Escape drops it.
    pub held: bool,
    /// A problem the Settings sheet answers: Enter opens it.
    pub opens_settings: bool,
    /// How long it stays once it has nothing more to say, in milliseconds.
    pub lingers_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChipPage {
    pub words: String,
    pub live: bool,
    pub tone: Tone,
}

/// The keys written as the panel shows them.
pub struct Keys<'a> {
    pub hold: &'a str,
    pub toggle: &'a str,
}

fn key(key: &str, does: &str) -> (String, String) {
    (key.to_owned(), does.to_owned())
}

/// What a problem is called, and what to do about it.
fn problem(problem: Problem, machine: &Machine) -> (String, String) {
    let size = |model: Model| machine.file(model).map_or_else(String::new, |file| format!(" ({})", megabytes(file.size)));
    match problem {
        Problem::Off => ("Dictation is off".into(), "Turn it on in Settings, under Dictation.".into()),
        Problem::NoModel => {
            let offers: Vec<String> = MODELS.iter().map(|model| format!("{}{}", model.short(), size(*model))).collect();
            ("No model downloaded".into(), format!("Download {} to dictate.", offers.join(" or ")))
        }
        Problem::NotDownloaded(model) => {
            let other = MODELS.iter().find(|other| **other != model && machine.ready(**other));
            let then = other.map_or_else(String::new, |other| format!(", or choose {}, which is", other.short()));
            (format!("{} is not downloaded", model.short()), format!("Download it{} in Settings{then}.", size(model)))
        }
        Problem::NoMicrophone => ("No microphone".into(), "Plug one in, or choose another in Settings.".into()),
        Problem::Refused => ("The system refused the microphone".into(), "Allow Argus to use the microphone in the system's privacy settings.".into()),
        Problem::NoGpu => ("No GPU found".into(), "Choose Auto or CPU in Settings.".into()),
    }
}

/// The panel for `run`. `alive` says whether its pane is still open, so a closed one is said
/// before the text is finished.
pub fn panel(run: &Run, machine: &Machine, keys: &Keys, alive: bool) -> PanelPage {
    let name = run.target.as_ref().map(|target| target.name().to_owned()).unwrap_or_default();
    let to = |word: &str| run.target.as_ref().map(|_| (word.to_owned(), name.clone()));
    let live_keys = || match run.mode {
        Mode::Hold => vec![key("release", "paste"), key("Esc", "discard"), key(keys.hold, "held")],
        Mode::Toggle => vec![key(keys.toggle, "paste"), key("Esc", "discard")],
    };
    let mut page = PanelPage { run: run.id, committed: run.committed.clone(), tentative: run.tentative.clone(), ..PanelPage::default() };
    let heard_nothing = run.committed.is_empty() && run.tentative.is_empty();
    match &run.phase {
        Phase::Loading => {
            page.state = format!("Loading {}\u{2026}", run.model.short());
            page.target = to("to");
            page.meter = Some(Meter::Loading);
            page.placeholder = Some("Listening already. The first words are held until the model is up.".into());
            page.keys = live_keys();
        }
        Phase::Listening => {
            page.state = "Listening".into();
            page.target = to("to");
            page.meter = Some(Meter::Live);
            page.placeholder = heard_nothing.then(|| "Start speaking\u{2026}".into());
            page.keys = live_keys();
        }
        Phase::Finishing => {
            page.state = "Finishing".into();
            page.target = to("to");
            page.meter = Some(Meter::Flat);
            page.keys = vec![key("Esc", "discard")];
        }
        Phase::Pasted { text } => {
            page.state = "Pasted".into();
            page.tone = Tone::Ok;
            page.target = to("into");
            (page.committed, page.tentative) = (text.trim_end().to_owned(), String::new());
            page.lingers_ms = Some(1200);
        }
        Phase::Held { text } => {
            page.state = match run.target {
                Some(Target::Field) => "The field went away".into(),
                _ => "The pane closed".into(),
            };
            page.tone = Tone::Warn;
            page.target = to("was");
            (page.committed, page.tentative) = (text.trim_end().to_owned(), String::new());
            page.keys = vec![key("Enter", "paste into focused pane"), key("C", "copy"), key("Esc", "drop")];
            page.held = true;
        }
        Phase::Empty => {
            page.state = "Nothing heard".into();
            page.target = to("to");
            page.placeholder = Some("Nothing was pasted.".into());
            page.lingers_ms = Some(2000);
        }
        Phase::NotSetUp(found) => {
            let (state, note) = problem(*found, machine);
            page.state = state;
            page.tone = Tone::Bad;
            page.note = Some(note);
            page.keys = vec![key("Enter", "open Dictation settings"), key("Esc", "dismiss")];
            page.opens_settings = true;
            page.lingers_ms = Some(8000);
        }
        Phase::Failed { reason } => {
            page.state = "Dictation failed".into();
            page.tone = Tone::Bad;
            page.target = to("to");
            page.note = Some(reason.clone());
            page.keys = vec![key("Esc", "dismiss")];
            page.lingers_ms = Some(6000);
        }
    }
    if run.open() && !alive {
        page.target = to("was");
        page.tone = Tone::Warn;
    }
    page.open = run.open();
    page
}

/// The titlebar chip: dictation's model and device while it rests, and the run's state while one
/// is open.
pub fn chip(chosen: &DictationPreferences, machine: &Machine, run: Option<&Run>) -> ChipPage {
    match run.map(|run| &run.phase) {
        Some(Phase::Loading) => return ChipPage { words: "Loading\u{2026}".into(), live: true, tone: Tone::Plain },
        Some(Phase::Listening) => return ChipPage { words: "Listening".into(), live: true, tone: Tone::Plain },
        Some(Phase::Finishing) => return ChipPage { words: "Finishing".into(), live: true, tone: Tone::Plain },
        _ => {}
    }
    if !machine.ready(chosen.model) {
        return ChipPage { words: "Dictation \u{b7} not set up".into(), live: false, tone: Tone::Warn };
    }
    let device = match &machine.engine {
        Engine::Ready { model, backend, .. } if *model == chosen.model => Some(if backend.eq_ignore_ascii_case("cpu") { "CPU" } else { "GPU" }),
        _ => match chosen.device {
            Device::Auto => None,
            Device::Gpu => Some("GPU"),
            Device::Cpu => Some("CPU"),
        },
    };
    let mut words = format!("Dictation \u{b7} {}", chosen.model.short());
    if let Some(device) = device {
        words.push_str(" \u{b7} ");
        words.push_str(device);
    }
    let tone = if matches!(machine.engine, Engine::Failed { .. }) { Tone::Bad } else { Tone::Plain };
    ChipPage { words, live: false, tone }
}
