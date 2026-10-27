use std::fs::{self, File};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const RELEASE_API_URL: &str = "https://api.github.com/repos/wyz541/kite/releases/latest";
const LATEST_RELEASE_PAGE_URL: &str = "https://github.com/wyz541/kite/releases/latest";
const RELEASE_DOWNLOAD_PREFIX: &str = "https://github.com/wyz541/kite/releases/download/";
const MAX_RELEASE_JSON_BYTES: u64 = 1024 * 1024;
const MAX_UPDATE_BYTES: u64 = 256 * 1024 * 1024;
const USER_AGENT: &str = concat!(
    "Kite/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/wyz541/kite)"
);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_name: String,
    pub release_url: String,
    pub release_notes: String,
    pub published_at: Option<String>,
    pub asset_name: String,
    pub asset_url: String,
    pub asset_sha256: String,
    pub asset_size: u64,
}

impl UpdateInfo {
    pub fn download(&self) -> Result<PathBuf, String> {
        download_update(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateTarget {
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    WindowsX64,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    MacOSArm64,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    draft: bool,
    prerelease: bool,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn check_for_update() -> Result<UpdateInfo, String> {
    let current = Version::parse(current_version())
        .map_err(|error| format!("无法读取当前 Kite 版本：{error}"))?;
    let target = current_target()?;
    let release = fetch_latest_release()?;
    resolve_release(release, &current, target)
}

fn fetch_latest_release() -> Result<GithubRelease, String> {
    match fetch_api_release() {
        Ok(release) => Ok(release),
        Err(api_error) => fetch_redirected_release().map_err(|fallback_error| {
            format!("无法读取 GitHub Release。API：{api_error}。回退：{fallback_error}")
        }),
    }
}

fn fetch_api_release() -> Result<GithubRelease, String> {
    let bytes = http_get_bytes(
        RELEASE_API_URL,
        "application/vnd.github+json",
        MAX_RELEASE_JSON_BYTES,
        "GitHub Release API",
    )?;
    serde_json::from_slice(&bytes).map_err(|error| format!("无法解析 GitHub Release：{error}"))
}

fn fetch_redirected_release() -> Result<GithubRelease, String> {
    let response = agent()
        .get(LATEST_RELEASE_PAGE_URL)
        .set("Accept", "text/html")
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("无法读取 GitHub 最新 Release 页面：{error}"))?;
    let final_url = response.get_url().to_string();
    drop(response);
    let tag = final_url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| value.starts_with('v') || value.starts_with('V'))
        .ok_or_else(|| "无法从 GitHub 跳转地址读取 Release 版本".to_string())?;
    parse_release_version(tag)?;

    let checksum_url = format!("{RELEASE_DOWNLOAD_PREFIX}{tag}/SHA256SUMS.txt");
    validate_asset_url(&checksum_url)?;
    let bytes = http_get_bytes(
        &checksum_url,
        "text/plain",
        MAX_RELEASE_JSON_BYTES,
        "GitHub Release 校验文件",
    )?;
    let checksums = String::from_utf8(bytes)
        .map_err(|error| format!("GitHub Release 校验文件不是 UTF-8：{error}"))?;
    let mut assets = Vec::new();
    for name in [
        "Kite.exe".to_string(),
        format!("Kite-{tag}-macOS-arm64.zip"),
    ] {
        if let Some(hash) = checksum_for(&checksums, &name) {
            assets.push(GithubAsset {
                name: name.clone(),
                size: 0,
                digest: Some(format!("sha256:{hash}")),
                browser_download_url: format!("{RELEASE_DOWNLOAD_PREFIX}{tag}/{name}"),
            });
        }
    }
    if assets.is_empty() {
        return Err("GitHub Release 校验文件没有支持平台的更新包".to_string());
    }
    Ok(GithubRelease {
        tag_name: tag.to_string(),
        name: Some(format!("Kite {tag}")),
        html_url: format!("https://github.com/wyz541/kite/releases/tag/{tag}"),
        body: Some("GitHub Release 未提供可读取的更新说明。".to_string()),
        published_at: None,
        draft: false,
        prerelease: false,
        assets,
    })
}

fn http_get_bytes(url: &str, accept: &str, limit: u64, context: &str) -> Result<Vec<u8>, String> {
    let response = agent()
        .get(url)
        .set("Accept", accept)
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("无法连接{context}：{error}"))?;
    let mut reader = response.into_reader().take(limit + 1);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取{context}：{error}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{context}超过大小限制"));
    }
    Ok(bytes)
}

fn checksum_for(checksums: &str, asset_name: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == asset_name).then(|| hash.to_string())
    })
}

fn resolve_release(
    release: GithubRelease,
    current: &Version,
    target: UpdateTarget,
) -> Result<UpdateInfo, String> {
    if release.draft || release.prerelease {
        return Err("GitHub 最新 Release 不是稳定版本".to_string());
    }
    let latest = parse_release_version(&release.tag_name)?;
    let update_available = latest > *current;
    let mut info = UpdateInfo {
        current_version: current.to_string(),
        latest_version: latest.to_string(),
        update_available,
        release_name: release
            .name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| release.tag_name.clone()),
        release_url: release.html_url,
        release_notes: release.body.unwrap_or_default(),
        published_at: release.published_at,
        asset_name: String::new(),
        asset_url: String::new(),
        asset_sha256: String::new(),
        asset_size: 0,
    };
    if !update_available {
        return Ok(info);
    }

    let asset = release
        .assets
        .iter()
        .find(|asset| asset_matches_target(&asset.name, target))
        .ok_or_else(|| format!("Release {} 不包含当前平台的更新包", latest))?;
    validate_asset_name(&asset.name)?;
    validate_asset_url(&asset.browser_download_url)?;
    if asset.size > MAX_UPDATE_BYTES {
        return Err("更新包大小无效".to_string());
    }
    let digest = asset
        .digest
        .as_deref()
        .ok_or_else(|| "更新包缺少 SHA-256 摘要".to_string())?;
    let digest = parse_sha256(digest)?;

    info.asset_name = asset.name.clone();
    info.asset_url = asset.browser_download_url.clone();
    info.asset_sha256 = digest;
    info.asset_size = asset.size;
    Ok(info)
}

fn parse_release_version(tag: &str) -> Result<Version, String> {
    let trimmed = tag.trim();
    let value = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    Version::parse(value).map_err(|error| format!("无法解析 Release 版本“{tag}”：{error}"))
}

fn current_target() -> Result<UpdateTarget, String> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return Ok(UpdateTarget::WindowsX64);
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return Ok(UpdateTarget::MacOSArm64);
    }
    #[allow(unreachable_code)]
    Err("Kite 更新暂不支持当前平台".to_string())
}

fn asset_matches_target(name: &str, target: UpdateTarget) -> bool {
    match target {
        UpdateTarget::WindowsX64 => name.eq_ignore_ascii_case("Kite.exe"),
        UpdateTarget::MacOSArm64 => {
            let lower = name.to_ascii_lowercase();
            lower.starts_with("kite-v") && lower.ends_with("macos-arm64.zip")
        }
    }
}

fn validate_asset_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || Path::new(name).file_name().and_then(|value| value.to_str()) != Some(name)
        || name.contains('/')
        || name.contains('\\')
    {
        return Err("更新包名称无效".to_string());
    }
    Ok(())
}

fn validate_asset_url(url: &str) -> Result<(), String> {
    if !url.starts_with(RELEASE_DOWNLOAD_PREFIX) || url.contains(char::is_whitespace) {
        return Err("更新包地址不在 Kite Release 中".to_string());
    }
    Ok(())
}

fn parse_sha256(value: &str) -> Result<String, String> {
    let value = value.strip_prefix("sha256:").unwrap_or(value).trim();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("更新包 SHA-256 摘要无效".to_string());
    }
    Ok(value.to_ascii_lowercase())
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60))
        .timeout_write(Duration::from_secs(60))
        .build()
}

pub fn download_update(info: &UpdateInfo) -> Result<PathBuf, String> {
    validate_asset_name(&info.asset_name)?;
    validate_asset_url(&info.asset_url)?;
    let expected_hash = parse_sha256(&info.asset_sha256)?;
    if info.asset_size > MAX_UPDATE_BYTES {
        return Err("更新包大小无效".to_string());
    }

    let version = parse_release_version(&info.latest_version)?.to_string();
    let directory = dirs::data_local_dir()
        .or_else(dirs::cache_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("Kite")
        .join("updates")
        .join(version);
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建更新目录：{error}"))?;
    let destination = directory.join(&info.asset_name);
    if destination.is_file()
        && destination.metadata().map(|meta| meta.len()).ok() == Some(info.asset_size)
        && sha256_file(&destination)? == expected_hash
    {
        return Ok(destination);
    }

    let partial = destination.with_extension("part");
    let _ = fs::remove_file(&partial);
    let response = agent()
        .get(&info.asset_url)
        .set("Accept", "application/octet-stream")
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("无法下载 Kite 更新：{error}"))?;
    let mut reader = response.into_reader().take(MAX_UPDATE_BYTES + 1);
    let mut file = File::create(&partial).map_err(|error| format!("无法创建更新文件：{error}"))?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("无法读取更新文件：{error}"))?;
        if count == 0 {
            break;
        }
        total = total.saturating_add(count as u64);
        if total > MAX_UPDATE_BYTES {
            drop(file);
            let _ = fs::remove_file(&partial);
            return Err("更新文件超过大小限制".to_string());
        }
        hasher.update(&buffer[..count]);
        file.write_all(&buffer[..count])
            .map_err(|error| format!("无法写入更新文件：{error}"))?;
    }
    file.flush()
        .map_err(|error| format!("无法写入更新文件：{error}"))?;
    file.sync_all()
        .map_err(|error| format!("无法保存更新文件：{error}"))?;
    drop(file);

    if info.asset_size > 0 && total != info.asset_size {
        let _ = fs::remove_file(&partial);
        return Err(format!(
            "更新文件大小不匹配：期望 {}，实际 {total}",
            info.asset_size
        ));
    }
    let actual_hash = format!("{:x}", hasher.finalize());
    if actual_hash != expected_hash {
        let _ = fs::remove_file(&partial);
        return Err("更新文件 SHA-256 校验失败".to_string());
    }
    if destination.exists() {
        fs::remove_file(&destination).map_err(|error| format!("无法替换旧更新文件：{error}"))?;
    }
    fs::rename(&partial, &destination).map_err(|error| format!("无法保存更新文件：{error}"))?;
    Ok(destination)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("无法读取更新文件：{error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("无法读取更新文件：{error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(target_os = "windows")]
pub fn install_windows_update(
    downloaded: &Path,
    expected_sha256: &str,
    version: &str,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt as _;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let version = parse_release_version(version)?.to_string();

    let target =
        std::env::current_exe().map_err(|error| format!("无法定位当前 Kite.exe：{error}"))?;
    let updates = dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Kite")
        .join("updates");
    fs::create_dir_all(&updates).map_err(|error| format!("无法创建更新目录：{error}"))?;
    let helper = updates.join(format!("apply-{version}-{}.ps1", std::process::id()));
    let log = updates.join("update.log");
    fs::write(&helper, WINDOWS_UPDATE_HELPER)
        .map_err(|error| format!("无法写入更新助手：{error}"))?;

    let powershell = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let mut command = Command::new(powershell);
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(&helper)
        .arg(std::process::id().to_string())
        .arg(downloaded)
        .arg(&target)
        .arg(parse_sha256(expected_sha256)?)
        .arg(&log)
        .arg(&helper)
        .creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .map_err(|error| format!("无法启动更新助手：{error}"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
const WINDOWS_UPDATE_HELPER: &str = r#"
param(
    [Parameter(Mandatory=$true)][int]$ProcessId,
    [Parameter(Mandatory=$true)][string]$Source,
    [Parameter(Mandatory=$true)][string]$Target,
    [Parameter(Mandatory=$true)][string]$ExpectedHash,
    [Parameter(Mandatory=$true)][string]$LogPath,
    [Parameter(Mandatory=$true)][string]$ScriptPath
)
$ErrorActionPreference = 'Stop'
function Write-UpdateLog([string]$Message) {
    $line = "$(Get-Date -Format o) $Message"
    Add-Content -LiteralPath $LogPath -Value $line -Encoding UTF8
}
$backup = "$Target.old"
$targetDirectory = Split-Path -Parent $Target
try {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($process) { $process.WaitForExit(60000) | Out-Null }
    $stream = $null
    for ($attempt = 0; $attempt -lt 120; $attempt++) {
        try {
            $stream = [IO.File]::Open($Target, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
            $stream.Close()
            break
        } catch {
            if ($attempt -eq 119) { throw }
            Start-Sleep -Milliseconds 250
        }
    }
    if (Test-Path -LiteralPath $backup) { Remove-Item -LiteralPath $backup -Force }
    if (Test-Path -LiteralPath $Target) { Move-Item -LiteralPath $Target -Destination $backup -Force }
    Copy-Item -LiteralPath $Source -Destination $Target -Force
    $actualHash = (Get-FileHash -LiteralPath $Target -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $ExpectedHash) { throw "SHA-256 mismatch" }
    Write-UpdateLog "Installed Kite update at $Target"
    $started = Start-Process -FilePath $Target -WorkingDirectory $targetDirectory -PassThru
    Start-Sleep -Seconds 2
    $started.Refresh()
    if ($started.HasExited) { throw "Updated Kite exited during startup" }
    if (Test-Path -LiteralPath $backup) { Remove-Item -LiteralPath $backup -Force }
    Remove-Item -LiteralPath $Source -Force -ErrorAction SilentlyContinue
} catch {
    Write-UpdateLog "Update failed: $($_.Exception.ToString())"
    if (Test-Path -LiteralPath $backup) {
        if (Test-Path -LiteralPath $Target) { Remove-Item -LiteralPath $Target -Force }
        Move-Item -LiteralPath $backup -Destination $Target -Force
    }
    if (Test-Path -LiteralPath $Target) {
        Start-Process -FilePath $Target -WorkingDirectory $targetDirectory
    }
    exit 1
} finally {
    Remove-Item -LiteralPath $ScriptPath -Force -ErrorAction SilentlyContinue
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn release_json(version: &str, digest: &str) -> String {
        json!({
            "tag_name": format!("v{version}"),
            "name": format!("Kite v{version}"),
            "html_url": format!("https://github.com/wyz541/kite/releases/tag/v{version}"),
            "body": "Release notes",
            "published_at": "2026-10-27T10:00:00Z",
            "draft": false,
            "prerelease": false,
            "assets": [
                {
                    "name": "Kite.exe",
                    "size": 1234,
                    "digest": format!("sha256:{digest}"),
                    "browser_download_url": format!("https://github.com/wyz541/kite/releases/download/v{version}/Kite.exe")
                },
                {
                    "name": format!("Kite-v{version}-macOS-arm64.zip"),
                    "size": 5678,
                    "digest": format!("sha256:{digest}"),
                    "browser_download_url": format!("https://github.com/wyz541/kite/releases/download/v{version}/Kite-v{version}-macOS-arm64.zip")
                }
            ]
        })
        .to_string()
    }

    fn parse(json: &str) -> GithubRelease {
        serde_json::from_str(json).expect("parse release fixture")
    }

    #[test]
    fn newer_windows_release_selects_verified_exe() {
        let digest = "a".repeat(64);
        let release = parse(&release_json("0.6.13", &digest));
        let info = resolve_release(
            release,
            &Version::parse("0.6.12").unwrap(),
            UpdateTarget::WindowsX64,
        )
        .expect("resolve update");

        assert!(info.update_available);
        assert_eq!(info.latest_version, "0.6.13");
        assert_eq!(info.asset_name, "Kite.exe");
        assert_eq!(info.asset_sha256, digest);
    }

    #[test]
    fn equal_version_reports_no_update() {
        let digest = "b".repeat(64);
        let release = parse(&release_json("0.6.12", &digest));
        let info = resolve_release(
            release,
            &Version::parse("0.6.12").unwrap(),
            UpdateTarget::MacOSArm64,
        )
        .expect("resolve equal version");

        assert!(!info.update_available);
        assert!(info.asset_name.is_empty());
    }

    #[test]
    fn update_requires_sha256_digest() {
        let digest = "c".repeat(64);
        let mut value: serde_json::Value =
            serde_json::from_str(&release_json("0.6.13", &digest)).unwrap();
        value["assets"][0]["digest"] = serde_json::Value::Null;
        let release: GithubRelease = serde_json::from_value(value).unwrap();
        let error = resolve_release(
            release,
            &Version::parse("0.6.12").unwrap(),
            UpdateTarget::WindowsX64,
        )
        .expect_err("missing digest must fail");

        assert!(error.contains("SHA-256"));
    }

    #[test]
    fn checksum_file_selects_exact_asset() {
        let checksums = "aa  Kite.exe\nbb *Kite-v0.6.13-windows-x64.zip\n";
        assert_eq!(checksum_for(checksums, "Kite.exe"), Some("aa".to_string()));
        assert_eq!(
            checksum_for(checksums, "Kite-v0.6.13-windows-x64.zip"),
            Some("bb".to_string())
        );
        assert_eq!(checksum_for(checksums, "Other.exe"), None);
    }

    #[test]
    fn update_url_must_stay_in_kite_release() {
        assert!(
            validate_asset_url("https://github.com/wyz541/kite/releases/download/v0.6.13/Kite.exe")
                .is_ok()
        );
        assert!(validate_asset_url("https://example.com/Kite.exe").is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "downloads the live Windows release asset"]
    fn live_windows_update_downloads_and_matches_digest() {
        let release = fetch_latest_release().expect("fetch latest release");
        let info = resolve_release(release, &Version::new(0, 0, 1), UpdateTarget::WindowsX64)
            .expect("resolve latest release");
        let path = info.download().expect("download release asset");

        assert!(path.is_file());
        assert_eq!(sha256_file(&path).unwrap(), info.asset_sha256);
        let _ = fs::remove_file(path);
    }

    #[test]
    #[ignore = "uses the live GitHub Release API"]
    fn live_github_release_has_a_verified_asset() {
        let release = fetch_latest_release().expect("fetch latest release");
        let info = resolve_release(release, &Version::new(0, 0, 1), current_target().unwrap())
            .expect("resolve latest release");

        assert!(info.update_available);
        assert!(!info.asset_name.is_empty());
        assert_eq!(info.asset_sha256.len(), 64);
    }
}
