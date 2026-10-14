//! Messages between the herdr bridge (laptop) and the panel (board).
//!
//! One JSON object per line over the USB serial port. The board's log output
//! shares that port, so receivers ignore any line that does not start with `{`.
//! Enums use serde's default (externally tagged) form, which `serde-json-core`
//! on the board can parse.
#![no_std]

pub use heapless;
use heapless::{String, Vec};
use serde::{Deserialize, Serialize};

/// Most agents the panel shows.
pub const MAX_AGENTS: usize = 12;

/// Longest label and id, in bytes.
pub const LABEL_LEN: usize = 24;
pub const ID_LEN: usize = 16;

/// Longest line either side sends, including the newline. Sized for a full
/// [`ToBoard::State`].
pub const MAX_LINE: usize = 4096;

pub type Label = String<LABEL_LEN>;
pub type Id = String<ID_LEN>;

/// What herdr reports for an agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Idle,
    Working,
    Blocked,
    Done,
    Unknown,
}

/// One agent running in a herdr pane.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    /// herdr's pane id, such as `w1:p2`. Also the focus target.
    pub id: Id,
    /// Caption: the agent's name if it has one, else its workspace's label.
    pub label: Label,
    pub status: Status,
    pub focused: bool,
}

/// Every agent herdr knows about, in display order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub agents: Vec<Agent, MAX_AGENTS>,
}

/// Bridge to board.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToBoard {
    State(State),
}

/// Board to bridge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToHost {
    /// The board has started (or restarted) and wants the current state.
    Hello,
    /// The user tapped an agent.
    Focus { agent: Id },
}

/// Copies `s` into a fixed-capacity string, cutting at a character boundary
/// when it does not fit.
pub fn truncated<const N: usize>(s: &str) -> String<N> {
    let mut out = String::new();
    for c in s.chars() {
        if out.push(c).is_err() {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    fn sample() -> State {
        let mut agents = Vec::new();
        agents
            .push(Agent { id: truncated("w1:p1"), label: truncated("herdr-panel"), status: Status::Working, focused: true })
            .unwrap();
        agents
            .push(Agent { id: truncated("w2:p1"), label: truncated("dotfiles"), status: Status::Idle, focused: false })
            .unwrap();
        State { agents }
    }

    #[test]
    fn state_round_trips_as_json() {
        let msg = ToBoard::State(sample());
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(serde_json::from_str::<ToBoard>(&json).unwrap(), msg);
        assert!(json.starts_with(r#"{"State":{"agents":[{"id":"w1:p1""#), "{json}");
    }

    #[test]
    fn host_messages_use_short_json() {
        let focus = ToHost::Focus { agent: truncated("w1:p2") };
        assert_eq!(serde_json::to_string(&focus).unwrap(), r#"{"Focus":{"agent":"w1:p2"}}"#);
        assert_eq!(serde_json::to_string(&ToHost::Hello).unwrap(), r#""Hello""#);
    }

    /// The board parses with serde-json-core and the bridge writes with
    /// serde_json; check they agree on the wire format.
    #[test]
    fn board_parser_reads_the_bridge_output() {
        let msg = ToBoard::State(sample());
        let json = serde_json::to_string(&msg).unwrap();
        let (parsed, _) = serde_json_core::from_str::<ToBoard>(&json).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn bridge_parser_reads_the_board_output() {
        for msg in [ToHost::Hello, ToHost::Focus { agent: truncated("w1:p2") }] {
            let mut buf = [0u8; 64];
            let n = serde_json_core::to_slice(&msg, &mut buf).unwrap();
            let json = core::str::from_utf8(&buf[..n]).unwrap();
            assert_eq!(serde_json::from_str::<ToHost>(json).unwrap(), msg, "{json}");
        }
    }

    #[test]
    fn truncation_keeps_characters_whole() {
        let s: String<5> = truncated("ab日本語");
        assert_eq!(s.as_str(), "ab日");
    }

    #[test]
    fn full_state_fits_in_a_line() {
        let mut state = State::default();
        for i in 0..MAX_AGENTS {
            state
                .agents
                .push(Agent {
                    id: truncated("wXXXXXXXXXX:pNNNN"),
                    label: truncated("a-long-agent-label-of-24"),
                    status: if i % 2 == 0 { Status::Unknown } else { Status::Blocked },
                    focused: i == 0,
                })
                .unwrap();
        }
        let json = serde_json::to_string(&ToBoard::State(state)).unwrap();
        assert!(json.len() < MAX_LINE, "{} bytes", json.len());
    }
}
