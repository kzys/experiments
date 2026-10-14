//! Client for herdr's Unix socket API: one JSON request line, one response line.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use herdr_panel_protocol::{Agent, ID_LEN, LABEL_LEN, State, Status, heapless::String as FixedString, truncated};
use serde_json::{Value, json};

pub struct Herdr {
    socket: PathBuf,
}

impl Herdr {
    /// Uses `HERDR_SOCKET_PATH`, which herdr sets in every pane it manages.
    pub fn from_env() -> Result<Self, String> {
        let socket = std::env::var_os("HERDR_SOCKET_PATH").ok_or("HERDR_SOCKET_PATH is not set; pass --socket")?;
        Ok(Self { socket: socket.into() })
    }

    pub fn with_socket(socket: PathBuf) -> Self {
        Self { socket }
    }

    pub fn socket(&self) -> &std::path::Path {
        &self.socket
    }

    fn connect(&self) -> Result<UnixStream, String> {
        UnixStream::connect(&self.socket).map_err(|e| format!("herdr socket {}: {e}", self.socket.display()))
    }

    fn send(stream: &mut UnixStream, id: &str, method: &str, params: Value) -> Result<(), String> {
        let mut line = json!({"id": id, "method": method, "params": params}).to_string();
        line.push('\n');
        stream.write_all(line.as_bytes()).map_err(|e| format!("herdr write: {e}"))
    }

    /// Calls `method` and returns its `result`.
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let mut stream = self.connect()?;
        Self::send(&mut stream, "bridge", method, params)?;
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).map_err(|e| format!("herdr read: {e}"))?;
        let mut response: Value = serde_json::from_str(&line).map_err(|e| format!("herdr response: {e}: {line}"))?;
        if let Some(error) = response.get("error") {
            return Err(format!("herdr {method}: {error}"));
        }
        Ok(response["result"].take())
    }

    /// The agents herdr currently knows about.
    pub fn state(&self) -> Result<State, String> {
        let agents = self.call("agent.list", json!({}))?;
        let workspaces = self.call("workspace.list", json!({}))?;
        Ok(build_state(&agents, &workspaces))
    }

    /// Focuses the agent in pane `pane_id` (or an agent name).
    pub fn focus_agent(&self, target: &str) -> Result<(), String> {
        self.call("agent.focus", json!({"target": target})).map(drop)
    }

    /// Opens a connection that yields one line per layout change event.
    pub fn subscribe(&self) -> Result<impl Iterator<Item = std::io::Result<String>>, String> {
        let mut stream = self.connect()?;
        let types = [
            "workspace.created",
            "workspace.updated",
            "workspace.renamed",
            "workspace.closed",
            "workspace.focused",
            "workspace.reordered",
            "tab.created",
            "tab.closed",
            "tab.focused",
            "tab.renamed",
            "tab.moved",
            "pane.updated",
            "pane.focused",
            "pane.created",
            "pane.closed",
        ];
        let subscriptions: Vec<Value> = types.iter().map(|t| json!({"type": t})).collect();
        Self::send(&mut stream, "events", "events.subscribe", json!({"subscriptions": subscriptions}))?;
        let mut lines = BufReader::new(stream).lines();
        // The first line acknowledges the subscription.
        match lines.next() {
            Some(Ok(ack)) if ack.contains("subscription_started") => Ok(lines),
            Some(Ok(other)) => Err(format!("herdr subscribe: {other}")),
            Some(Err(e)) => Err(format!("herdr subscribe: {e}")),
            None => Err("herdr subscribe: connection closed".into()),
        }
    }
}

fn status(v: &Value) -> Status {
    match v.as_str() {
        Some("idle") => Status::Idle,
        Some("working") => Status::Working,
        Some("blocked") => Status::Blocked,
        Some("done") => Status::Done,
        _ => Status::Unknown,
    }
}

fn str_of<const N: usize>(v: &Value) -> FixedString<N> {
    truncated(v.as_str().unwrap_or_default())
}

/// Builds the panel's view from the `result` of `agent.list` and
/// `workspace.list`. Agents are ordered by workspace number, then pane id;
/// extras beyond the panel's capacity are dropped. The caption is the agent's
/// name, or failing that its workspace's label.
pub fn build_state(agents: &Value, workspaces: &Value) -> State {
    let workspaces: Vec<&Value> = workspaces["workspaces"].as_array().into_iter().flatten().collect();
    let number_of = |id: &Value| {
        workspaces.iter().find(|w| &w["workspace_id"] == id).and_then(|w| w["number"].as_u64()).unwrap_or(u64::MAX)
    };
    let mut agents: Vec<&Value> = agents["agents"].as_array().into_iter().flatten().collect();
    agents.sort_by_key(|a| (number_of(&a["workspace_id"]), a["pane_id"].as_str().unwrap_or_default().to_string()));

    let mut state = State::default();
    for a in agents {
        let name = a["name"].as_str().filter(|n| !n.is_empty());
        let workspace_label = workspaces.iter().find(|w| w["workspace_id"] == a["workspace_id"]).and_then(|w| w["label"].as_str());
        let agent = Agent {
            id: str_of::<ID_LEN>(&a["pane_id"]),
            label: truncated::<LABEL_LEN>(name.or(workspace_label).unwrap_or_default()),
            status: status(&a["agent_status"]),
            focused: a["focused"].as_bool().unwrap_or(false),
        };
        if state.agents.push(agent).is_err() {
            break;
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKSPACES: &str = r#"{"type":"workspace_list","workspaces":[
        {"active_tab_id":"w2T:t3","agent_status":"idle","focused":false,"label":"statusline","number":2,"workspace_id":"w2T"},
        {"active_tab_id":"w2S:t1","agent_status":"unknown","focused":false,"label":"~","number":1,"workspace_id":"w2S"},
        {"active_tab_id":"w2V:t1","agent_status":"working","focused":true,"label":"herdr-panel","number":3,"workspace_id":"w2V"}]}"#;
    const AGENTS: &str = r#"{"type":"agent_list","agents":[
        {"agent":"claude","agent_status":"working","focused":true,"name":null,"pane_id":"w2V:p1","tab_id":"w2V:t1","workspace_id":"w2V"},
        {"agent":"claude","agent_status":"idle","focused":false,"name":"reviewer","pane_id":"w2T:p1","tab_id":"w2T:t1","workspace_id":"w2T"},
        {"agent":"codex","agent_status":"blocked","focused":false,"pane_id":"w2T:p4","tab_id":"w2T:t3","workspace_id":"w2T"}]}"#;

    fn state() -> State {
        build_state(&serde_json::from_str(AGENTS).unwrap(), &serde_json::from_str(WORKSPACES).unwrap())
    }

    #[test]
    fn agents_follow_workspace_order_then_pane_id() {
        let state = state();
        let ids: Vec<&str> = state.agents.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["w2T:p1", "w2T:p4", "w2V:p1"]);
    }

    #[test]
    fn caption_is_the_name_or_the_workspace_label() {
        let state = state();
        let labels: Vec<&str> = state.agents.iter().map(|a| a.label.as_str()).collect();
        assert_eq!(labels, ["reviewer", "statusline", "herdr-panel"]);
    }

    #[test]
    fn status_and_focus_are_copied() {
        let state = state();
        let by_id = |id: &str| state.agents.iter().find(|a| a.id.as_str() == id).unwrap();
        assert_eq!((by_id("w2V:p1").status, by_id("w2V:p1").focused), (Status::Working, true));
        assert_eq!(by_id("w2T:p1").status, Status::Idle);
        assert_eq!(by_id("w2T:p4").status, Status::Blocked);
    }

    #[test]
    fn unknown_status_strings_become_unknown() {
        assert_eq!(status(&json!("something-new")), Status::Unknown);
        assert_eq!(status(&json!(null)), Status::Unknown);
    }

    #[test]
    fn extra_agents_are_dropped() {
        let many: Vec<Value> = (0..20)
            .map(|i| json!({"agent_status":"idle","focused":false,"pane_id":format!("w1:p{i}"),"workspace_id":"w1"}))
            .collect();
        let state = build_state(&json!({"agents": many}), &json!({}));
        assert_eq!(state.agents.len(), herdr_panel_protocol::MAX_AGENTS);
    }

    #[test]
    fn missing_lists_give_an_empty_state() {
        assert_eq!(build_state(&json!({}), &json!({})), State::default());
    }
}
