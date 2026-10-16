//! Messages between the herdr bridge (laptop) and the panel (board).
//!
//! One JSON value per line over the USB serial port. The board's log output
//! shares that port, so receivers ignore any line that does not start with `{`
//! or `"`.
//! Enums use serde's default (externally tagged) form, which `serde-json-core`
//! on the board can parse.
#![no_std]

pub use heapless;
use heapless::{String, Vec};
use serde::{Deserialize, Serialize};

/// Most agents the panel shows.
pub const MAX_AGENTS: usize = 250;

/// Longest label and id, in bytes.
pub const LABEL_LEN: usize = 24;
pub const ID_LEN: usize = 16;

/// Longest line either side sends, including the newline: one agent, with
/// room to spare.
pub const MAX_LINE: usize = 512;

pub type Label = String<LABEL_LEN>;

/// Longest herdr version, in bytes.
pub const VERSION_LEN: usize = 16;
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
    /// Its workspace's label; empty from a bridge that does not send it.
    #[serde(default)]
    pub workspace: Label,
    pub status: Status,
    pub focused: bool,
}

/// Every agent herdr knows about, in display order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub agents: Vec<Agent, MAX_AGENTS>,
}

/// What the panel's sky shows, by the bridge's local time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeOfDay {
    Dawn,
    Day,
    Sunset,
    Night,
}

impl TimeOfDay {
    /// Every time of day, from dawn.
    pub const ALL: [TimeOfDay; 4] = [Self::Dawn, Self::Day, Self::Sunset, Self::Night];

    /// The time of day `minute` minutes after local midnight: dawn from
    /// 5:00, day from 7:00, sunset from 17:00 and night from 19:00.
    pub fn at(minute: u16) -> Self {
        match minute / 60 {
            5..=6 => Self::Dawn,
            7..=16 => Self::Day,
            17..=18 => Self::Sunset,
            _ => Self::Night,
        }
    }

    /// Its name, as in the art's file names.
    pub fn name(self) -> &'static str {
        match self {
            Self::Dawn => "dawn",
            Self::Day => "day",
            Self::Sunset => "sunset",
            Self::Night => "night",
        }
    }
}

/// How far clouds drift, in pixels per hour: so slowly that one takes about
/// two weeks to cross the screen.
pub const CLOUD_PX_PER_HOUR: u32 = 1;

/// The left edge of a cloud `w` wide that starts at `x` on a screen `screen_w`
/// wide, after `hours` hours of drifting right: past the right edge, it comes
/// back in at the left.
pub fn cloud_x(x: i32, w: i32, screen_w: i32, hours: u32) -> i32 {
    let span = (screen_w + w) as u32;
    let drift = (hours % span * CLOUD_PX_PER_HOUR % span) as i32;
    (x + drift + w) % (screen_w + w) - w
}

/// Bridge to board.
///
/// A [`State`] goes as `Begin`, then one `Agent` per agent, then `End`, each on
/// its own line (see [`messages`]): the board decodes one small message at a
/// time instead of a whole state, which would need several copies of it on
/// its stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToBoard {
    Begin {
        count: u16,
    },
    Agent(Agent),
    End,
    /// The bridge's clock: minutes since local midnight, and hours since the
    /// Unix epoch. Sent on its own, whenever the minute changes.
    Clock {
        minute: u16,
        hour: u32,
    },
    /// The version of herdr's server, such as `0.9.3`. Sent on its own, when
    /// it changes.
    Herdr {
        version: String<VERSION_LEN>,
    },
}

/// The messages that send `state`, in order.
pub fn messages(state: &State) -> impl Iterator<Item = ToBoard> + '_ {
    let begin = ToBoard::Begin {
        count: state.agents.len() as u16,
    };
    core::iter::once(begin)
        .chain(state.agents.iter().cloned().map(ToBoard::Agent))
        .chain(core::iter::once(ToBoard::End))
}

/// Rebuilds states from [`messages`], one message at a time.
#[derive(Debug, Default)]
pub struct Assembler {
    state: State,
    /// Agents announced by the last `Begin`; `None` until one arrives, or
    /// after an agent did not fit.
    expected: Option<u16>,
}

impl Assembler {
    pub const fn new() -> Self {
        Self {
            state: State { agents: Vec::new() },
            expected: None,
        }
    }

    /// Takes the next message. On an `End` that completes a state, returns
    /// it; it stays in the assembler until the next `Begin`, so the caller can
    /// swap it out rather than copy it. A state with a missing or extra agent
    /// is dropped.
    pub fn push(&mut self, msg: ToBoard) -> Option<&mut State> {
        match msg {
            ToBoard::Begin { count } => {
                self.state.agents.clear();
                self.expected = Some(count);
            }
            ToBoard::Agent(agent) => {
                if self.expected.is_some() && self.state.agents.push(agent).is_err() {
                    self.expected = None;
                }
            }
            ToBoard::Clock { .. } | ToBoard::Herdr { .. } => {}
            ToBoard::End => {
                let complete = self
                    .expected
                    .take()
                    .is_some_and(|n| usize::from(n) == self.state.agents.len());
                return complete.then_some(&mut self.state);
            }
        }
        None
    }
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

/// The workspace part of a pane id: `w1` in `w1:p2`.
pub fn workspace(id: &str) -> &str {
    id.split(':').next().unwrap_or_default()
}

/// One space of the panel's tower, a few desks of a storey: agents of one
/// workspace, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Space<'a> {
    /// Which workspace this is, counting from 0 in the order they appear;
    /// spaces of the same workspace share it.
    pub workspace: usize,
    pub agents: &'a [Agent],
}

impl<'a> Space<'a> {
    /// The workspace's label.
    pub fn label(&self) -> &'a str {
        self.agents.first().map_or("", |a| a.workspace.as_str())
    }

    /// Whether the space's lights are on: someone there is not idle, or has
    /// the focus.
    pub fn lit(&self) -> bool {
        self.agents
            .iter()
            .any(|a| a.status != Status::Idle || a.focused)
    }
}

/// Splits `agents` into spaces of at most `per_space` agents (at least 1),
/// one workspace per space: a workspace with more agents takes several
/// spaces in a row. Agents of a workspace must be next to each other, as the
/// bridge sends them.
pub fn spaces(agents: &[Agent], per_space: usize) -> impl Iterator<Item = Space<'_>> {
    let per_space = per_space.max(1);
    let mut rest = agents;
    let mut index = 0;
    let mut previous: Option<&str> = None;
    core::iter::from_fn(move || {
        let first = rest.first()?;
        let ws = workspace(&first.id);
        if previous.is_some_and(|p| p != ws) {
            index += 1;
        }
        previous = Some(ws);
        let run = rest
            .iter()
            .take_while(|a| workspace(&a.id) == ws)
            .count()
            .min(per_space);
        let (space, tail) = rest.split_at(run);
        rest = tail;
        Some(Space {
            workspace: index,
            agents: space,
        })
    })
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    fn sample() -> State {
        let mut agents = Vec::new();
        agents
            .push(Agent {
                id: truncated("w1:p1"),
                label: truncated("herdr-panel"),
                workspace: truncated("herdr-panel"),
                status: Status::Working,
                focused: true,
            })
            .unwrap();
        agents
            .push(Agent {
                id: truncated("w2:p1"),
                label: truncated("reviewer"),
                workspace: truncated("dotfiles"),
                status: Status::Idle,
                focused: false,
            })
            .unwrap();
        State { agents }
    }

    fn lines(state: &State) -> std::vec::Vec<std::string::String> {
        messages(state)
            .map(|m| serde_json::to_string(&m).unwrap())
            .collect()
    }

    #[test]
    fn a_state_goes_as_begin_agents_end() {
        assert_eq!(
            lines(&sample()),
            [
                r#"{"Begin":{"count":2}}"#,
                r#"{"Agent":{"id":"w1:p1","label":"herdr-panel","workspace":"herdr-panel","status":"working","focused":true}}"#,
                r#"{"Agent":{"id":"w2:p1","label":"reviewer","workspace":"dotfiles","status":"idle","focused":false}}"#,
                r#""End""#,
            ]
        );
    }

    #[test]
    fn an_agent_without_a_workspace_has_an_empty_one() {
        let line = r#"{"Agent":{"id":"w1:p1","label":"x","status":"idle","focused":false}}"#;
        let (msg, _) = serde_json_core::from_str::<ToBoard>(line).unwrap();
        let ToBoard::Agent(agent) = msg else {
            panic!("{msg:?}");
        };
        assert_eq!(agent.workspace, "");
    }

    #[test]
    fn the_clock_is_one_short_line() {
        let clock = ToBoard::Clock {
            minute: 1439,
            hour: 500_000,
        };
        let json = serde_json::to_string(&clock).unwrap();
        assert_eq!(json, r#"{"Clock":{"minute":1439,"hour":500000}}"#);
        assert_eq!(
            serde_json_core::from_str::<ToBoard>(&json).unwrap().0,
            clock
        );
    }

    #[test]
    fn herdrs_version_is_one_short_line() {
        let msg = ToBoard::Herdr {
            version: truncated("0.9.3"),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(json, r#"{"Herdr":{"version":"0.9.3"}}"#);
        assert_eq!(serde_json_core::from_str::<ToBoard>(&json).unwrap().0, msg);
    }

    #[test]
    fn the_day_turns_at_5_7_17_and_19() {
        let at = |h: u16, m: u16| TimeOfDay::at(h * 60 + m);
        assert_eq!(at(4, 59), TimeOfDay::Night);
        assert_eq!(at(5, 0), TimeOfDay::Dawn);
        assert_eq!(at(7, 0), TimeOfDay::Day);
        assert_eq!(at(16, 59), TimeOfDay::Day);
        assert_eq!(at(17, 0), TimeOfDay::Sunset);
        assert_eq!(at(19, 0), TimeOfDay::Night);
        assert_eq!(at(0, 0), TimeOfDay::Night);
    }

    #[test]
    fn clouds_drift_right_and_wrap_around() {
        let x = |hours| cloud_x(14, 46, 320, hours);
        assert_eq!(x(0), 14);
        assert_eq!(x(3), 17);
        // The last pixel column on screen, then just off the left edge.
        assert_eq!(x(305), 319);
        assert_eq!(x(306), -46);
        assert_eq!(x(307), -45);
        // Back where it started after crossing the screen and itself.
        assert_eq!(x(366), 14);
        assert_eq!(x(u32::MAX), x(u32::MAX % 366));
    }

    #[test]
    fn host_messages_use_short_json() {
        let focus = ToHost::Focus {
            agent: truncated("w1:p2"),
        };
        assert_eq!(
            serde_json::to_string(&focus).unwrap(),
            r#"{"Focus":{"agent":"w1:p2"}}"#
        );
        assert_eq!(serde_json::to_string(&ToHost::Hello).unwrap(), r#""Hello""#);
    }

    /// The board parses with serde-json-core and the bridge writes with
    /// serde_json; check they agree on the wire format, and that the board
    /// gets the state back.
    #[test]
    fn board_rebuilds_the_bridge_state() {
        let mut asm = Assembler::new();
        let mut done = None;
        for line in lines(&sample()) {
            let (msg, _) = serde_json_core::from_str::<ToBoard>(&line).unwrap();
            done = asm.push(msg).map(|s| s.clone());
        }
        assert_eq!(done, Some(sample()));
    }

    #[test]
    fn a_state_with_a_lost_agent_is_dropped() {
        let mut asm = Assembler::new();
        let msgs: std::vec::Vec<ToBoard> = messages(&sample()).collect();
        for (i, m) in msgs.iter().enumerate() {
            if i != 1 {
                assert_eq!(asm.push(m.clone()), None);
            }
        }
        // The next complete state gets through.
        let last = messages(&sample())
            .map(|m| asm.push(m).map(|s| s.clone()))
            .last()
            .flatten();
        assert_eq!(last, Some(sample()));
    }

    #[test]
    fn an_end_without_a_begin_is_ignored() {
        let mut asm = Assembler::new();
        assert_eq!(asm.push(ToBoard::Agent(sample().agents[0].clone())), None);
        assert_eq!(asm.push(ToBoard::End), None);
    }

    #[test]
    fn an_empty_state_is_a_state() {
        let mut asm = Assembler::new();
        let last = messages(&State::default())
            .map(|m| asm.push(m).map(|s| s.clone()))
            .last()
            .flatten();
        assert_eq!(last, Some(State::default()));
    }

    #[test]
    fn bridge_parser_reads_the_board_output() {
        for msg in [
            ToHost::Hello,
            ToHost::Focus {
                agent: truncated("w1:p2"),
            },
        ] {
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

    fn agents(ids: &[&str]) -> std::vec::Vec<Agent> {
        ids.iter()
            .map(|id| Agent {
                id: truncated(id),
                label: Label::new(),
                workspace: truncated(workspace(id)),
                status: Status::Idle,
                focused: false,
            })
            .collect()
    }

    fn shape(agents: &[Agent], per: usize) -> std::vec::Vec<(usize, usize)> {
        spaces(agents, per)
            .map(|s| (s.workspace, s.agents.len()))
            .collect()
    }

    #[test]
    fn each_workspace_gets_a_space() {
        let a = agents(&["w1:p1", "w1:p2", "w2:p1", "w3:p4"]);
        assert_eq!(shape(&a, 6), [(0, 2), (1, 1), (2, 1)]);
    }

    #[test]
    fn a_big_workspace_takes_several_spaces() {
        let ids: std::vec::Vec<std::string::String> =
            (0..14).map(|i| std::format!("w1:p{i}")).collect();
        let mut a = agents(&ids.iter().map(|s| s.as_str()).collect::<std::vec::Vec<_>>());
        a.extend(agents(&["w2:p1"]));
        assert_eq!(shape(&a, 6), [(0, 6), (0, 6), (0, 2), (1, 1)]);
    }

    #[test]
    fn a_full_space_does_not_spill_into_the_next_workspace() {
        let a = agents(&["w1:p1", "w1:p2", "w2:p1"]);
        assert_eq!(shape(&a, 2), [(0, 2), (1, 1)]);
    }

    #[test]
    fn spaces_keep_every_agent_in_order() {
        let a = agents(&["w1:p1", "w1:p2", "w1:p3", "w2:p1", "w2:p2"]);
        let flat: std::vec::Vec<&str> = spaces(&a, 2)
            .flat_map(|s| s.agents.iter().map(|a| a.id.as_str()))
            .collect();
        assert_eq!(flat, ["w1:p1", "w1:p2", "w1:p3", "w2:p1", "w2:p2"]);
    }

    #[test]
    fn a_space_has_its_workspace_label() {
        let a = agents(&["w1:p1", "w1:p2", "w2:p1"]);
        let labels: std::vec::Vec<&str> = spaces(&a, 1).map(|s| s.label()).collect();
        assert_eq!(labels, ["w1", "w1", "w2"]);
    }

    #[test]
    fn a_space_is_lit_unless_everyone_idles_unfocused() {
        let mut a = agents(&["w1:p1", "w1:p2"]);
        let lit = |a: &[Agent]| spaces(a, 6).next().unwrap().lit();
        assert!(!lit(&a));
        a[1].focused = true;
        assert!(lit(&a));
        a[1].focused = false;
        a[0].status = Status::Working;
        assert!(lit(&a));
    }

    #[test]
    fn no_agents_no_spaces() {
        assert_eq!(spaces(&[], 6).count(), 0);
    }

    #[test]
    fn every_message_of_a_full_state_fits_in_a_line() {
        let mut state = State::default();
        for i in 0..MAX_AGENTS {
            state
                .agents
                .push(Agent {
                    id: truncated("wXXXXXXXXXX:pNNNN"),
                    label: truncated("a-long-agent-label-of-24"),
                    workspace: truncated("a-long-workspace-label24"),
                    status: if i % 2 == 0 {
                        Status::Unknown
                    } else {
                        Status::Blocked
                    },
                    focused: i == 0,
                })
                .unwrap();
        }
        for line in lines(&state) {
            assert!(line.len() < MAX_LINE, "{} bytes", line.len());
        }
    }
}
