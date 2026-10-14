//! The panel's USB serial port: JSON lines both ways, plus the board's log text.

use std::{
    io::{BufRead, BufReader, ErrorKind, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use herdr_panel_protocol::{ToBoard, ToHost};
use serialport::SerialPort;

/// Writing side of the port, shared with the reader thread, which replaces it
/// whenever the board is plugged in again.
#[derive(Clone, Default)]
pub struct Writer(Arc<Mutex<Option<Box<dyn SerialPort>>>>);

impl Writer {
    /// Sends one message. Fails while the board is not connected.
    pub fn send(&self, msg: &ToBoard) -> Result<(), String> {
        let mut line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
        line.push('\n');
        let mut port = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let port = port.as_mut().ok_or("panel not connected")?;
        port.write_all(line.as_bytes()).and_then(|()| port.flush()).map_err(|e| format!("panel write: {e}"))
    }

    fn set(&self, port: Option<Box<dyn SerialPort>>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = port;
    }
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
    let mut port = match serialport::new(path, 115_200).timeout(Duration::from_millis(500)).open() {
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
    fn json_lines_are_messages() {
        assert_eq!(parse_line("\"Hello\"\n"), Ok(ToHost::Hello));
        assert_eq!(parse_line(r#"{"Focus":{"tab":"w1:t2"}}"#), Ok(ToHost::Focus { tab: truncated("w1:t2") }));
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
