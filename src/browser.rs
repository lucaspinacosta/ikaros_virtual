//! Native messaging boundary for an opt-in browser extension.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};

use crate::{
    collectors::{BrowserEvent, browser_event},
    events::record,
};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NativeRequest {
    TabChanged { title: String, url: String },
    DownloadCompleted { path: String },
    MediaChanged { playing: bool },
}

#[derive(Serialize)]
struct NativeResponse<'a> {
    accepted: bool,
    message: &'a str,
}

pub fn run_native_host() -> io::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    while let Some(payload) = read_message(&mut input)? {
        let response = match serde_json::from_slice::<NativeRequest>(&payload) {
            Ok(request) => {
                let event = match request {
                    NativeRequest::TabChanged { title, url } => {
                        browser_event(BrowserEvent::TabChanged { title, url })
                    }
                    NativeRequest::DownloadCompleted { path } => {
                        browser_event(BrowserEvent::DownloadCompleted(path))
                    }
                    NativeRequest::MediaChanged { playing } => {
                        browser_event(BrowserEvent::MediaChanged { playing })
                    }
                };
                record(&event)
                    .map(|_| NativeResponse {
                        accepted: true,
                        message: "recorded",
                    })
                    .unwrap_or(NativeResponse {
                        accepted: false,
                        message: "could not record event",
                    })
            }
            Err(_) => NativeResponse {
                accepted: false,
                message: "invalid native message",
            },
        };
        write_message(
            &mut output,
            &serde_json::to_vec(&response).expect("response is serializable"),
        )?;
    }
    Ok(())
}

fn read_message(reader: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut length = [0; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length > 1_048_576 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native message exceeds 1 MiB",
        ));
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_message(writer: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    writer.write_all(&(payload.len() as u32).to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_messages_use_little_endian_framing() {
        let mut input = Vec::new();
        input.extend_from_slice(&3u32.to_le_bytes());
        input.extend_from_slice(b"owl");
        assert_eq!(
            read_message(&mut input.as_slice()).unwrap(),
            Some(b"owl".to_vec())
        );
    }
}
