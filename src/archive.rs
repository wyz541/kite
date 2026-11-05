use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

impl ArchiveFormat {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "zip" => Some(Self::Zip),
            "tar.gz" | "tgz" => Some(Self::TarGz),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
        }
    }
}

pub fn format_for_path(path: &Path) -> Option<ArchiveFormat> {
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    if name.ends_with(".zip") {
        Some(ArchiveFormat::Zip)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        Some(ArchiveFormat::TarGz)
    } else {
        None
    }
}

pub fn is_supported_archive(path: &Path) -> bool {
    format_for_path(path).is_some()
}

pub fn compress(source: &Path, format: ArchiveFormat) -> Result<PathBuf, String> {
    let metadata = fs::metadata(source).map_err(|error| format!("无法读取待压缩项目：{error}"))?;
    validate_compressible_source(source)?;
    let parent = source
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| "不能压缩根目录".to_string())?;
    let original_name = source
        .file_name()
        .ok_or_else(|| "无法确定待压缩项目名称".to_string())?;
    let source_name = if metadata.is_dir() {
        original_name
    } else {
        source.file_stem().unwrap_or(original_name)
    };
    let destination = unique_archive_path(parent, source_name, format);
    let temporary = temporary_archive_path(parent, source_name, format);

    let result = match format {
        ArchiveFormat::Zip => write_zip(source, &temporary),
        ArchiveFormat::TarGz => write_tar_gz(source, &temporary),
    };
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    if let Err(error) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("无法保存压缩文件：{error}"));
    }
    Ok(destination)
}

pub fn extraction_directory(source: &Path) -> Result<PathBuf, String> {
    source
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "无法确定解压目标文件夹".to_string())
}

pub fn extract(source: &Path, destination: &Path) -> Result<PathBuf, String> {
    let format = format_for_path(source)
        .ok_or_else(|| "不支持此压缩格式。当前支持 ZIP 和 TAR.GZ。".to_string())?;
    fs::create_dir_all(destination).map_err(|error| format!("无法创建解压目标文件夹：{error}"))?;

    match format {
        ArchiveFormat::Zip => extract_zip(source, destination)?,
        ArchiveFormat::TarGz => extract_tar_gz(source, destination)?,
    }
    Ok(destination.to_path_buf())
}

fn validate_compressible_source(source: &Path) -> Result<(), String> {
    for item in WalkDir::new(source).follow_links(false) {
        let entry = item.map_err(|error| format!("读取待压缩项目失败：{error}"))?;
        if entry.path_is_symlink() {
            return Err(format!("暂不支持压缩符号链接：{}", entry.path().display()));
        }
    }
    Ok(())
}

fn write_zip(source: &Path, destination: &Path) -> Result<(), String> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("无法创建 ZIP 文件：{error}"))?;
    let mut writer = ZipWriter::new(file);
    let parent = source
        .parent()
        .ok_or_else(|| "不能压缩根目录".to_string())?;
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o755);

    for item in WalkDir::new(source).follow_links(false) {
        let entry = item.map_err(|error| format!("读取待压缩项目失败：{error}"))?;
        if entry.path_is_symlink() {
            return Err(format!("暂不支持压缩符号链接：{}", entry.path().display()));
        }
        let relative = entry
            .path()
            .strip_prefix(parent)
            .map_err(|_| "无法生成压缩文件内部路径".to_string())?;
        let name = zip_entry_name(relative)?;
        if name.is_empty() {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|error| format!("读取待压缩项目属性失败：{error}"))?;
        if metadata.is_dir() {
            writer
                .add_directory_from_path(Path::new(&name), options)
                .map_err(|error| format!("写入 ZIP 文件夹失败：{error}"))?;
        } else if metadata.is_file() {
            writer
                .start_file_from_path(
                    Path::new(&name),
                    options.large_file(metadata.len() >= u32::MAX as u64),
                )
                .map_err(|error| format!("写入 ZIP 文件项失败：{error}"))?;
            let mut input =
                File::open(entry.path()).map_err(|error| format!("无法读取待压缩文件：{error}"))?;
            io::copy(&mut input, &mut writer)
                .map_err(|error| format!("写入 ZIP 文件内容失败：{error}"))?;
        } else {
            return Err(format!("暂不支持压缩特殊文件：{}", entry.path().display()));
        }
    }

    writer
        .finish()
        .map_err(|error| format!("完成 ZIP 文件失败：{error}"))?;
    Ok(())
}

fn write_tar_gz(source: &Path, destination: &Path) -> Result<(), String> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("无法创建 TAR.GZ 文件：{error}"))?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let parent = source
        .parent()
        .ok_or_else(|| "不能压缩根目录".to_string())?;
    let relative = source
        .strip_prefix(parent)
        .map_err(|_| "无法生成压缩文件内部路径".to_string())?;
    builder.follow_symlinks(false);
    builder
        .append_dir_all(relative, source)
        .map_err(|error| format!("写入 TAR.GZ 文件失败：{error}"))?;
    let encoder = builder
        .into_inner()
        .map_err(|error| format!("完成 TAR.GZ 文件失败：{error}"))?;
    encoder
        .finish()
        .map_err(|error| format!("完成 TAR.GZ 压缩失败：{error}"))?;
    Ok(())
}

fn extract_zip(source: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(source).map_err(|error| format!("无法打开 ZIP 文件：{error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("无法读取 ZIP 文件：{error}"))?;
    let mut directories = Vec::new();
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("无法读取 ZIP 文件项：{error}"))?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(format!("ZIP 文件包含不安全路径：{}", entry.name()));
        };
        validate_relative_path(&relative)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        if !seen.insert(relative.clone()) {
            return Err(format!("ZIP 文件包含重复路径：{}", relative.display()));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("暂不支持解压包含符号链接的 ZIP 文件".to_string());
        }
        let target = destination.join(&relative);
        if entry.is_dir() {
            directories.push(target);
        } else {
            ensure_available(&target)?;
            files.push((index, target));
        }
    }

    for directory in directories {
        fs::create_dir_all(&directory).map_err(|error| format!("创建 ZIP 文件夹失败：{error}"))?;
    }
    for (index, target) in files {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("创建 ZIP 文件夹失败：{error}"))?;
        }
        let mut input = archive
            .by_index(index)
            .map_err(|error| format!("无法读取 ZIP 文件项：{error}"))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(|error| format!("无法创建解压文件 {}：{error}", target.display()))?;
        io::copy(&mut input, &mut output).map_err(|error| format!("写入解压文件失败：{error}"))?;
    }
    Ok(())
}

fn extract_tar_gz(source: &Path, destination: &Path) -> Result<(), String> {
    validate_tar_gz(source, destination)?;
    let file = File::open(source).map_err(|error| format!("无法打开 TAR.GZ 文件：{error}"))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for item in archive
        .entries()
        .map_err(|error| format!("无法读取 TAR.GZ 文件：{error}"))?
    {
        let mut entry = item.map_err(|error| format!("无法读取 TAR.GZ 文件项：{error}"))?;
        let unpacked = entry
            .unpack_in(destination)
            .map_err(|error| format!("解压 TAR.GZ 文件项失败：{error}"))?;
        if !unpacked {
            return Err("TAR.GZ 文件包含不安全路径".to_string());
        }
    }
    Ok(())
}

fn validate_tar_gz(source: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(source).map_err(|error| format!("无法打开 TAR.GZ 文件：{error}"))?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut seen = BTreeSet::new();

    for item in archive
        .entries()
        .map_err(|error| format!("无法读取 TAR.GZ 文件：{error}"))?
    {
        let entry = item.map_err(|error| format!("无法读取 TAR.GZ 文件项：{error}"))?;
        let relative = entry
            .path()
            .map_err(|error| format!("TAR.GZ 文件路径无效：{error}"))?
            .to_path_buf();
        validate_relative_path(&relative)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        if !seen.insert(relative.clone()) {
            return Err(format!("TAR.GZ 文件包含重复路径：{}", relative.display()));
        }
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err("暂不支持解压包含符号链接的 TAR.GZ 文件".to_string());
        }
        if !entry_type.is_dir() && !entry_type.is_file() {
            return Err(format!(
                "TAR.GZ 文件包含不支持的项目：{}",
                relative.display()
            ));
        }
        if entry_type.is_file() {
            ensure_available(&destination.join(relative))?;
        }
    }
    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.is_absolute() {
        return Err(format!("压缩文件包含绝对路径：{}", path.display()));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("压缩文件包含不安全路径：{}", path.display()));
            }
        }
    }
    Ok(())
}

fn zip_entry_name(path: &Path) -> Result<String, String> {
    validate_relative_path(path)?;
    let parts = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>();
    Ok(parts.join("/"))
}

fn ensure_available(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok() {
        Err(format!("目标文件已经存在：{}", path.display()))
    } else {
        Ok(())
    }
}

fn unique_archive_path(
    parent: &Path,
    source_name: &std::ffi::OsStr,
    format: ArchiveFormat,
) -> PathBuf {
    let base = source_name.to_string_lossy();
    for index in 0.. {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!(" ({index})")
        };
        let candidate = parent.join(format!("{base}{suffix}.{}", format.extension()));
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
    unreachable!("archive path counter is unbounded")
}

fn temporary_archive_path(
    parent: &Path,
    source_name: &std::ffi::OsStr,
    format: ArchiveFormat,
) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    parent.join(format!(
        ".{}.kite-{}-{nonce}.{}",
        source_name.to_string_lossy(),
        std::process::id(),
        format.extension()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kite-archive-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn fixture(label: &str) -> (PathBuf, PathBuf) {
        let root = temporary_root(label);
        let source = root.join("sample");
        fs::create_dir_all(source.join("nested")).expect("create fixture");
        fs::write(source.join("hello.txt"), b"hello").expect("write fixture");
        fs::write(source.join("nested").join("data.txt"), b"data").expect("write nested fixture");
        (root, source)
    }

    #[test]
    fn detects_supported_archive_names() {
        assert_eq!(
            format_for_path(Path::new("report.ZIP")),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(
            format_for_path(Path::new("report.tar.gz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            format_for_path(Path::new("report.tgz")),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(format_for_path(Path::new("report.7z")), None);
    }

    #[test]
    fn file_archive_name_uses_the_file_stem() {
        let (root, source) = fixture("name");
        let file = source.join("hello.txt");
        let archive = compress(&file, ArchiveFormat::Zip).expect("compress");
        assert_eq!(
            archive.file_name().and_then(|name| name.to_str()),
            Some("hello.zip")
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn zip_round_trip_keeps_folder_structure() {
        let (root, source) = fixture("zip");
        let archive = compress(&source, ArchiveFormat::Zip).expect("compress");
        let destination = root.join("extracted");
        extract(&archive, &destination).expect("extract");

        assert_eq!(
            fs::read(destination.join("sample").join("hello.txt")).expect("read"),
            b"hello"
        );
        assert_eq!(
            fs::read(destination.join("sample").join("nested").join("data.txt"))
                .expect("read nested"),
            b"data"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn tar_gz_round_trip_keeps_folder_structure() {
        let (root, source) = fixture("tar");
        let archive = compress(&source, ArchiveFormat::TarGz).expect("compress");
        let destination = root.join("extracted");
        extract(&archive, &destination).expect("extract");

        assert_eq!(
            fs::read(destination.join("sample").join("hello.txt")).expect("read"),
            b"hello"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn compression_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;

        let (root, source) = fixture("symlink");
        symlink(source.join("hello.txt"), source.join("link.txt")).expect("create symlink");

        let zip_error = compress(&source, ArchiveFormat::Zip).expect_err("reject zip symlink");
        let tar_error = compress(&source, ArchiveFormat::TarGz).expect_err("reject tar symlink");
        assert!(zip_error.contains("符号链接"));
        assert!(tar_error.contains("符号链接"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn extraction_does_not_overwrite_existing_files() {
        let (root, source) = fixture("existing");
        let archive = compress(&source, ArchiveFormat::Zip).expect("compress");
        let destination = root.join("extracted");
        fs::create_dir_all(destination.join("sample")).expect("create destination");
        fs::write(destination.join("sample").join("hello.txt"), b"old").expect("write old file");

        let error = extract(&archive, &destination).expect_err("must reject overwrite");
        assert!(error.contains("已经存在"));
        assert_eq!(
            fs::read(destination.join("sample").join("hello.txt")).expect("read old"),
            b"old"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
