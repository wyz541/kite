use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use atomicwrites::{AllowOverwrite, AtomicFile};
use sha2::{Digest, Sha256};

pub const MAX_TEXT_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextLineEnding {
    Lf,
    CrLf,
}

impl TextLineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "lf",
            Self::CrLf => "crlf",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "lf" => Some(Self::Lf),
            "crlf" => Some(Self::CrLf),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextDocument {
    pub path: PathBuf,
    pub text: String,
    pub language: String,
    pub line_ending: TextLineEnding,
    pub has_bom: bool,
    pub content_sha256: String,
    pub byte_len: u64,
}

impl TextDocument {
    pub fn open(path: &Path) -> Result<Self, String> {
        let metadata =
            fs::metadata(path).map_err(|error| format!("无法读取文本文件信息：{error}"))?;
        if !metadata.is_file() {
            return Err("目标不是文件".to_string());
        }
        if metadata.len() > MAX_TEXT_FILE_BYTES {
            return Err(format!(
                "文件超过 {} MB，文本编辑器不会打开。",
                MAX_TEXT_FILE_BYTES / 1024 / 1024
            ));
        }

        let bytes = fs::read(path).map_err(|error| format!("无法读取文本文件：{error}"))?;
        decode_text(path, &bytes, metadata.len())
    }

    pub fn save(&self, text: &str) -> Result<Self, String> {
        save_text_document(
            &self.path,
            text,
            self.line_ending,
            self.has_bom,
            &self.content_sha256,
        )
    }
}

pub fn save_text_document(
    path: &Path,
    text: &str,
    line_ending: TextLineEnding,
    has_bom: bool,
    expected_sha256: &str,
) -> Result<TextDocument, String> {
    if text.as_bytes().contains(&0) {
        return Err("文本包含空字节，Kite 已阻止保存。".to_string());
    }

    let original = fs::read(path).map_err(|error| format!("无法检查原文件状态：{error}"))?;
    let current_hash = hash_bytes(&original);
    if !expected_sha256.is_empty() && current_hash != expected_sha256 {
        return Err("文件已在 Kite 外被修改。请重新打开后再保存。".to_string());
    }

    let bytes = encode_text(text, line_ending, has_bom);
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| file.write_all(&bytes))
        .map_err(|error| format!("无法保存文本文件：{error}"))?;
    TextDocument::open(path)
}

pub fn is_text_path(path: &Path) -> bool {
    let language = language_for_path(path);
    if language != "text" {
        return true;
    }
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    extension.is_empty()
        || matches!(
            extension.as_str(),
            "txt"
                | "log"
                | "ini"
                | "conf"
                | "cfg"
                | "properties"
                | "env"
                | "csv"
                | "tsv"
                | "svg"
                | "xml"
                | "bat"
                | "cmd"
                | "ps1"
                | "psm1"
                | "psd1"
        )
        || matches!(
            name.as_str(),
            ".gitignore" | ".gitattributes" | ".gitmodules" | ".editorconfig"
        )
}

pub fn language_for_path(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match name.as_str() {
        "dockerfile" => return "dockerfile".to_string(),
        "makefile" | "gnumakefile" => return "make".to_string(),
        ".gitignore" | ".gitattributes" | ".gitmodules" => return "text".to_string(),
        _ => {}
    }

    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let language = match extension.as_str() {
        "rs" => "rust",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" => "typescript",
        "tsx" => "tsx",
        "py" | "pyw" => "python",
        "json" | "jsonc" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "md" | "markdown" | "mdx" => "markdown",
        "html" | "htm" => "html",
        "css" | "scss" | "less" => "css",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hxx" => "cpp",
        "cs" => "csharp",
        "java" => "java",
        "go" => "go",
        "kt" | "kts" => "kotlin",
        "sh" | "bash" | "zsh" => "bash",
        "sql" => "sql",
        "swift" => "swift",
        "lua" => "lua",
        "rb" => "ruby",
        "php" => "php",
        "proto" => "proto",
        "cmake" => "cmake",
        "diff" | "patch" => "diff",
        _ => "text",
    };
    language.to_string()
}

fn decode_text(path: &Path, bytes: &[u8], byte_len: u64) -> Result<TextDocument, String> {
    let (has_bom, content) = match bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        Some(content) => (true, content),
        None => (false, bytes),
    };
    if content.contains(&0) {
        return Err("此文件不是 UTF-8 文本文件。".to_string());
    }
    let text =
        std::str::from_utf8(content).map_err(|_| "此文件不是 UTF-8 文本文件。".to_string())?;
    let line_ending = detect_line_ending(text);
    Ok(TextDocument {
        path: path.to_path_buf(),
        text: normalize_line_endings(text),
        language: language_for_path(path),
        line_ending,
        has_bom,
        content_sha256: hash_bytes(bytes),
        byte_len,
    })
}

fn detect_line_ending(text: &str) -> TextLineEnding {
    let crlf = text.match_indices("\r\n").count();
    let lf = text.bytes().filter(|byte| *byte == b'\n').count();
    if crlf > 0 && crlf == lf {
        TextLineEnding::CrLf
    } else {
        TextLineEnding::Lf
    }
}

fn normalize_line_endings(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn encode_text(text: &str, line_ending: TextLineEnding, has_bom: bool) -> Vec<u8> {
    let normalized = normalize_line_endings(text);
    let serialized = match line_ending {
        TextLineEnding::Lf => normalized,
        TextLineEnding::CrLf => normalized.replace('\n', "\r\n"),
    };
    let mut bytes = Vec::with_capacity(serialized.len() + usize::from(has_bom) * 3);
    if has_bom {
        bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    bytes.extend_from_slice(serialized.as_bytes());
    bytes
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{MAX_TEXT_FILE_BYTES, TextLineEnding, language_for_path, save_text_document};

    fn temporary_path(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kite-text-editor-{label}-{}-{unique}.txt",
            std::process::id()
        ))
    }

    #[test]
    fn detects_utf8_bom_crlf_and_language() {
        let path = temporary_path("detect").with_extension("rs");
        fs::write(&path, b"\xEF\xBB\xBFfn main() {\r\n}\r\n").expect("write fixture");
        let document = super::TextDocument::open(&path).expect("open document");
        assert!(document.has_bom);
        assert_eq!(document.line_ending, TextLineEnding::CrLf);
        assert_eq!(document.text, "fn main() {\n}\n");
        assert_eq!(document.language, "rust");
        assert_eq!(language_for_path(&path), "rust");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn save_preserves_bom_and_crlf() {
        let path = temporary_path("save");
        fs::write(&path, b"\xEF\xBB\xBFfirst\r\nsecond\r\n").expect("write fixture");
        let document = super::TextDocument::open(&path).expect("open document");
        let saved = document
            .save("first\nsecond\nthird\n")
            .expect("save document");
        assert!(saved.has_bom);
        assert_eq!(saved.line_ending, TextLineEnding::CrLf);
        assert_eq!(
            fs::read(&path).expect("read saved file"),
            b"\xEF\xBB\xBFfirst\r\nsecond\r\nthird\r\n"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_an_external_change() {
        let path = temporary_path("conflict");
        fs::write(&path, "original").expect("write fixture");
        let document = super::TextDocument::open(&path).expect("open document");
        fs::write(&path, "changed outside Kite").expect("modify fixture");
        let error = save_text_document(
            &path,
            "Kite change",
            document.line_ending,
            document.has_bom,
            &document.content_sha256,
        )
        .expect_err("external change must fail");
        assert!(error.contains("Kite 外被修改"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_binary_and_oversized_files() {
        let binary = temporary_path("binary");
        fs::write(&binary, b"text\0binary").expect("write fixture");
        assert!(super::TextDocument::open(&binary).is_err());
        let _ = fs::remove_file(binary);

        let oversized = temporary_path("oversized");
        let file = fs::File::create(&oversized).expect("create fixture");
        file.set_len(MAX_TEXT_FILE_BYTES + 1)
            .expect("extend fixture");
        let error = super::TextDocument::open(&oversized).expect_err("oversized file must fail");
        assert!(error.contains("8 MB"));
        let _ = fs::remove_file(oversized);
    }
}
