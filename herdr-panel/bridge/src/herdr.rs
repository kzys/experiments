//! Client for herdr's Unix socket API: one JSON request line, one response line.

use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use herdr_panel_protocol::{
    ID_LEN, LABEL_LEN, State, Status, Tab, Workspace,
    heapless::{self, String as FixedString},
    truncated,
};
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

    /// The current workspaces and tabs.
    pub fn state(&self) -> Result<State, String> {
        let workspaces = self.call("workspace.list", json!({}))?;
        let tabs = self.call("tab.list", json!({}))?;
        Ok(build_state(&workspaces, &tabs))
    }

    pub fn focus_tab(&self, tab_id: &str) -> Result<(), String> {
        self.call("tab.focus", json!({"tab_id": tab_id})).map(drop)
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

/// Builds the panel's view from the `result` of `workspace.list` and `tab.list`.
/// Workspaces and tabs keep herdr's `number` order; extras beyond the panel's
/// capacity are dropped.
pub fn build_state(workspaces: &Value, tabs: &Value) -> State {
    let mut workspaces: Vec<&Value> = workspaces["workspaces"].as_array().into_iter().flatten().collect();
    workspaces.sort_by_key(|w| w["number"].as_u64().unwrap_or(u64::MAX));
    let mut tabs: Vec<&Value> = tabs["tabs"].as_array().into_iter().flatten().collect();
    tabs.sort_by_key(|t| t["number"].as_u64().unwrap_or(u64::MAX));

    let mut state = State::default();
    for w in workspaces {
        let mut ws = Workspace {
            id: str_of::<ID_LEN>(&w["workspace_id"]),
            label: str_of::<LABEL_LEN>(&w["label"]),
            status: status(&w["agent_status"]),
            focused: w["focused"].as_bool().unwrap_or(false),
            tabs: heapless::Vec::new(),
        };
        for t in tabs.iter().filter(|t| t["workspace_id"] == w["workspace_id"]) {
            let tab = Tab {
                id: str_of(&t["tab_id"]),
                label: str_of(&t["label"]),
                status: status(&t["agent_status"]),
                focused: t["focused"].as_bool().unwrap_or(false),
            };
            if ws.tabs.push(tab).is_err() {
                break;
            }
        }
        if state.workspaces.push(ws).is_err() {
            break;
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORKSPACES: &str = r#"{"type":"workspace_list","workspaces":[
        {"active_tab_id":"w2T:t3","agent_status":"idle","focused":false,"label":"statusline","number":2,"pane_count":3,"tab_count":2,"workspace_id":"w2T"},
        {"active_tab_id":"w2S:t1","agent_status":"unknown","focused":false,"label":"~","number":1,"pane_count":1,"tab_count":1,"workspace_id":"w2S"},
        {"active_tab_id":"w2V:t1","agent_status":"working","focused":true,"label":"herdr-panel","number":3,"pane_count":3,"tab_count":2,"workspace_id":"w2V"}]}"#;
    const TABS: &str = r#"{"type":"tab_list","tabs":[
        {"agent_status":"unknown","focused":false,"label":"1","number":1,"pane_count":1,"tab_id":"w2S:t1","workspace_id":"w2S"},
        {"agent_status":"idle","focused":false,"label":"1","number":1,"pane_count":2,"tab_id":"w2T:t1","workspace_id":"w2T"},
        {"agent_status":"unknown","focused":false,"label":"2","number":3,"pane_count":1,"tab_id":"w2T:t3","workspace_id":"w2T"},
        {"agent_status":"working","focused":true,"label":"1","number":1,"pane_count":2,"tab_id":"w2V:t1","workspace_id":"w2V"},
        {"agent_status":"unknown","focused":false,"label":"2","number":2,"pane_count":1,"tab_id":"w2V:t2","workspace_id":"w2V"}]}"#;

    fn state() -> State {
        build_state(&serde_json::from_str(WORKSPACES).unwrap(), &serde_json::from_str(TABS).unwrap())
    }

    #[test]
    fn workspaces_follow_herdr_numbers() {
        let state = state();
        let ids: Vec<&str> = state.workspaces.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["w2S", "w2T", "w2V"]);
    }

    #[test]
    fn tabs_are_grouped_under_their_workspace() {
        let state = state();
        let tabs = |i: usize| state.workspaces[i].tabs.iter().map(|t| t.id.as_str()).collect::<Vec<_>>();
        assert_eq!(tabs(0), ["w2S:t1"]);
        assert_eq!(tabs(1), ["w2T:t1", "w2T:t3"]);
        assert_eq!(tabs(2), ["w2V:t1", "w2V:t2"]);
    }

    #[test]
    fn status_and_focus_are_copied() {
        let state = state();
        let w = &state.workspaces[2];
        assert_eq!((w.status, w.focused), (Status::Working, true));
        assert_eq!(w.tabs[0].status, Status::Working);
        assert!(w.tabs[0].focused && !w.tabs[1].focused);
        assert_eq!(state.workspaces[0].status, Status::Unknown);
    }

    #[test]
    fn unknown_status_strings_become_unknown() {
        assert_eq!(status(&json!("something-new")), Status::Unknown);
        assert_eq!(status(&json!(null)), Status::Unknown);
    }

    #[test]
    fn missing_lists_give_an_empty_state() {
        assert_eq!(build_state(&json!({}), &json!({})), State::default());
    }
}
