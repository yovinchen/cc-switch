use http::HeaderMap;
use std::io::Read;

use crate::strip_entity_headers_for_rebuilt_body;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseBodyDecodeStatus {
    NotEncoded,
    Decoded { encoding: String },
    UnsupportedEncoding { encoding: String },
    DecodeFailed { encoding: String, error: String },
}

impl ResponseBodyDecodeStatus {
    pub fn content_encoding(&self) -> Option<&str> {
        match self {
            Self::NotEncoded => None,
            Self::Decoded { encoding }
            | Self::UnsupportedEncoding { encoding }
            | Self::DecodeFailed { encoding, .. } => Some(encoding),
        }
    }

    pub fn log_event(&self) -> Option<ResponseBodyDecodeLogEvent> {
        match self {
            Self::NotEncoded => None,
            Self::Decoded { encoding } => Some(ResponseBodyDecodeLogEvent::Decoded {
                encoding: encoding.clone(),
            }),
            Self::UnsupportedEncoding { encoding } => {
                Some(ResponseBodyDecodeLogEvent::UnsupportedEncoding {
                    encoding: encoding.clone(),
                })
            }
            Self::DecodeFailed { encoding, error } => {
                Some(ResponseBodyDecodeLogEvent::DecodeFailed {
                    encoding: encoding.clone(),
                    error: error.clone(),
                })
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseBodyDecodeLogLevel {
    Debug,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseBodyDecodeLogEvent {
    Decoded { encoding: String },
    UnsupportedEncoding { encoding: String },
    DecodeFailed { encoding: String, error: String },
}

impl ResponseBodyDecodeLogEvent {
    pub fn level(&self) -> ResponseBodyDecodeLogLevel {
        match self {
            Self::Decoded { .. } => ResponseBodyDecodeLogLevel::Debug,
            Self::UnsupportedEncoding { .. } | Self::DecodeFailed { .. } => {
                ResponseBodyDecodeLogLevel::Warn
            }
        }
    }

    pub fn message(&self, tag: &str) -> String {
        match self {
            Self::Decoded { encoding } => {
                format!("[{tag}] 解压非流式响应: content-encoding={encoding}")
            }
            Self::UnsupportedEncoding { encoding } => {
                format!("未知的 content-encoding: {encoding}，跳过解压")
            }
            Self::DecodeFailed { encoding, error } => {
                format!("[{tag}] 解压失败 ({encoding}): {error}，使用原始数据")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseBodyDecode {
    pub body: Vec<u8>,
    pub status: ResponseBodyDecodeStatus,
}

/// Decode a response body according to `content-encoding`.
///
/// `Ok(None)` means the encoding is intentionally unsupported and callers must
/// keep the original body and the `content-encoding` header.
pub fn decompress_body(
    content_encoding: &str,
    body: &[u8],
) -> Result<Option<Vec<u8>>, std::io::Error> {
    match content_encoding {
        "gzip" | "x-gzip" => {
            let mut decoder = flate2::read::GzDecoder::new(body);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(Some(decompressed))
        }
        "deflate" => {
            let mut decompressed = Vec::new();
            let mut zlib = flate2::read::ZlibDecoder::new(body);
            match zlib.read_to_end(&mut decompressed) {
                Ok(_) => Ok(Some(decompressed)),
                Err(_) => {
                    let mut decompressed = Vec::new();
                    let mut raw = flate2::read::DeflateDecoder::new(body);
                    raw.read_to_end(&mut decompressed)?;
                    Ok(Some(decompressed))
                }
            }
        }
        "br" => {
            let mut decompressed = Vec::new();
            brotli::BrotliDecompress(&mut std::io::Cursor::new(body), &mut decompressed)?;
            Ok(Some(decompressed))
        }
        _ => Ok(None),
    }
}

pub fn decode_response_body(headers: &mut HeaderMap, raw_body: &[u8]) -> ResponseBodyDecode {
    let Some(encoding) = get_content_encoding(headers) else {
        return ResponseBodyDecode {
            body: raw_body.to_vec(),
            status: ResponseBodyDecodeStatus::NotEncoded,
        };
    };

    match decompress_body(&encoding, raw_body) {
        Ok(Some(decompressed)) => {
            strip_entity_headers_for_rebuilt_body(headers);
            ResponseBodyDecode {
                body: decompressed,
                status: ResponseBodyDecodeStatus::Decoded { encoding },
            }
        }
        Ok(None) => ResponseBodyDecode {
            body: raw_body.to_vec(),
            status: ResponseBodyDecodeStatus::UnsupportedEncoding { encoding },
        },
        Err(error) => ResponseBodyDecode {
            body: raw_body.to_vec(),
            status: ResponseBodyDecodeStatus::DecodeFailed {
                encoding,
                error: error.to_string(),
            },
        },
    }
}

/// Extract response `content-encoding`, ignoring empty values and `identity`.
pub fn get_content_encoding(headers: &HeaderMap) -> Option<String> {
    headers
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty() && value != "identity")
}

#[cfg(test)]
mod tests {
    use super::{
        decode_response_body, decompress_body, get_content_encoding, ResponseBodyDecodeLogLevel,
        ResponseBodyDecodeStatus,
    };
    use http::{
        header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING},
        HeaderMap, HeaderValue,
    };
    use std::io::Write;

    #[test]
    fn extracts_supported_content_encoding_values() {
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", HeaderValue::from_static("GZip"));

        assert_eq!(get_content_encoding(&headers), Some("gzip".to_string()));
    }

    #[test]
    fn skips_identity_and_blank_content_encoding_values() {
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", HeaderValue::from_static("identity"));
        assert_eq!(get_content_encoding(&headers), None);

        headers.insert("content-encoding", HeaderValue::from_static(""));
        assert_eq!(get_content_encoding(&headers), None);
    }

    #[test]
    fn response_body_decode_status_projects_log_events() {
        let decoded = ResponseBodyDecodeStatus::Decoded {
            encoding: "gzip".to_string(),
        }
        .log_event()
        .expect("decoded log event");
        assert_eq!(decoded.level(), ResponseBodyDecodeLogLevel::Debug);
        assert_eq!(
            decoded.message("REQ-1"),
            "[REQ-1] 解压非流式响应: content-encoding=gzip"
        );

        let unsupported = ResponseBodyDecodeStatus::UnsupportedEncoding {
            encoding: "zstd".to_string(),
        }
        .log_event()
        .expect("unsupported log event");
        assert_eq!(unsupported.level(), ResponseBodyDecodeLogLevel::Warn);
        assert_eq!(
            unsupported.message("REQ-1"),
            "未知的 content-encoding: zstd，跳过解压"
        );

        let failed = ResponseBodyDecodeStatus::DecodeFailed {
            encoding: "gzip".to_string(),
            error: "invalid gzip".to_string(),
        }
        .log_event()
        .expect("failed log event");
        assert_eq!(failed.level(), ResponseBodyDecodeLogLevel::Warn);
        assert_eq!(
            failed.message("REQ-1"),
            "[REQ-1] 解压失败 (gzip): invalid gzip，使用原始数据"
        );

        assert_eq!(ResponseBodyDecodeStatus::NotEncoded.log_event(), None);
    }

    #[test]
    fn decompresses_gzip_body() {
        let payload = br#"{"ok":true}"#;
        let mut encoder =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let decompressed = decompress_body("gzip", &compressed).unwrap().unwrap();

        assert_eq!(decompressed, payload);
    }

    #[test]
    fn decompresses_brotli_body() {
        let payload = br#"{"ok":true}"#;
        let mut compressed = Vec::new();
        {
            let mut writer = brotli::CompressorWriter::new(&mut compressed, 4096, 5, 22);
            writer.write_all(payload).unwrap();
        }

        let decompressed = decompress_body("br", &compressed).unwrap().unwrap();

        assert_eq!(decompressed, payload);
    }

    #[test]
    fn decompresses_zlib_wrapped_deflate_body() {
        let payload = br#"{"ok":true}"#;
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let decompressed = decompress_body("deflate", &compressed).unwrap().unwrap();

        assert_eq!(decompressed, payload);
    }

    #[test]
    fn decompresses_raw_deflate_body() {
        let payload = br#"{"ok":true}"#;
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        let compressed = encoder.finish().unwrap();

        let decompressed = decompress_body("deflate", &compressed).unwrap().unwrap();

        assert_eq!(decompressed, payload);
    }

    #[test]
    fn unknown_encoding_returns_none_to_preserve_original_body_and_header() {
        let result = decompress_body("zstd", b"\x28\xb5\x2f\xfd").unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn decode_response_body_decodes_supported_encoding_and_strips_stale_entity_headers() {
        let payload = br#"{"ok":true}"#;
        let mut encoder =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(payload).unwrap();
        let compressed = encoder.finish().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("42"));
        headers.insert(TRANSFER_ENCODING, HeaderValue::from_static("chunked"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let decoded = decode_response_body(&mut headers, &compressed);

        assert_eq!(decoded.body, payload);
        assert_eq!(
            decoded.status,
            ResponseBodyDecodeStatus::Decoded {
                encoding: "gzip".to_string(),
            }
        );
        assert!(!headers.contains_key(CONTENT_ENCODING));
        assert!(!headers.contains_key(CONTENT_LENGTH));
        assert!(!headers.contains_key(TRANSFER_ENCODING));
        assert_eq!(
            headers.get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
    }

    #[test]
    fn decode_response_body_preserves_unknown_encoding_body_and_headers() {
        let raw = b"\x28\xb5\x2f\xfd";
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_ENCODING, HeaderValue::from_static("zstd"));
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("4"));

        let decoded = decode_response_body(&mut headers, raw);

        assert_eq!(decoded.body, raw);
        assert_eq!(
            decoded.status,
            ResponseBodyDecodeStatus::UnsupportedEncoding {
                encoding: "zstd".to_string(),
            }
        );
        assert_eq!(
            headers.get(CONTENT_ENCODING),
            Some(&HeaderValue::from_static("zstd"))
        );
        assert_eq!(
            headers.get(CONTENT_LENGTH),
            Some(&HeaderValue::from_static("4"))
        );
    }

    #[test]
    fn decode_response_body_preserves_failed_decode_body_and_headers() {
        let raw = b"not gzip";
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("8"));

        let decoded = decode_response_body(&mut headers, raw);

        assert_eq!(decoded.body, raw);
        match decoded.status {
            ResponseBodyDecodeStatus::DecodeFailed { encoding, error } => {
                assert_eq!(encoding, "gzip");
                assert!(!error.is_empty());
            }
            status => panic!("expected decode failure, got {status:?}"),
        }
        assert_eq!(
            headers.get(CONTENT_ENCODING),
            Some(&HeaderValue::from_static("gzip"))
        );
        assert_eq!(
            headers.get(CONTENT_LENGTH),
            Some(&HeaderValue::from_static("8"))
        );
    }

    #[test]
    fn decode_response_body_leaves_unencoded_body_unchanged() {
        let raw = br#"{"ok":true}"#;
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let decoded = decode_response_body(&mut headers, raw);

        assert_eq!(decoded.body, raw);
        assert_eq!(decoded.status, ResponseBodyDecodeStatus::NotEncoded);
        assert_eq!(
            headers.get(CONTENT_TYPE),
            Some(&HeaderValue::from_static("application/json"))
        );
    }
}
