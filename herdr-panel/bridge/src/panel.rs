//! The panel's USB serial port: JSON lines both ways, plus the board's log text.

use std::{
    io::{BufRead, BufReader, ErrorKind, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use herdr_panel_protocol::{State, ToBoard, ToHost, messages};
use serialport::SerialPort;

/// Writing side of the port, shared with the reader thread, which replaces it
/// whenever the board is plugged in again.
#[derive(Clone, Default)]
pub struct Writer(Arc<Mutex<Option<Box<dyn SerialPort>>>>);

impl Writer {
    /// Sends `state`, one message per line, in one write. Fails while the
    /// board is not connected.
    pub fn send_state(&self, state: &State) -> Result<(), String> {
        self.write(&state_lines(state)?)
    }

    /// Sends one message on its own line. Fails while the board is not
    /// connected.
    pub fn send(&self, msg: &ToBoard) -> Result<(), String> {
        let mut line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        line.push('\n');
        self.write(&line)
    }

    fn write(&self, text: &str) -> Result<(), String> {
        let mut port = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let port = port.as_mut().ok_or("panel not connected")?;
        port.write_all(text.as_bytes())
            .and_then(|()| port.flush())
            .map_err(|e| format!("panel write: {e}"))
    }

    fn set(&self, port: Option<Box<dyn SerialPort>>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = port;
    }
}

/// The lines that send `state` to the board.
fn state_lines(state: &State) -> Result<String, String> {
    let mut text = String::new();
    for msg in messages(state) {
        text.push_str(&serde_json::to_string(&msg).map_err(|e| e.to_string())?);
        text.push('\n');
    }
    Ok(text)
}

/// What the reader thread reports.
pub enum Event {
    Connected,
    Message(ToHost),
}

/// Classifies one line from the board: a message, or log text to show.
pub fn parse_line(line: &str) -> Result<ToHost, &str> {
    let line = line.trim();
    if !line.starts_with('{') && !line.starts_with('"') {
        return Err(line);
    }
    serde_json::from_str(line).map_err(|_| line)
}

/// Opens `path` and reads lines until the port fails, forwarding messages to
/// `on_event`. Board log lines are printed to stderr. Returns the error that
/// ended the session.
pub fn session(path: &str, writer: &Writer, mut on_event: impl FnMut(Event)) -> String {
    let mut port = match serialport::new(path, 115_200)
        .timeout(Duration::from_millis(500))
        .open()
    {
        Ok(p) => p,
        Err(e) => return format!("open {path}: {e}"),
    };
    // On the board's USB-Serial-JTAG, DTR and RTS drive reset and boot mode.
    // Errors are ignored: not every port (a pty, for example) has these lines.
    let _ = port.write_data_terminal_ready(false);
    let _ = port.write_request_to_send(false);
    let reader = match port.try_clone() {
        Ok(r) => r,
        Err(e) => return format!("clone {path}: {e}"),
    };
    writer.set(Some(port));
    on_event(Event::Connected);

    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let error = loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break "port closed".to_string(),
            Ok(_) => match parse_line(&line) {
                Ok(msg) => on_event(Event::Message(msg)),
                Err(text) if !text.is_empty() => eprintln!("board: {text}"),
                Err(_) => {}
            },
            Err(e) if e.kind() == ErrorKind::TimedOut => {}
            Err(e) => break format!("read: {e}"),
        }
    };
    writer.set(None);
    error
}

#[cfg(test)]
mod tests {
    use herdr_panel_protocol::truncated;

    use super::*;

    #[test]
    fn a_state_is_one_line_per_message() {
        let text = state_lines(&State::default()).unwrap();
        assert_eq!(text, "{\"Begin\":{\"count\":0}}\n\"End\"\n");
    }

    #[test]
    fn json_lines_are_messages() {
        assert_eq!(parse_line("\"Hello\"\n"), Ok(ToHost::Hello));
        assert_eq!(
            parse_line(r#"{"Focus":{"agent":"w1:p2"}}"#),
            Ok(ToHost::Focus {
                agent: truncated("w1:p2")
            })
        );
    }

    #[test]
    fn other_lines_are_board_logs() {
        assert_eq!(parse_line("INFO - panel up\r\n"), Err("INFO - panel up"));
        assert_eq!(parse_line("\r\n"), Err(""));
    }

    #[test]
    fn malformed_json_is_returned_as_text() {
        assert_eq!(parse_line("{\"Focus\":"), Err("{\"Focus\":"));
    }
}
