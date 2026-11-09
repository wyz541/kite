use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum SearchKindFilter {
    #[default]
    All,
    Folders,
    Documents,
    SourceCode,
    Markdown,
    Images,
    Media,
}

impl SearchKindFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "全部项目",
            Self::Folders => "文件夹",
            Self::Documents => "文档",
            Self::SourceCode => "源代码",
            Self::Markdown => "Markdown",
            Self::Images => "图片",
            Self::Media => "媒体",
        }
    }

    pub fn compact_label(self) -> &'static str {
        match self {
            Self::All => "类型",
            Self::Folders => "文件夹",
            Self::Documents => "文档",
            Self::SourceCode => "代码",
            Self::Markdown => "MD",
            Self::Images => "图片",
            Self::Media => "媒体",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Enum)]
pub enum SearchTimeFilter {
    #[default]
    Any,
    Last24Hours,
    Last7Days,
    Last30Days,
}

impl SearchTimeFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::Any => "任意时间",
            Self::Last24Hours => "最近 24 小时",
            Self::Last7Days => "最近 7 天",
            Self::Last30Days => "最近 30 天",
        }
    }

    pub fn compact_label(self) -> &'static str {
        match self {
            Self::Any => "时间",
            Self::Last24Hours => "24小时",
            Self::Last7Days => "7天",
            Self::Last30Days => "30天",
        }
    }

    pub fn duration(self) -> Option<Duration> {
        match self {
            Self::Any => None,
            Self::Last24Hours => Some(Duration::from_secs(24 * 60 * 60)),
            Self::Last7Days => Some(Duration::from_secs(7 * 24 * 60 * 60)),
            Self::Last30Days => Some(Duration::from_secs(30 * 24 * 60 * 60)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct SearchFilters {
    pub kind: SearchKindFilter,
    pub modified: SearchTimeFilter,
}

impl SearchFilters {
    pub fn is_default(self) -> bool {
        self.kind == SearchKindFilter::All && self.modified == SearchTimeFilter::Any
    }

    pub fn summary(self) -> Option<String> {
        if self.is_default() {
            return None;
        }
        Some(format!("{} · {}", self.kind.label(), self.modified.label()))
    }

    pub fn matches(self, path: &Path, is_dir: bool, modified: Option<SystemTime>) -> bool {
        self.matches_at(path, is_dir, modified, SystemTime::now())
    }

    pub fn matches_unix_ms(self, path: &Path, is_dir: bool, modified_unix_ms: Option<i64>) -> bool {
        let modified = modified_unix_ms.and_then(|value| {
            u64::try_from(value)
                .ok()
                .and_then(|value| UNIX_EPOCH.checked_add(Duration::from_millis(value)))
        });
        self.matches(path, is_dir, modified)
    }

    pub fn modified_after_unix_ms(self) -> Option<i64> {
        let duration = self.modified.duration()?;
        let threshold = SystemTime::now().checked_sub(duration)?;
        threshold
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|value| value.as_millis() as i64)
    }

    pub fn kind_extensions(self) -> Option<&'static [&'static str]> {
        match self.kind {
            SearchKindFilter::Documents => Some(DOCUMENT_EXTENSIONS),
            SearchKindFilter::SourceCode => Some(SOURCE_EXTENSIONS),
            SearchKindFilter::Markdown => Some(MARKDOWN_EXTENSIONS),
            SearchKindFilter::Images => Some(IMAGE_EXTENSIONS),
            SearchKindFilter::Media => Some(MEDIA_EXTENSIONS),
            SearchKindFilter::All | SearchKindFilter::Folders => None,
        }
    }

    fn matches_at(
        self,
        path: &Path,
        is_dir: bool,
        modified: Option<SystemTime>,
        now: SystemTime,
    ) -> bool {
        let kind_matches = match self.kind {
            SearchKindFilter::All => true,
            SearchKindFilter::Folders => is_dir,
            _ if is_dir => false,
            _ => self
                .kind_extensions()
                .is_some_and(|extensions| extension_matches(path, extensions)),
        };
        if !kind_matches {
            return false;
        }
        let Some(duration) = self.modified.duration() else {
            return true;
        };
        modified.is_some_and(|modified| {
            now.duration_since(modified)
                .map(|age| age <= duration)
                .unwrap_or(true)
        })
    }
}

fn extension_matches(path: &Path, extensions: &[&str]) -> bool {
    path.extension().is_some_and(|extension| {
        let extension = extension.to_string_lossy();
        extensions
            .iter()
            .any(|candidate| extension.eq_ignore_ascii_case(candidate))
    })
}

pub const DOCUMENT_EXTENSIONS: &[&str] = &[
    "txt", "rtf", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "csv",
    "epub",
];
pub const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "c", "h", "cc", "cpp", "cxx", "hpp", "cs", "java", "kt", "kts", "go", "py", "rb", "php",
    "js", "jsx", "ts", "tsx", "vue", "svelte", "swift", "m", "mm", "scala", "sh", "bash", "zsh",
    "fish", "ps1", "sql", "html", "htm", "css", "scss", "sass", "less", "xml", "yaml", "yml",
    "toml", "ini", "json", "proto", "gradle",
];
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdx"];
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "tif", "tiff", "heic", "heif", "avif",
];
pub const MEDIA_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "flac", "aac", "ogg", "m4a", "opus", "wma", "mp4", "mkv", "mov", "avi", "webm",
    "m4v", "wmv", "flv",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn filters_kind_and_time_before_use() {
        let filters = SearchFilters {
            kind: SearchKindFilter::SourceCode,
            modified: SearchTimeFilter::Last24Hours,
        };
        let now = SystemTime::now();
        assert!(filters.matches_at(
            &PathBuf::from("src/main.rs"),
            false,
            Some(now - Duration::from_secs(60)),
            now
        ));
        assert!(!filters.matches_at(
            &PathBuf::from("notes.md"),
            false,
            Some(now - Duration::from_secs(60)),
            now
        ));
        assert!(!filters.matches_at(
            &PathBuf::from("src/main.rs"),
            false,
            Some(now - Duration::from_secs(48 * 60 * 60)),
            now
        ));
    }

    #[test]
    fn folders_match_only_folder_filter() {
        let now = SystemTime::now();
        assert!(
            SearchFilters {
                kind: SearchKindFilter::Folders,
                modified: SearchTimeFilter::Any,
            }
            .matches_at(Path::new("folder"), true, None, now)
        );
        assert!(SearchFilters::default().matches_unix_ms(Path::new("ignored"), false, Some(0)));
        assert!(
            !SearchFilters {
                kind: SearchKindFilter::Documents,
                modified: SearchTimeFilter::Any,
            }
            .matches_at(Path::new("folder"), true, None, now)
        );
    }

    #[test]
    fn extension_classes_cover_expected_categories() {
        assert!(
            SearchFilters {
                kind: SearchKindFilter::Documents,
                modified: SearchTimeFilter::Any,
            }
            .matches(Path::new("report.pdf"), false, None)
        );
        assert!(
            SearchFilters {
                kind: SearchKindFilter::Markdown,
                modified: SearchTimeFilter::Any,
            }
            .matches(Path::new("README.MD"), false, None)
        );
        assert!(
            SearchFilters {
                kind: SearchKindFilter::Images,
                modified: SearchTimeFilter::Any,
            }
            .matches(Path::new("photo.webp"), false, None)
        );
        assert!(
            SearchFilters {
                kind: SearchKindFilter::Media,
                modified: SearchTimeFilter::Any,
            }
            .matches(Path::new("clip.mp4"), false, None)
        );
    }
}
