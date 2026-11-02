use std::io::{BufRead, Write};

use anyhow::Result;
use serde_json::{Value, json};
use url::Url;

use crate::analysis;
use crate::documents::Documents;
use crate::positions::{Position, Range, position_to_offset, range_to_lsp};

#[derive(Debug, Default)]
pub struct Server {
    documents: Documents,
    shutdown: bool,
    exit: bool,
}

pub fn run<R: BufRead, W: Write>(mut reader: R, mut writer: W) -> Result<()> {
    let mut server = Server::default();
    while let Some(message) = crate::protocol::read_message(&mut reader)? {
        for response in server.handle(message) {
            crate::protocol::write_message(&mut writer, &response)?;
        }
        if server.exit {
            break;
        }
    }
    Ok(())
}

impl Server {
    pub fn handle(&mut self, message: Value) -> Vec<Value> {
        let id = message.get("id").cloned();
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = message.get("params").cloned().unwrap_or(Value::Null);

        match method {
            "initialize" => response(
                id,
                json!({
                    "capabilities": {
                        "textDocumentSync": 1,
                        "completionProvider": { "triggerCharacters": ["$"] },
                        "hoverProvider": true,
                        "definitionProvider": true,
                        "documentSymbolProvider": true
                    },
                    "serverInfo": {
                        "name": "brepscad-lsp",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            ),
            "initialized" => Vec::new(),
            "shutdown" => {
                self.shutdown = true;
                response(id, Value::Null)
            }
            "exit" => {
                self.exit = true;
                Vec::new()
            }
            "textDocument/didOpen" => {
                let Some(document) = params.get("textDocument") else {
                    return Vec::new();
                };
                let Some(uri) = document.get("uri").and_then(Value::as_str) else {
                    return Vec::new();
                };
                let text = document
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let version = document.get("version").and_then(Value::as_i64).unwrap_or(0) as i32;
                self.documents.open(uri.to_owned(), version, text);
                self.publish(uri)
            }
            "textDocument/didChange" => {
                let Some(uri) = text_document_uri(&params) else {
                    return Vec::new();
                };
                let version = params
                    .get("textDocument")
                    .and_then(|document| document.get("version"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32;
                let Some(text) = params
                    .get("contentChanges")
                    .and_then(Value::as_array)
                    .and_then(|changes| changes.last())
                    .and_then(|change| change.get("text"))
                    .and_then(Value::as_str)
                else {
                    return Vec::new();
                };
                self.documents.change(&uri, version, text.to_owned());
                self.publish(&uri)
            }
            "textDocument/didSave" => text_document_uri(&params)
                .map(|uri| self.publish(&uri))
                .unwrap_or_default(),
            "textDocument/didClose" => {
                let Some(uri) = text_document_uri(&params) else {
                    return Vec::new();
                };
                self.documents.close(&uri);
                vec![json!({
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": { "uri": uri, "diagnostics": [] }
                })]
            }
            "textDocument/completion" => self.completion(id, &params),
            "textDocument/hover" => self.hover(id, &params),
            "textDocument/definition" => self.definition(id, &params),
            "textDocument/documentSymbol" => self.document_symbols(id, &params),
            _ => id
                .map(|id| vec![error_response(id, -32601, "method not found")])
                .unwrap_or_default(),
        }
    }

    fn publish(&self, uri: &str) -> Vec<Value> {
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        let diagnostics: Vec<Value> = analysis::diagnostics(&document.text)
            .into_iter()
            .map(|diagnostic| {
                json!({
                    "range": range_json(&document.text, diagnostic.range),
                    "severity": diagnostic.severity,
                    "source": "brepscad",
                    "message": diagnostic.message
                })
            })
            .collect();
        vec![json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": uri,
                "version": document.version,
                "diagnostics": diagnostics
            }
        })]
    }

    fn completion(&self, id: Option<Value>, params: &Value) -> Vec<Value> {
        let Some(document) = self.document(params) else {
            return response(id, Value::Null);
        };
        let Some(position) = position(params) else {
            return response(id, Value::Null);
        };
        let offset = position_to_offset(&document.text, position);
        let items: Vec<Value> = analysis::completion(&document.text, offset)
            .into_iter()
            .map(|item| {
                json!({
                    "label": item.label,
                    "kind": item.kind,
                    "detail": item.detail,
                    "insertText": item.insert_text,
                    "insertTextFormat": 1
                })
            })
            .collect();
        response(id, Value::Array(items))
    }

    fn hover(&self, id: Option<Value>, params: &Value) -> Vec<Value> {
        let Some(document) = self.document(params) else {
            return response(id, Value::Null);
        };
        let Some(position) = position(params) else {
            return response(id, Value::Null);
        };
        let offset = position_to_offset(&document.text, position);
        let result = analysis::hover(&document.text, offset).map_or(Value::Null, |hover| {
            json!({
                "contents": { "kind": "markdown", "value": hover.contents },
                "range": range_json(&document.text, hover.range)
            })
        });
        response(id, result)
    }

    fn definition(&self, id: Option<Value>, params: &Value) -> Vec<Value> {
        let Some(uri) = text_document_uri(params) else {
            return response(id, Value::Null);
        };
        let Some(document) = self.documents.get(&uri) else {
            return response(id, Value::Null);
        };
        let Some(position) = position(params) else {
            return response(id, Value::Null);
        };
        let offset = position_to_offset(&document.text, position);
        let path = uri_to_path(&uri);
        let result = analysis::definition(&document.text, path.as_deref(), offset).map_or(
            Value::Null,
            |definition| {
                let target_uri = definition
                    .path
                    .as_deref()
                    .and_then(path_to_uri)
                    .unwrap_or_else(|| uri.clone());
                json!({
                    "uri": target_uri,
                    "range": range_json(&document.text, definition.range)
                })
            },
        );
        response(id, result)
    }

    fn document_symbols(&self, id: Option<Value>, params: &Value) -> Vec<Value> {
        let Some(uri) = text_document_uri(params) else {
            return response(id, Value::Null);
        };
        let Some(document) = self.documents.get(&uri) else {
            return response(id, Value::Null);
        };
        let symbols: Vec<Value> = analysis::symbols(&document.text)
            .into_iter()
            .map(|symbol| {
                json!({
                    "name": symbol.name,
                    "kind": symbol.kind,
                    "location": {
                        "uri": uri,
                        "range": range_json(&document.text, symbol.range)
                    }
                })
            })
            .collect();
        response(id, Value::Array(symbols))
    }

    fn document(&self, params: &Value) -> Option<&crate::documents::Document> {
        self.documents.get(&text_document_uri(params)?)
    }
}

fn response(id: Option<Value>, result: Value) -> Vec<Value> {
    id.into_iter()
        .map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": result }))
        .collect()
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

fn text_document_uri(params: &Value) -> Option<String> {
    params
        .get("textDocument")
        .and_then(|document| document.get("uri"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn position(params: &Value) -> Option<Position> {
    serde_json::from_value(params.get("position")?.clone()).ok()
}

fn range_json(text: &str, range: std::ops::Range<usize>) -> Value {
    let Range { start, end } = range_to_lsp(text, range);
    json!({ "start": start, "end": end })
}

fn path_to_uri(path: &std::path::Path) -> Option<String> {
    Url::from_file_path(path).ok().map(|url| url.to_string())
}

fn uri_to_path(uri: &str) -> Option<std::path::PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{read_all, write_message};
    use std::io::Cursor;

    fn run_messages(messages: Vec<Value>) -> Vec<Value> {
        let mut input = Vec::new();
        for message in messages {
            write_message(&mut input, &message).unwrap();
        }
        let mut output = Vec::new();
        run(Cursor::new(input), &mut output).unwrap();
        read_all(&output).unwrap()
    }

    #[test]
    fn initialize_reports_capabilities() {
        let messages = run_messages(vec![json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        })]);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["result"]["capabilities"]["textDocumentSync"], 1);
    }

    #[test]
    fn did_open_publishes_parse_diagnostics() {
        let messages = run_messages(vec![json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": "file:///tmp/test.scad",
                    "version": 1,
                    "text": "cube("
                }
            }
        })]);
        assert_eq!(messages[0]["method"], "textDocument/publishDiagnostics");
        assert!(
            !messages[0]["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn completion_and_symbols_work_over_stdio_messages() {
        let messages = run_messages(vec![
            json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": "file:///tmp/test.scad",
                        "version": 1,
                        "text": "module bracket(w) { cube(w); }\n"
                    }
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "textDocument/completion",
                "params": {
                    "textDocument": { "uri": "file:///tmp/test.scad" },
                    "position": { "line": 1, "character": 0 }
                }
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "textDocument/documentSymbol",
                "params": { "textDocument": { "uri": "file:///tmp/test.scad" } }
            }),
        ]);
        let completion = messages.iter().find(|m| m["id"] == 2).unwrap();
        assert!(
            completion["result"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["insertText"] == "bracket")
        );
        let symbols = messages.iter().find(|m| m["id"] == 3).unwrap();
        assert!(
            symbols["result"]
                .as_array()
                .unwrap()
                .iter()
                .any(|symbol| symbol["name"] == "bracket")
        );
    }
}
