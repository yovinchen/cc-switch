use http::HeaderMap;
use std::io::Read;

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
    use super::{decompress_body, get_content_encoding};
    use http::{HeaderMap, HeaderValue};
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
}
