use std::collections::HashMap;

#[derive(Clone)]
pub struct ContentDetector {
    mime_to_ext: HashMap<String, String>,
}

impl ContentDetector {
    pub fn new() -> Self {
        let mut mime_to_ext = HashMap::new();

        mime_to_ext.insert("image/png".to_string(), "png".to_string());
        mime_to_ext.insert("image/jpeg".to_string(), "jpg".to_string());
        mime_to_ext.insert("image/gif".to_string(), "gif".to_string());
        mime_to_ext.insert("image/webp".to_string(), "webp".to_string());
        mime_to_ext.insert("image/svg+xml".to_string(), "svg".to_string());
        mime_to_ext.insert("image/bmp".to_string(), "bmp".to_string());
        mime_to_ext.insert("image/x-icon".to_string(), "ico".to_string());
        mime_to_ext.insert("image/tiff".to_string(), "tiff".to_string());
        mime_to_ext.insert("image/avif".to_string(), "avif".to_string());
        mime_to_ext.insert("image/apng".to_string(), "apng".to_string());
        mime_to_ext.insert("image/heic".to_string(), "heic".to_string());
        mime_to_ext.insert("image/heif".to_string(), "heif".to_string());
        mime_to_ext.insert("image/jxl".to_string(), "jxl".to_string());

        mime_to_ext.insert("video/mp4".to_string(), "mp4".to_string());
        mime_to_ext.insert("video/webm".to_string(), "webm".to_string());
        mime_to_ext.insert("video/ogg".to_string(), "ogv".to_string());
        mime_to_ext.insert("video/x-matroska".to_string(), "mkv".to_string());
        mime_to_ext.insert("video/quicktime".to_string(), "mov".to_string());
        mime_to_ext.insert("video/x-msvideo".to_string(), "avi".to_string());
        mime_to_ext.insert("video/x-flv".to_string(), "flv".to_string());
        mime_to_ext.insert("video/3gpp".to_string(), "3gp".to_string());
        mime_to_ext.insert("video/mp2t".to_string(), "ts".to_string());
        mime_to_ext.insert("video/mpeg".to_string(), "mpeg".to_string());

        mime_to_ext.insert("audio/mpeg".to_string(), "mp3".to_string());
        mime_to_ext.insert("audio/ogg".to_string(), "ogg".to_string());
        mime_to_ext.insert("audio/wav".to_string(), "wav".to_string());
        mime_to_ext.insert("audio/webm".to_string(), "weba".to_string());
        mime_to_ext.insert("audio/flac".to_string(), "flac".to_string());
        mime_to_ext.insert("audio/aac".to_string(), "aac".to_string());
        mime_to_ext.insert("audio/opus".to_string(), "opus".to_string());
        mime_to_ext.insert("audio/midi".to_string(), "midi".to_string());
        mime_to_ext.insert("audio/x-m4a".to_string(), "m4a".to_string());

        mime_to_ext.insert("application/pdf".to_string(), "pdf".to_string());
        mime_to_ext.insert("application/zip".to_string(), "zip".to_string());
        mime_to_ext.insert("application/x-7z-compressed".to_string(), "7z".to_string());
        mime_to_ext.insert(
            "application/x-rar-compressed".to_string(),
            "rar".to_string(),
        );
        mime_to_ext.insert("application/gzip".to_string(), "gz".to_string());
        mime_to_ext.insert("application/x-bzip2".to_string(), "bz2".to_string());
        mime_to_ext.insert("application/x-xz".to_string(), "xz".to_string());
        mime_to_ext.insert("application/x-tar".to_string(), "tar".to_string());
        mime_to_ext.insert(
            "application/vnd.microsoft.portable-executable".to_string(),
            "exe".to_string(),
        );
        mime_to_ext.insert("application/x-msdownload".to_string(), "exe".to_string());
        mime_to_ext.insert("application/x-msi".to_string(), "msi".to_string());
        mime_to_ext.insert("application/x-deb".to_string(), "deb".to_string());
        mime_to_ext.insert("application/x-rpm".to_string(), "rpm".to_string());
        mime_to_ext.insert(
            "application/x-apple-diskimage".to_string(),
            "dmg".to_string(),
        );
        mime_to_ext.insert(
            "application/vnd.appimage".to_string(),
            "appimage".to_string(),
        );

        mime_to_ext.insert("application/json".to_string(), "json".to_string());
        mime_to_ext.insert("application/xml".to_string(), "xml".to_string());
        mime_to_ext.insert("application/yaml".to_string(), "yaml".to_string());
        mime_to_ext.insert("application/toml".to_string(), "toml".to_string());
        mime_to_ext.insert("application/wasm".to_string(), "wasm".to_string());

        mime_to_ext.insert(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string(),
            "docx".to_string(),
        );
        mime_to_ext.insert(
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
            "xlsx".to_string(),
        );
        mime_to_ext.insert(
            "application/vnd.openxmlformats-officedocument.presentationml.presentation".to_string(),
            "pptx".to_string(),
        );
        mime_to_ext.insert("application/msword".to_string(), "doc".to_string());
        mime_to_ext.insert("application/vnd.ms-excel".to_string(), "xls".to_string());
        mime_to_ext.insert(
            "application/vnd.ms-powerpoint".to_string(),
            "ppt".to_string(),
        );
        mime_to_ext.insert(
            "application/vnd.oasis.opendocument.text".to_string(),
            "odt".to_string(),
        );
        mime_to_ext.insert(
            "application/vnd.oasis.opendocument.spreadsheet".to_string(),
            "ods".to_string(),
        );
        mime_to_ext.insert(
            "application/vnd.oasis.opendocument.presentation".to_string(),
            "odp".to_string(),
        );

        mime_to_ext.insert("application/rtf".to_string(), "rtf".to_string());
        mime_to_ext.insert(
            "application/x-shockwave-flash".to_string(),
            "swf".to_string(),
        );
        mime_to_ext.insert("application/java-archive".to_string(), "jar".to_string());
        mime_to_ext.insert("application/x-iso9660-image".to_string(), "iso".to_string());

        mime_to_ext.insert("text/plain".to_string(), "txt".to_string());
        mime_to_ext.insert("text/html".to_string(), "html".to_string());
        mime_to_ext.insert("text/css".to_string(), "css".to_string());
        mime_to_ext.insert("text/javascript".to_string(), "js".to_string());
        mime_to_ext.insert("text/csv".to_string(), "csv".to_string());
        mime_to_ext.insert("text/markdown".to_string(), "md".to_string());
        mime_to_ext.insert("text/xml".to_string(), "xml".to_string());
        mime_to_ext.insert("text/x-python".to_string(), "py".to_string());
        mime_to_ext.insert("text/x-java".to_string(), "java".to_string());
        mime_to_ext.insert("text/x-c".to_string(), "c".to_string());
        mime_to_ext.insert("text/x-c++".to_string(), "cpp".to_string());
        mime_to_ext.insert("text/x-rust".to_string(), "rs".to_string());
        mime_to_ext.insert("text/x-go".to_string(), "go".to_string());
        mime_to_ext.insert("text/x-php".to_string(), "php".to_string());
        mime_to_ext.insert("text/x-ruby".to_string(), "rb".to_string());
        mime_to_ext.insert("text/x-sh".to_string(), "sh".to_string());

        mime_to_ext.insert("font/ttf".to_string(), "ttf".to_string());
        mime_to_ext.insert("font/otf".to_string(), "otf".to_string());
        mime_to_ext.insert("font/woff".to_string(), "woff".to_string());
        mime_to_ext.insert("font/woff2".to_string(), "woff2".to_string());

        Self { mime_to_ext }
    }

    pub fn detect(&self, data: &[u8]) -> String {
        if data.is_empty() {
            return "application/octet-stream".to_string();
        }

        let len = data.len();

        match () {
            _ if len >= 8 && &data[0..8] == b"\x89PNG\r\n\x1a\n" => "image/png".to_string(),
            _ if len >= 3 && &data[0..3] == b"\xFF\xD8\xFF" => "image/jpeg".to_string(),
            _ if len >= 6 && (&data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a") => {
                "image/gif".to_string()
            }
            _ if len >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" => {
                "image/webp".to_string()
            }
            _ if len >= 2 && &data[0..2] == b"BM" => "image/bmp".to_string(),
            _ if len >= 4 && &data[0..4] == b"\x00\x00\x01\x00" => "image/x-icon".to_string(),
            _ if len >= 12 && (&data[0..4] == b"II\x2A\x00" || &data[0..4] == b"MM\x00\x2A") => {
                "image/tiff".to_string()
            }
            _ if len >= 12 && &data[4..12] == b"ftypavif" => "image/avif".to_string(),
            _ if len >= 12 && &data[4..12] == b"ftypheic" => "image/heic".to_string(),
            _ if len >= 12 && &data[4..12] == b"ftypheif" => "image/heif".to_string(),
            _ if len >= 12
                && &data[0..12] == b"\xFF\x0A\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00" =>
            {
                "image/jxl".to_string()
            }
            _ if len >= 5 && &data[0..5] == b"<?xml" => {
                if data.windows(4).any(|w| w == b"<svg") {
                    "image/svg+xml".to_string()
                } else {
                    "application/xml".to_string()
                }
            }
            _ if len >= 4 && &data[0..4] == b"<svg" => "image/svg+xml".to_string(),
            _ if len >= 12 && &data[0..4] == b"\x00\x00\x00\x20" => match &data[4..12] {
                b"ftypmp42" | b"ftypisom" | b"ftypM4V " | b"ftypqt  " => "video/mp4".to_string(),
                b"ftyp" if len >= 12 && &data[8..11] == b"M4A" => "audio/x-m4a".to_string(),
                _ => "application/octet-stream".to_string(),
            },
            _ if len >= 12 && &data[0..4] == b"\x00\x00\x00\x18" && &data[4..12] == b"ftypqt  " => {
                "video/quicktime".to_string()
            }
            _ if len >= 4 && &data[0..4] == b"\x1a\x45\xDF\xA3" => "video/x-matroska".to_string(),
            _ if len >= 3 && &data[0..3] == b"FLV" => "video/x-flv".to_string(),
            _ if len >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"AVI " => {
                "video/x-msvideo".to_string()
            }
            _ if len >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WAVE" => {
                "audio/wav".to_string()
            }
            _ if len >= 4 && &data[0..4] == b"\x00\x00\x01\xBA" => "video/mpeg".to_string(),
            _ if len >= 4 && &data[0..4] == b"OggS" => {
                if len > 35 && &data[28..35] == b"\x01vorbis" {
                    "audio/ogg".to_string()
                } else if len > 35 && &data[28..35] == b"OpusHea" {
                    "audio/opus".to_string()
                } else {
                    "video/ogg".to_string()
                }
            }
            _ if len >= 3 && &data[0..3] == b"ID3" => "audio/mpeg".to_string(),
            _ if len >= 2 && &data[0..2] == b"\xFF\xFB" => "audio/mpeg".to_string(),
            _ if len >= 2 && &data[0..2] == b"\xFF\xF1" => "audio/aac".to_string(),
            _ if len >= 4 && &data[0..4] == b"fLaC" => "audio/flac".to_string(),
            _ if len >= 4 && &data[0..4] == b"MThd" => "audio/midi".to_string(),
            _ if len >= 5 && &data[0..5] == b"%PDF-" => "application/pdf".to_string(),
            _ if len >= 4 && &data[0..4] == b"PK\x03\x04" => {
                if len > 50 {
                    if data.windows(19).any(|w| w == b"word/document.xml") {
                        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                            .to_string()
                    } else if data.windows(17).any(|w| w == b"xl/workbook.xml") {
                        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                            .to_string()
                    } else if data.windows(22).any(|w| w == b"ppt/presentation.xml") {
                        "application/vnd.openxmlformats-officedocument.presentationml.presentation"
                            .to_string()
                    } else {
                        "application/zip".to_string()
                    }
                } else {
                    "application/zip".to_string()
                }
            }
            _ if len >= 6 && &data[0..6] == b"7z\xBC\xAF\x27\x1C" => {
                "application/x-7z-compressed".to_string()
            }
            _ if len >= 7 && &data[0..7] == b"Rar!\x1A\x07\x00" => {
                "application/x-rar-compressed".to_string()
            }
            _ if len >= 2 && &data[0..2] == b"\x1f\x8b" => "application/gzip".to_string(),
            _ if len >= 3 && &data[0..3] == b"BZh" => "application/x-bzip2".to_string(),
            _ if len >= 6 && &data[0..6] == b"\xFD7zXZ\x00" => "application/x-xz".to_string(),
            _ if len >= 262 && &data[257..262] == b"ustar" => "application/x-tar".to_string(),
            _ if len >= 2 && &data[0..2] == b"MZ" => "application/x-msdownload".to_string(),
            _ if len >= 8 && &data[0..8] == b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1" => {
                if len > 2080 {
                    if data[2080..].windows(4).any(|w| w == b"Word") {
                        "application/msword".to_string()
                    } else if data[2080..].windows(5).any(|w| w == b"Excel") {
                        "application/vnd.ms-excel".to_string()
                    } else if data[2080..].windows(10).any(|w| w == b"PowerPoint") {
                        "application/vnd.ms-powerpoint".to_string()
                    } else {
                        "application/x-msi".to_string()
                    }
                } else {
                    "application/x-msi".to_string()
                }
            }
            _ if len >= 4 && &data[0..4] == b"!<ar" => {
                if len > 20 && &data[8..20] == b"debian-bina" {
                    "application/x-deb".to_string()
                } else {
                    "application/octet-stream".to_string()
                }
            }
            _ if len >= 2 && &data[0..2] == b"\xED\xAB" => "application/x-rpm".to_string(),
            _ if len >= 4 && &data[0..4] == b"x\x01\x73\x0D" => {
                "application/x-apple-diskimage".to_string()
            }
            _ if len >= 4 && &data[0..4] == b"\x7FELF" => {
                if len > 16 && data[16] == 2 {
                    "application/vnd.appimage".to_string()
                } else {
                    "application/octet-stream".to_string()
                }
            }
            _ if len >= 4 && &data[0..4] == b"wOFF" => "font/woff".to_string(),
            _ if len >= 4 && &data[0..4] == b"wOF2" => "font/woff2".to_string(),
            _ if len >= 5 && &data[0..5] == b"\x00\x01\x00\x00\x00" => "font/ttf".to_string(),
            _ if len >= 4 && &data[0..4] == b"OTTO" => "font/otf".to_string(),
            _ if len >= 4 && &data[0..4] == b"CWS\x08" => {
                "application/x-shockwave-flash".to_string()
            }
            _ if len >= 8 && &data[0..8] == b"\0asm\x01\0\0\0" => "application/wasm".to_string(),
            _ if len >= 5 && &data[0..5] == b"<!DOC" => "text/html".to_string(),
            _ if len >= 5 && &data[0..5] == b"<html" => "text/html".to_string(),
            _ if len >= 6 && &data[0..6] == b"<HTML" => "text/html".to_string(),
            _ if is_likely_text(data) => {
                if !data.is_empty() && data[0] == b'{' {
                    "application/json".to_string()
                } else if len >= 3 && &data[0..3] == b"---" {
                    "application/yaml".to_string()
                } else if data
                    .windows(9)
                    .any(|w| w == b"#!/bin/sh" || w == b"#!/bin/ba")
                {
                    "text/x-sh".to_string()
                } else if data.windows(15).any(|w| w == b"#!/usr/bin/pyth") {
                    "text/x-python".to_string()
                } else {
                    "text/plain".to_string()
                }
            }
            _ => "application/octet-stream".to_string(),
        }
    }

    pub fn get_extension(&self, mime_type: &str) -> String {
        self.mime_to_ext
            .get(mime_type)
            .cloned()
            .unwrap_or_else(|| "bin".to_string())
    }
}

fn is_likely_text(data: &[u8]) -> bool {
    let sample_size = std::cmp::min(512, data.len());
    let sample = &data[..sample_size];

    let mut text_chars = 0;
    for &byte in sample {
        if byte == b'\t' || byte == b'\n' || byte == b'\r' || (32..127).contains(&byte) {
            text_chars += 1;
        }
    }

    text_chars > (sample_size * 85) / 100
}
