//! Small shared helpers.

use base64::Engine;

use crate::error::{AppError, AppResult};

/// Decode an image payload supplied by the frontend.
///
/// Accepts either a full data URL (`data:image/png;base64,<payload>`, as
/// produced by `HTMLCanvasElement.toDataURL()`) or a bare base64 string. Returns
/// `(mime, bytes)`; the mime defaults to `image/png` when not present in the URL.
pub fn parse_data_url(input: &str) -> AppResult<(String, Vec<u8>)> {
    let input = input.trim();
    let (mime, b64) = if let Some(rest) = input.strip_prefix("data:") {
        // rest = "<mime>;base64,<payload>"  (we require base64 encoding)
        let (meta, payload) = rest
            .split_once(',')
            .ok_or_else(|| AppError::BadRequest("malformed data URL".into()))?;
        if !meta.contains("base64") {
            return Err(AppError::BadRequest(
                "only base64-encoded data URLs are supported".into(),
            ));
        }
        let mime = meta.split(';').next().unwrap_or("").to_string();
        let mime = if mime.is_empty() {
            "image/png".to_string()
        } else {
            mime
        };
        (mime, payload)
    } else {
        ("image/png".to_string(), input)
    };

    let bytes = base64::engine::general_purpose::STANDARD.decode(b64)?;
    if bytes.is_empty() {
        return Err(AppError::BadRequest("image data is empty".into()));
    }
    Ok((mime, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_png_data_url() {
        // base64 of "hello"
        let (mime, bytes) = parse_data_url("data:image/png;base64,aGVsbG8=").unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn parses_jpeg_mime() {
        let (mime, _) = parse_data_url("data:image/jpeg;base64,aGVsbG8=").unwrap();
        assert_eq!(mime, "image/jpeg");
    }

    #[test]
    fn bare_base64_defaults_to_png() {
        let (mime, bytes) = parse_data_url("aGVsbG8=").unwrap();
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn rejects_non_base64_data_url() {
        assert!(parse_data_url("data:image/png,rawbytes").is_err());
    }

    #[test]
    fn rejects_garbage_base64() {
        assert!(parse_data_url("data:image/png;base64,!!!!").is_err());
    }

    #[test]
    fn rejects_empty_payload() {
        assert!(parse_data_url("data:image/png;base64,").is_err());
    }
}
