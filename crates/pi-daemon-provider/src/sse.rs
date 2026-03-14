//! Server-Sent Events (SSE) parser for reqwest response streams.

use futures::StreamExt;
use tokio_stream::Stream;

/// A parsed SSE event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// The event type (from `event:` field). Empty if not specified.
    pub event: String,
    /// The data payload (from `data:` field(s)).
    pub data: String,
}

/// Parse a reqwest response body into a stream of SSE events.
///
/// Splits incoming bytes on `\n\n` boundaries and extracts `event:` and `data:` fields.
/// Comment lines (starting with `:`) are ignored.
pub fn parse_sse(
    response: reqwest::Response,
) -> impl Stream<Item = Result<SseEvent, String>> + Send {
    let byte_stream = response.bytes_stream();

    async_stream::stream! {
        let mut buffer = String::new();

        futures::pin_mut!(byte_stream);

        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    yield Err(format!("Stream read error: {e}"));
                    return;
                }
            };

            let text = match std::str::from_utf8(&chunk) {
                Ok(t) => t,
                Err(e) => {
                    yield Err(format!("UTF-8 decode error: {e}"));
                    return;
                }
            };

            buffer.push_str(text);

            // Process complete events (separated by \n\n)
            while let Some(pos) = buffer.find("\n\n") {
                let event_text = buffer[..pos].to_string();
                buffer = buffer[pos + 2..].to_string();

                if event_text.trim().is_empty() {
                    continue;
                }

                let mut event_type = String::new();
                let mut data_lines: Vec<String> = Vec::new();

                for line in event_text.lines() {
                    if let Some(value) = line.strip_prefix("event:") {
                        event_type = value.trim().to_string();
                    } else if let Some(value) = line.strip_prefix("data:") {
                        // SSE spec: single space after colon is optional but conventional
                        data_lines.push(value.strip_prefix(' ').unwrap_or(value).to_string());
                    } else if line.starts_with(':') {
                        // Comment line, skip
                    }
                }

                let data = data_lines.join("\n");

                if !data.is_empty() || !event_type.is_empty() {
                    yield Ok(SseEvent {
                        event: event_type,
                        data,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sse_event_debug() {
        let event = SseEvent {
            event: "message".to_string(),
            data: r#"{"text": "hello"}"#.to_string(),
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("message"));
        assert!(debug.contains("hello"));
    }
}
