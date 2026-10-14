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
///
/// A [`State`] goes as `Begin`, then one `Agent` per agent, then `End`, each on
/// its own line (see [`messages`]): the board decodes one small message at a
/// time instead of a whole state, which would need several copies of it on
/// its stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToBoard {
    Begin { count: u16 },
    Agent(Agent),
    End,
}

/// The messages that send `state`, in order.
pub fn messages(state: &State) -> impl Iterator<Item = ToBoard> + '_ {
    let begin = ToBoard::Begin { count: state.agents.len() as u16 };
    core::iter::once(begin).chain(state.agents.iter().cloned().map(ToBoard::Agent)).chain(core::iter::once(ToBoard::End))
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
        Self { state: State { agents: Vec::new() }, expected: None }
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
            ToBoard::End => {
                let complete = self.expected.take().is_some_and(|n| usize::from(n) == self.state.agents.len());
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

/// One storey of the panel's tower: agents of one workspace, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Storey<'a> {
    /// Which workspace this is, counting from 0 in the order they appear;
    /// storeys of the same workspace share it.
    pub workspace: usize,
    pub agents: &'a [Agent],
}

impl Storey<'_> {
    /// Whether the storey's lights are on: someone there is not idle, or
    /// has the focus.
    pub fn lit(&self) -> bool {
        self.agents.iter().any(|a| a.status != Status::Idle || a.focused)
    }
}

/// Splits `agents` into storeys of at most `per_storey` agents (at least 1),
/// one workspace per storey: a workspace with more agents takes several
/// storeys in a row. Agents of a workspace must be next to each other, as the
/// bridge sends them.
pub fn storeys(agents: &[Agent], per_storey: usize) -> impl Iterator<Item = Storey<'_>> {
    let per_storey = per_storey.max(1);
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
        let run = rest.iter().take_while(|a| workspace(&a.id) == ws).count().min(per_storey);
        let (storey, tail) = rest.split_at(run);
        rest = tail;
        Some(Storey { workspace: index, agents: storey })
    })
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

    fn lines(state: &State) -> std::vec::Vec<std::string::String> {
        messages(state).map(|m| serde_json::to_string(&m).unwrap()).collect()
    }

    #[test]
    fn a_state_goes_as_begin_agents_end() {
        assert_eq!(
            lines(&sample()),
            [
                r#"{"Begin":{"count":2}}"#,
                r#"{"Agent":{"id":"w1:p1","label":"herdr-panel","status":"working","focused":true}}"#,
                r#"{"Agent":{"id":"w2:p1","label":"dotfiles","status":"idle","focused":false}}"#,
                r#""End""#,
            ]
        );
    }

    #[test]
    fn host_messages_use_short_json() {
        let focus = ToHost::Focus { agent: truncated("w1:p2") };
        assert_eq!(serde_json::to_string(&focus).unwrap(), r#"{"Focus":{"agent":"w1:p2"}}"#);
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
        let last = messages(&sample()).map(|m| asm.push(m).map(|s| s.clone())).last().flatten();
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
        let last = messages(&State::default()).map(|m| asm.push(m).map(|s| s.clone())).last().flatten();
        assert_eq!(last, Some(State::default()));
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

    fn agents(ids: &[&str]) -> std::vec::Vec<Agent> {
        ids.iter()
            .map(|id| Agent { id: truncated(id), label: Label::new(), status: Status::Idle, focused: false })
            .collect()
    }

    fn shape(agents: &[Agent], per: usize) -> std::vec::Vec<(usize, usize)> {
        storeys(agents, per).map(|s| (s.workspace, s.agents.len())).collect()
    }

    #[test]
    fn each_workspace_gets_a_storey() {
        let a = agents(&["w1:p1", "w1:p2", "w2:p1", "w3:p4"]);
        assert_eq!(shape(&a, 6), [(0, 2), (1, 1), (2, 1)]);
    }

    #[test]
    fn a_big_workspace_takes_several_storeys() {
        let ids: std::vec::Vec<std::string::String> = (0..14).map(|i| std::format!("w1:p{i}")).collect();
        let mut a = agents(&ids.iter().map(|s| s.as_str()).collect::<std::vec::Vec<_>>());
        a.extend(agents(&["w2:p1"]));
        assert_eq!(shape(&a, 6), [(0, 6), (0, 6), (0, 2), (1, 1)]);
    }

    #[test]
    fn a_full_storey_does_not_spill_into_the_next_workspace() {
        let a = agents(&["w1:p1", "w1:p2", "w2:p1"]);
        assert_eq!(shape(&a, 2), [(0, 2), (1, 1)]);
    }

    #[test]
    fn storeys_keep_every_agent_in_order() {
        let a = agents(&["w1:p1", "w1:p2", "w1:p3", "w2:p1", "w2:p2"]);
        let flat: std::vec::Vec<&str> = storeys(&a, 2).flat_map(|s| s.agents.iter().map(|a| a.id.as_str())).collect();
        assert_eq!(flat, ["w1:p1", "w1:p2", "w1:p3", "w2:p1", "w2:p2"]);
    }

    #[test]
    fn a_storey_is_lit_unless_everyone_idles_unfocused() {
        let mut a = agents(&["w1:p1", "w1:p2"]);
        let lit = |a: &[Agent]| storeys(a, 6).next().unwrap().lit();
        assert!(!lit(&a));
        a[1].focused = true;
        assert!(lit(&a));
        a[1].focused = false;
        a[0].status = Status::Working;
        assert!(lit(&a));
    }

    #[test]
    fn no_agents_no_storeys() {
        assert_eq!(storeys(&[], 6).count(), 0);
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
                    status: if i % 2 == 0 { Status::Unknown } else { Status::Blocked },
                    focused: i == 0,
                })
                .unwrap();
        }
        for line in lines(&state) {
            assert!(line.len() < MAX_LINE, "{} bytes", line.len());
        }
    }
}
