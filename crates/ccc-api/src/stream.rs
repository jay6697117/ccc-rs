//! SSE stream parser for the Anthropic streaming Messages API.
//! Parses `data: {...}` lines and decodes them into `StreamEvent`.

use bytes::Bytes;
use futures_util::Stream;
use std::pin::Pin;

use crate::{error::ApiError, types::StreamEvent};

/// A pinned, boxed stream of `StreamEvent` results.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<StreamEvent, ApiError>> + Send>>;

/// Parse a raw SSE byte stream from reqwest into a typed `EventStream`.
///
/// The Anthropic API sends lines like:
/// ```text
/// event: content_block_delta
/// data: {"type":"content_block_delta",...}
/// ```
/// We only care about `data:` lines; the `event:` line is redundant because
/// the JSON payload carries `"type"` itself.
pub fn parse_sse(byte_stream: impl Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static) -> EventStream {
    use futures_util::StreamExt;

    let event_stream = async_stream::try_stream! {
        let mut buf = String::new();

        futures_util::pin_mut!(byte_stream);
        while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk.map_err(ApiError::Http)?;
            let text = std::str::from_utf8(&chunk)
                .map_err(|e| ApiError::SseParse(e.to_string()))?;
            buf.push_str(text);

            // Process all complete lines in the buffer.
            while let Some(newline_pos) = buf.find('\n') {
                let line = buf[..newline_pos].trim_end_matches('\r').to_owned();
                buf.drain(..=newline_pos);

                if let Some(json) = line.strip_prefix("data: ") {
                    if json == "[DONE]" {
                        return;
                    }
                    let event: StreamEvent = serde_json::from_str(json)
                        .map_err(|e| ApiError::SseParse(format!("{e}: {json}")))? ;
                    yield event;
                }
                // Skip `event:`, `id:`, comment (`:`) lines.
            }
        }
    };

    Box::pin(event_stream)
}
