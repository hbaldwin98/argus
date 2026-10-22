//! The lanes of a timeline: which columns its events are drawn in, and the one each event is in.
//!
//! One row per event, so row N of every lane is one step of the story and the lanes read side by
//! side. An event that names no lane goes in the first lane's column.

use argus_protocol::TimelineEvent;

/// The lanes the events name, in the order they first appear. None when no event names one, which is
/// a plain timeline.
pub fn lanes(events: &[TimelineEvent]) -> Vec<&str> {
    let mut found: Vec<&str> = Vec::new();
    for lane in events.iter().filter_map(|event| event.lane.as_deref()) {
        if !found.contains(&lane) {
            found.push(lane);
        }
    }
    found
}

/// The index of the lane `event` is drawn in, among `lanes` (which is not empty).
pub fn lane_of(event: &TimelineEvent, lanes: &[&str]) -> usize {
    event.lane.as_deref().and_then(|own| lanes.iter().position(|lane| *lane == own)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use argus_protocol::Tone;

    fn event(lane: Option<&str>) -> TimelineEvent {
        TimelineEvent { at: None, title: "e".into(), tone: Tone::Neutral, body: None, lane: lane.map(str::to_owned), opens: None }
    }

    #[test]
    fn lanes_are_listed_once_in_the_order_they_first_appear() {
        let events = [event(Some("ci")), event(None), event(Some("agent")), event(Some("ci"))];
        assert_eq!(lanes(&events), ["ci", "agent"]);
    }

    #[test]
    fn events_naming_no_lane_make_none() {
        assert!(lanes(&[event(None), event(None)]).is_empty());
    }

    #[test]
    fn an_event_is_in_its_lane_and_one_naming_none_is_in_the_first() {
        let lanes = ["ci", "agent"];
        assert_eq!(lane_of(&event(Some("agent")), &lanes), 1);
        assert_eq!(lane_of(&event(Some("ci")), &lanes), 0);
        assert_eq!(lane_of(&event(None), &lanes), 0);
    }
}
