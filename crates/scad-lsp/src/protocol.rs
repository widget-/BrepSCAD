use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use serde_json::Value;

pub fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>> {
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
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("invalid Content-Length")?,
            );
        }
    }

    let length = content_length.context("missing Content-Length")?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let message = serde_json::from_slice(&body).context("invalid JSON-RPC body")?;
    Ok(Some(message))
}

pub fn write_message<W: Write>(writer: &mut W, message: &Value) -> Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
pub fn read_all(bytes: &[u8]) -> Result<Vec<Value>> {
    let mut reader = std::io::Cursor::new(bytes);
    let mut messages = Vec::new();
    while let Some(message) = read_message(&mut reader)? {
        messages.push(message);
    }
    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn messages_round_trip() {
        let mut bytes = Vec::new();
        write_message(&mut bytes, &json!({"jsonrpc": "2.0", "id": 1})).unwrap();
        write_message(&mut bytes, &json!({"jsonrpc": "2.0", "method": "exit"})).unwrap();
        let messages = read_all(&bytes).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["id"], 1);
    }

    #[test]
    fn missing_content_length_is_an_error() {
        let mut reader = std::io::Cursor::new(b"Content-Type: application/json\r\n\r\n{}".to_vec());
        assert!(read_message(&mut reader).is_err());
    }

    #[test]
    fn eof_before_headers_returns_none() {
        let mut reader = std::io::Cursor::new(Vec::new());
        assert!(read_message(&mut reader).unwrap().is_none());
    }
}
