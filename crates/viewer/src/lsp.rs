use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

use serde_json::{Value, json};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone)]
pub struct LspDiagnostic {
    pub range: Range,
    pub severity: u8,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub insert_text: String,
    pub detail: String,
    pub kind: u8,
}

#[derive(Debug, Clone)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

#[derive(Debug)]
pub enum Event {
    Diagnostics {
        uri: String,
        diagnostics: Vec<LspDiagnostic>,
    },
    Completion {
        id: u64,
        items: Vec<CompletionItem>,
    },
    Hover {
        id: u64,
        text: Option<String>,
    },
    Definition {
        id: u64,
        location: Option<Location>,
    },
    Log(String),
}

#[derive(Debug, Clone, Copy)]
enum Pending {
    Completion,
    Hover,
    Definition,
}

pub struct LspClient {
    to_server: Sender<Value>,
    from_server: Receiver<Value>,
    child: Child,
    uri: Option<String>,
    version: i64,
    next_id: u64,
    pending: HashMap<u64, Pending>,
    initialized: bool,
}

impl LspClient {
    pub fn start() -> Result<Self, String> {
        let server = find_server().ok_or_else(|| "brepscad-lsp not found".to_owned())?;
        let mut child = Command::new(&server)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("failed to start {}: {error}", server.display()))?;
        let stdin = child.stdin.take().ok_or("missing language server stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("missing language server stdout")?;

        let (to_writer, writer_rx) = mpsc::channel::<Value>();
        std::thread::spawn(move || {
            let mut writer = stdin;
            while let Ok(message) = writer_rx.recv() {
                if write_message(&mut writer, &message).is_err() {
                    break;
                }
            }
        });

        let (reader_tx, from_server) = mpsc::channel::<Value>();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(message)) = read_message(&mut reader) {
                if reader_tx.send(message).is_err() {
                    break;
                }
            }
        });

        let client = Self {
            to_server: to_writer,
            from_server,
            child,
            uri: None,
            version: 0,
            next_id: 1,
            pending: HashMap::new(),
            initialized: false,
        };
        client.send(json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "processId": null,
                "rootUri": null,
                "capabilities": {}
            }
        }));
        Ok(client)
    }

    pub fn open(&mut self, uri: String, text: &str) {
        if let Some(previous) = self.uri.take() {
            self.notify(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": previous } }),
            );
        }
        self.uri = Some(uri.clone());
        self.version = 1;
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "scad",
                    "version": self.version,
                    "text": text
                }
            }),
        );
    }

    pub fn change(&mut self, text: &str) {
        let Some(uri) = self.uri.clone() else {
            return;
        };
        self.version += 1;
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": uri, "version": self.version },
                "contentChanges": [{ "text": text }]
            }),
        );
    }

    pub fn save(&mut self) {
        let Some(uri) = self.uri.clone() else {
            return;
        };
        self.notify(
            "textDocument/didSave",
            json!({ "textDocument": { "uri": uri } }),
        );
    }

    pub fn request_completion(&mut self, position: Position) -> Option<u64> {
        self.request(
            Pending::Completion,
            "textDocument/completion",
            json!({
                "textDocument": { "uri": self.uri.clone()? },
                "position": { "line": position.line, "character": position.character }
            }),
        )
    }

    pub fn request_hover(&mut self, position: Position) -> Option<u64> {
        self.request(
            Pending::Hover,
            "textDocument/hover",
            json!({
                "textDocument": { "uri": self.uri.clone()? },
                "position": { "line": position.line, "character": position.character }
            }),
        )
    }

    pub fn request_definition(&mut self, position: Position) -> Option<u64> {
        self.request(
            Pending::Definition,
            "textDocument/definition",
            json!({
                "textDocument": { "uri": self.uri.clone()? },
                "position": { "line": position.line, "character": position.character }
            }),
        )
    }

    pub fn poll(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            match self.from_server.try_recv() {
                Ok(message) => {
                    if let Some(event) = self.handle_message(message) {
                        events.push(event);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    events.push(Event::Log("language server stopped".to_owned()));
                    break;
                }
            }
        }
        events
    }

    fn request(&mut self, pending: Pending, method: &str, params: Value) -> Option<u64> {
        self.uri.as_ref()?;
        let id = self.next_id;
        self.next_id += 1;
        self.pending.insert(id, pending);
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }));
        Some(id)
    }

    fn handle_message(&mut self, message: Value) -> Option<Event> {
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            if id == 0 {
                if !self.initialized {
                    self.initialized = true;
                    self.notify("initialized", json!({}));
                }
                return None;
            }
            let pending = self.pending.remove(&id)?;
            return match pending {
                Pending::Completion => Some(Event::Completion {
                    id,
                    items: completion_items(message.get("result")),
                }),
                Pending::Hover => Some(Event::Hover {
                    id,
                    text: hover_text(message.get("result")),
                }),
                Pending::Definition => Some(Event::Definition {
                    id,
                    location: definition_location(message.get("result")),
                }),
            };
        }

        let method = message.get("method").and_then(Value::as_str)?;
        if method == "textDocument/publishDiagnostics" {
            let params = message.get("params")?;
            let uri = params.get("uri").and_then(Value::as_str)?.to_owned();
            let diagnostics = params
                .get("diagnostics")
                .and_then(Value::as_array)
                .map(|diagnostics| {
                    diagnostics
                        .iter()
                        .filter_map(parse_diagnostic)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            return Some(Event::Diagnostics { uri, diagnostics });
        }
        None
    }

    fn notify(&self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn send(&self, message: Value) {
        let _ = self.to_server.send(message);
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

pub fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut offset = offset.min(source.len());
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &source[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    Position {
        line,
        character: source[line_start..offset].encode_utf16().count() as u32,
    }
}

pub fn position_to_offset(source: &str, position: Position) -> usize {
    let mut line_start = 0;
    for _ in 0..position.line {
        let Some(newline) = source[line_start..].find('\n') else {
            return source.len();
        };
        line_start += newline + 1;
    }
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |newline| line_start + newline);
    let mut units = 0;
    for (byte, character) in source[line_start..line_end].char_indices() {
        if units >= position.character as usize {
            return line_start + byte;
        }
        units += character.len_utf16();
        if units > position.character as usize {
            return line_start + byte;
        }
    }
    line_end
}

fn parse_diagnostic(value: &Value) -> Option<LspDiagnostic> {
    Some(LspDiagnostic {
        range: parse_range(value.get("range")?)?,
        severity: value.get("severity").and_then(Value::as_u64).unwrap_or(1) as u8,
        message: value.get("message")?.as_str()?.to_owned(),
    })
}

fn completion_items(result: Option<&Value>) -> Vec<CompletionItem> {
    let items = match result {
        Some(Value::Array(items)) => items.as_slice(),
        Some(Value::Object(object)) => object
            .get("items")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default(),
        _ => &[],
    };
    items
        .iter()
        .filter_map(|item| {
            Some(CompletionItem {
                label: item.get("label")?.as_str()?.to_owned(),
                insert_text: item
                    .get("insertText")
                    .or_else(|| item.get("label"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                detail: item
                    .get("detail")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                kind: item.get("kind").and_then(Value::as_u64).unwrap_or(1) as u8,
            })
        })
        .collect()
}

fn hover_text(result: Option<&Value>) -> Option<String> {
    let result = result?;
    match result.get("contents")? {
        Value::String(text) => Some(text.clone()),
        Value::Object(object) => object
            .get("value")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

fn definition_location(result: Option<&Value>) -> Option<Location> {
    let result = result?;
    let result = match result {
        Value::Array(items) => items.first()?,
        value => value,
    };
    Some(Location {
        uri: result.get("uri")?.as_str()?.to_owned(),
        range: parse_range(result.get("range")?)?,
    })
}

fn parse_range(value: &Value) -> Option<Range> {
    Some(Range {
        start: parse_position(value.get("start")?)?,
        end: parse_position(value.get("end")?)?,
    })
}

fn parse_position(value: &Value) -> Option<Position> {
    Some(Position {
        line: value.get("line")?.as_u64()? as u32,
        character: value.get("character")?.as_u64()? as u32,
    })
}

pub fn file_uri(path: &Path) -> Option<String> {
    Url::from_file_path(path).ok().map(|url| url.to_string())
}

pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

fn find_server() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("BREPSCAD_LSP") {
        return Some(PathBuf::from(path));
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        let candidate = directory.join("brepscad-lsp");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    Some(PathBuf::from("brepscad-lsp"))
}

fn read_message<R: BufRead>(reader: &mut R) -> std::io::Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
    let length = content_length.ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "missing Content-Length")
    })?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(std::io::Error::other)
}

fn write_message<W: Write>(writer: &mut W, message: &Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(message).map_err(std::io::Error::other)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_use_utf16_units() {
        let source = "aé\n😀b";
        assert_eq!(
            offset_to_position(source, 4),
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            offset_to_position(source, 9),
            Position {
                line: 1,
                character: 3
            }
        );
        assert_eq!(
            position_to_offset(
                source,
                Position {
                    line: 1,
                    character: 3
                }
            ),
            9
        );
    }

    #[test]
    fn diagnostic_payloads_parse() {
        let message = json!({
            "params": {
                "uri": "file:///a.scad",
                "diagnostics": [{
                    "range": {
                        "start": { "line": 1, "character": 2 },
                        "end": { "line": 1, "character": 4 }
                    },
                    "severity": 2,
                    "message": "unknown thing"
                }]
            }
        });
        let diagnostic = parse_diagnostic(&message["params"]["diagnostics"][0]).unwrap();
        assert_eq!(diagnostic.severity, 2);
        assert_eq!(diagnostic.range.start.character, 2);
    }
}
