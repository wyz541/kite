use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use kite_core::archive::{self, ArchiveFormat};
use kite_core::content_search::{
    CONTENT_SEARCH_MAX_FILES, ContentIndexState, ContentSearchService,
};
use kite_core::devices;
use kite_core::fs as kite_fs;
use kite_core::name_index::{NameIndexService, NameIndexState};
use kite_core::session::{
    SESSION_SCHEMA_VERSION, SessionStateDto, SessionTabDto, load_session_for_performance,
    save_session_for_performance,
};
use kite_core::text_editor::TextDocument;
use kite_core::updater;
use kite_core::vcs;
use serde::Serialize;
use serde_json::{Value, json};
use sysinfo::{Disks, Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const REPORT_SCHEMA_VERSION: u32 = 1;
const RSS_SAMPLE_INTERVAL: Duration = Duration::from_millis(10);
const CONTENT_INDEX_TIMEOUT: Duration = Duration::from_secs(180);
const NAME_INDEX_TIMEOUT: Duration = Duration::from_secs(180);
const INCREMENTAL_INDEX_TIMEOUT: Duration = Duration::from_secs(10);

const AREAS: [&str; 13] = [
    "Startup and launch",
    "Column navigation",
    "Tabs and session restore",
    "Directory watch and refresh",
    "Column search",
    "Global name search",
    "Global content search",
    "Preview and editor",
    "File operations",
    "Archive operations",
    "VCS and context menus",
    "Sidebar and devices",
    "Update path",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Profile {
    Smoke,
    Ci,
    Standard,
    Extended,
}

impl Profile {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "smoke" => Ok(Self::Smoke),
            "ci" => Ok(Self::Ci),
            "standard" => Ok(Self::Standard),
            "extended" => Ok(Self::Extended),
            _ => Err(format!(
                "unknown profile '{value}'; expected smoke, ci, standard, or extended"
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Ci => "ci",
            Self::Standard => "standard",
            Self::Extended => "extended",
        }
    }

    fn config(self) -> ProfileConfig {
        match self {
            Self::Smoke => ProfileConfig {
                directory_sizes: vec![1_000],
                tree_sizes: vec![5_000],
                content_files: 250,
                copy_files: 250,
                archive_files: 250,
                vcs_files: 1_000,
                large_file_mib: 32,
                samples: 5,
                cold_samples: 3,
            },
            Self::Ci => ProfileConfig {
                directory_sizes: vec![1_000, 10_000],
                tree_sizes: vec![10_000],
                content_files: 500,
                copy_files: 1_000,
                archive_files: 500,
                vcs_files: 5_000,
                large_file_mib: 64,
                samples: 10,
                cold_samples: 5,
            },
            Self::Standard => ProfileConfig {
                directory_sizes: vec![1_000, 10_000, 50_000],
                tree_sizes: vec![10_000, 100_000],
                content_files: 2_000,
                copy_files: 1_000,
                archive_files: 2_000,
                vcs_files: 10_000,
                large_file_mib: 256,
                samples: 30,
                cold_samples: 10,
            },
            Self::Extended => ProfileConfig {
                directory_sizes: vec![1_000, 10_000, 50_000],
                tree_sizes: vec![10_000, 100_000, 250_000],
                content_files: 5_000,
                copy_files: 10_000,
                archive_files: 5_000,
                vcs_files: 50_000,
                large_file_mib: 1_024,
                samples: 50,
                cold_samples: 20,
            },
        }
    }
}

#[derive(Clone, Debug)]
struct ProfileConfig {
    directory_sizes: Vec<usize>,
    tree_sizes: Vec<usize>,
    content_files: usize,
    copy_files: usize,
    archive_files: usize,
    vcs_files: usize,
    large_file_mib: u64,
    samples: usize,
    cold_samples: usize,
}

#[derive(Debug)]
struct Args {
    profile: Profile,
    output: Option<PathBuf>,
    markdown: Option<PathBuf>,
    samples: Option<usize>,
    cold_samples: Option<usize>,
    fixture_root: Option<PathBuf>,
    keep_fixtures: bool,
    include_network: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            profile: Profile::Standard,
            output: None,
            markdown: None,
            samples: None,
            cold_samples: None,
            fixture_root: None,
            keep_fixtures: false,
            include_network: false,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("kite-perf: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args()?;
    let mut profile = args.profile.config();
    if let Some(samples) = args.samples {
        if samples == 0 {
            return Err("--samples must be greater than zero".to_string());
        }
        profile.samples = samples;
    }
    if let Some(samples) = args.cold_samples {
        if samples == 0 {
            return Err("--cold-samples must be greater than zero".to_string());
        }
        profile.cold_samples = samples;
    }

    let fixture_root = args
        .fixture_root
        .clone()
        .unwrap_or_else(unique_fixture_root);
    if fixture_root.exists() {
        return Err(format!(
            "fixture root already exists: {}; choose a new path",
            fixture_root.display()
        ));
    }
    fs::create_dir_all(&fixture_root)
        .map_err(|error| format!("cannot create fixture root: {error}"))?;
    let fixture_guard = FixtureGuard {
        root: fixture_root.clone(),
        keep: args.keep_fixtures,
    };

    eprintln!(
        "kite-perf: preparing {} fixtures in {}",
        args.profile.name(),
        fixture_root.display()
    );
    let fixtures = FixtureSet::create(&fixture_root, &profile)?;
    eprintln!(
        "kite-perf: fixtures ready; {} profile, {} interactive samples, {} cold samples",
        args.profile.name(),
        profile.samples,
        profile.cold_samples
    );

    let host = HostInfo::collect(&fixture_root)?;
    let mut results = Vec::new();

    measure_column_navigation(&fixtures, &profile, &mut results)?;
    measure_tabs_and_session(&fixtures, &profile, &mut results)?;
    measure_directory_watch(&fixtures, &profile, &mut results)?;
    measure_name_searches(&fixtures, &profile, &mut results)?;
    measure_name_index(&fixtures, &profile, &mut results)?;
    measure_content_search(&fixtures, &profile, &mut results)?;
    measure_preview_and_editor(&fixtures, &profile, &mut results)?;
    measure_file_operations(&fixtures, &profile, &mut results)?;
    measure_archive_operations(&fixtures, &profile, &mut results)?;
    measure_vcs(&fixtures, &profile, &mut results)?;
    measure_devices(&profile, &mut results)?;
    measure_update_path(&fixtures, &profile, args.include_network, &mut results)?;

    add_known_gaps(&fixtures, &profile, &mut results);
    let coverage = build_coverage(&results);
    let summary = build_summary(&results);
    let report = PerformanceReport {
        schema_version: REPORT_SCHEMA_VERSION,
        generated_at_unix_ms: now_unix_ms(),
        kite_version: updater::current_version().to_string(),
        git_commit: git_commit(),
        profile: args.profile.name().to_string(),
        build_profile: if cfg!(debug_assertions) {
            "debug".to_string()
        } else {
            "release".to_string()
        },
        host,
        method: MethodInfo {
            timer: "std::time::Instant (monotonic)".to_string(),
            percentile: "linear interpolation over sorted samples".to_string(),
            rss: format!(
                "sysinfo resident memory sampled every {} ms; peak is max of sampler and post-sample RSS",
                RSS_SAMPLE_INTERVAL.as_millis()
            ),
            cache_control: "fixtures are generated locally; the harness does not purge OS caches"
                .to_string(),
            interactive_samples: profile.samples,
            cold_samples: profile.cold_samples,
        },
        fixtures: fixtures.manifest.clone(),
        coverage,
        summary,
        results,
    };

    let output = args
        .output
        .unwrap_or_else(|| default_output_path(&args.profile));
    write_json(&report, &output)?;
    eprintln!("kite-perf: wrote {}", output.display());

    if let Some(markdown) = args.markdown {
        write_markdown(&report, &markdown, &output)?;
        eprintln!("kite-perf: wrote {}", markdown.display());
    }

    if report.summary.errors > 0 {
        return Err(format!(
            "{} benchmark scenario(s) returned an error",
            report.summary.errors
        ));
    }

    drop(fixture_guard);
    Ok(())
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();
    let mut values = env::args().skip(1);
    while let Some(value) = values.next() {
        match value.as_str() {
            "--profile" => {
                let raw = values
                    .next()
                    .ok_or_else(|| "--profile requires a value".to_string())?;
                args.profile = Profile::parse(&raw)?;
            }
            "--output" => {
                args.output = Some(PathBuf::from(
                    values
                        .next()
                        .ok_or_else(|| "--output requires a path".to_string())?,
                ));
            }
            "--markdown" => {
                args.markdown = Some(PathBuf::from(
                    values
                        .next()
                        .ok_or_else(|| "--markdown requires a path".to_string())?,
                ));
            }
            "--samples" => {
                args.samples = Some(parse_positive_usize(
                    values
                        .next()
                        .ok_or_else(|| "--samples requires a value".to_string())?,
                    "--samples",
                )?);
            }
            "--cold-samples" => {
                args.cold_samples = Some(parse_positive_usize(
                    values
                        .next()
                        .ok_or_else(|| "--cold-samples requires a value".to_string())?,
                    "--cold-samples",
                )?);
            }
            "--fixture-root" => {
                args.fixture_root =
                    Some(PathBuf::from(values.next().ok_or_else(|| {
                        "--fixture-root requires a path".to_string()
                    })?));
            }
            "--keep-fixtures" => args.keep_fixtures = true,
            "--include-network" => args.include_network = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

fn parse_positive_usize(value: String, flag: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{flag} must be a positive integer"))
}

fn print_help() {
    println!(
        "kite-perf {}\n\nUsage: cargo run --release --features performance --bin kite-perf -- [options]\n\nOptions:\n  --profile <smoke|ci|standard|extended>  Fixture and sample preset (default: standard)\n  --output <path>                         JSON report path\n  --markdown <path>                       Optional Markdown summary path\n  --samples <n>                           Override interactive sample count\n  --cold-samples <n>                      Override cold-start/build sample count\n  --fixture-root <path>                   Use a new explicit temporary fixture directory\n  --keep-fixtures                         Keep generated fixtures after the run\n  --include-network                       Measure the GitHub update check as an external boundary\n  --help                                  Print this help",
        env!("CARGO_PKG_VERSION")
    );
}

struct FixtureGuard {
    root: PathBuf,
    keep: bool,
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.root);
        } else {
            eprintln!("kite-perf: kept fixtures at {}", self.root.display());
        }
    }
}

#[derive(Clone, Debug)]
struct FixtureSet {
    root: PathBuf,
    directories: BTreeMap<usize, PathBuf>,
    trees: BTreeMap<usize, PathBuf>,
    content_dir: PathBuf,
    copy_root: PathBuf,
    archive_root: PathBuf,
    vcs_dir: Option<PathBuf>,
    git_available: bool,
    svn_available: bool,
    large_file: PathBuf,
    text_files: BTreeMap<u64, PathBuf>,
    manifest: Vec<FixtureDescription>,
}

#[derive(Clone, Debug, Serialize)]
struct FixtureDescription {
    name: String,
    kind: String,
    entries: u64,
    bytes: u64,
    description: String,
}

impl FixtureSet {
    fn create(root: &Path, config: &ProfileConfig) -> Result<Self, String> {
        let directories_root = root.join("directories");
        let trees_root = root.join("trees");
        let content_dir = root.join("content");
        let copy_root = root.join("copy");
        let archive_root = root.join("archive");
        let session_dir = root.join("session");
        let large_dir = root.join("large");
        let text_dir = root.join("text");
        let vcs_dir = root.join("vcs");
        for directory in [
            &directories_root,
            &trees_root,
            &content_dir,
            &copy_root,
            &archive_root,
            &session_dir,
            &large_dir,
            &text_dir,
            &vcs_dir,
        ] {
            fs::create_dir_all(directory)
                .map_err(|error| format!("cannot create fixture directory: {error}"))?;
        }

        let mut manifest = Vec::new();
        let mut directories = BTreeMap::new();
        for size in &config.directory_sizes {
            let path = directories_root.join(format!("flat-{size}"));
            let bytes = create_flat_files(&path, *size)?;
            manifest.push(FixtureDescription {
                name: format!("directories/flat-{size}"),
                kind: "flat_directory".to_string(),
                entries: *size as u64,
                bytes,
                description: "Flat directory used for load and sort measurements".to_string(),
            });
            directories.insert(*size, path);
        }

        let mut trees = BTreeMap::new();
        for size in &config.tree_sizes {
            let path = trees_root.join(format!("tree-{size}"));
            let bytes = create_tree_files(&path, *size)?;
            manifest.push(FixtureDescription {
                name: format!("trees/tree-{size}"),
                kind: "nested_tree".to_string(),
                entries: *size as u64,
                bytes,
                description: "Nested tree used for column and global name search".to_string(),
            });
            trees.insert(*size, path);
        }

        let content_bytes = create_content_files(&content_dir, config.content_files)?;
        manifest.push(FixtureDescription {
            name: "content".to_string(),
            kind: "text_corpus".to_string(),
            entries: config.content_files as u64,
            bytes: content_bytes,
            description: "UTF-8 corpus used for content indexing and incremental updates"
                .to_string(),
        });

        let copy_source = copy_root.join("copy-source");
        let copy_bytes = create_flat_files(&copy_source, config.copy_files)?;
        fs::create_dir_all(copy_root.join("copy-destinations"))
            .map_err(|error| format!("cannot create copy destinations: {error}"))?;
        manifest.push(FixtureDescription {
            name: "copy/copy-source".to_string(),
            kind: "copy_source".to_string(),
            entries: config.copy_files as u64,
            bytes: copy_bytes,
            description: "Small-file tree used for copy operations".to_string(),
        });

        let move_sources_root = copy_root.join("move-sources");
        fs::create_dir_all(&move_sources_root)
            .map_err(|error| format!("cannot create move sources: {error}"))?;
        for index in 0..config.cold_samples.max(1) {
            create_flat_files(&move_sources_root.join(format!("source-{index}")), 100)?;
        }
        manifest.push(FixtureDescription {
            name: "copy/move-sources".to_string(),
            kind: "move_sources".to_string(),
            entries: (config.cold_samples.max(1) * 100) as u64,
            bytes: (config.cold_samples.max(1) * 100 * 64) as u64,
            description: "Precreated sources used for move measurements".to_string(),
        });

        let archive_source = archive_root.join("source");
        let archive_bytes = create_content_files(&archive_source, config.archive_files)?;
        manifest.push(FixtureDescription {
            name: "archive/source".to_string(),
            kind: "archive_source".to_string(),
            entries: config.archive_files as u64,
            bytes: archive_bytes,
            description: "Compressible text tree used for archive measurements".to_string(),
        });

        let large_file = large_dir.join(format!("large-{}.bin", config.large_file_mib));
        let large_bytes = create_pattern_file(&large_file, config.large_file_mib * 1024 * 1024)?;
        manifest.push(FixtureDescription {
            name: format!("large/large-{}.bin", config.large_file_mib),
            kind: "large_file".to_string(),
            entries: 1,
            bytes: large_bytes,
            description: "Deterministic binary file used for I/O, hash, and copy measurements"
                .to_string(),
        });

        let mut text_files = BTreeMap::new();
        for mib in [1_u64, 8, 100] {
            let path = text_dir.join(format!("text-{mib}mib.txt"));
            let bytes = create_text_file(&path, mib * 1024 * 1024)?;
            manifest.push(FixtureDescription {
                name: format!("text/text-{mib}mib.txt"),
                kind: "text_file".to_string(),
                entries: 1,
                bytes,
                description: "Deterministic UTF-8 file used for preview and editor measurements"
                    .to_string(),
            });
            text_files.insert(mib, path);
        }

        let git_available = command_available("git", &["--version"]);
        let svn_available = command_available("svn", &["--version", "--quiet"]);
        let vcs_dir = if git_available {
            create_git_fixture(&vcs_dir, config.vcs_files)?;
            manifest.push(FixtureDescription {
                name: "vcs/git".to_string(),
                kind: "git_worktree".to_string(),
                entries: config.vcs_files as u64,
                bytes: (config.vcs_files * 64) as u64,
                description: "Git worktree used for clean and dirty VCS status measurements"
                    .to_string(),
            });
            Some(vcs_dir)
        } else {
            None
        };

        Ok(Self {
            root: root.to_path_buf(),
            directories,
            trees,
            content_dir,
            copy_root,
            archive_root,
            vcs_dir,
            git_available,
            svn_available,
            large_file,
            text_files,
            manifest,
        })
    }
}

fn create_flat_files(directory: &Path, count: usize) -> Result<u64, String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create flat fixture: {error}"))?;
    let bytes = vec![b'x'; 64];
    let mut written = 0_u64;
    for index in 0..count {
        let name = if index % 101 == 0 {
            format!("needle-{index:06}.txt")
        } else {
            format!("file-{index:06}.txt")
        };
        fs::write(directory.join(name), &bytes)
            .map_err(|error| format!("cannot write flat fixture file: {error}"))?;
        written += bytes.len() as u64;
    }
    Ok(written)
}

fn create_tree_files(root: &Path, count: usize) -> Result<u64, String> {
    fs::create_dir_all(root).map_err(|error| format!("cannot create tree fixture: {error}"))?;
    let mut written = 0_u64;
    let bytes = vec![b't'; 64];
    for index in 0..count {
        let bucket = index / 100;
        let directory = root.join(format!("bucket-{bucket:05}"));
        if index % 100 == 0 {
            fs::create_dir_all(&directory)
                .map_err(|error| format!("cannot create tree bucket: {error}"))?;
        }
        let name = if index % 101 == 0 {
            format!("needle-{index:06}.txt")
        } else {
            format!("file-{index:06}.txt")
        };
        fs::write(directory.join(name), &bytes)
            .map_err(|error| format!("cannot write tree fixture file: {error}"))?;
        written += bytes.len() as u64;
    }
    Ok(written)
}

fn create_content_files(directory: &Path, count: usize) -> Result<u64, String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create content fixture: {error}"))?;
    let mut written = 0_u64;
    for index in 0..count {
        let padding = "Kite performance fixture line. ".repeat(48);
        let content = format!(
            "baseline_needle document {index}\n{padding}\nunique token: baseline-{index:06}\n"
        );
        let path = directory.join(format!("content-{index:06}.txt"));
        fs::write(&path, content.as_bytes())
            .map_err(|error| format!("cannot write content fixture: {error}"))?;
        written += content.len() as u64;
    }
    Ok(written)
}

fn create_pattern_file(path: &Path, size: u64) -> Result<u64, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create large-file directory: {error}"))?;
    }
    let file = File::create(path).map_err(|error| format!("cannot create large file: {error}"))?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut remaining = size;
    while remaining > 0 {
        let chunk = remaining.min(buffer.len() as u64) as usize;
        for byte in &mut buffer[..chunk] {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        writer
            .write_all(&buffer[..chunk])
            .map_err(|error| format!("cannot write large file: {error}"))?;
        remaining -= chunk as u64;
    }
    writer
        .flush()
        .map_err(|error| format!("cannot flush large file: {error}"))?;
    Ok(size)
}

fn create_text_file(path: &Path, size: u64) -> Result<u64, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create text-file directory: {error}"))?;
    }
    let file = File::create(path).map_err(|error| format!("cannot create text file: {error}"))?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    let line = "Kite performance text fixture with deterministic content. 0123456789\n";
    let mut written = 0_u64;
    while written < size {
        let count = (size - written).min(line.len() as u64) as usize;
        writer
            .write_all(&line.as_bytes()[..count])
            .map_err(|error| format!("cannot write text file: {error}"))?;
        written += count as u64;
    }
    writer
        .flush()
        .map_err(|error| format!("cannot flush text file: {error}"))?;
    Ok(written)
}

fn create_git_fixture(directory: &Path, count: usize) -> Result<(), String> {
    create_flat_files(directory, count)?;
    run_command(directory, "git", &["init", "--quiet"])?;
    run_command(directory, "git", &["add", "."])?;
    run_command(
        directory,
        "git",
        &[
            "-c",
            "user.name=Kite Performance",
            "-c",
            "user.email=performance@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "performance fixture",
        ],
    )?;
    Ok(())
}

fn command_available(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn run_command(directory: &Path, command: &str, args: &[&str]) -> Result<(), String> {
    let output = Command::new(command)
        .current_dir(directory)
        .args(args)
        .output()
        .map_err(|error| format!("cannot run {command}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "{command} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

#[derive(Clone, Debug, Serialize)]
struct HostInfo {
    os: String,
    os_version: Option<String>,
    architecture: String,
    cpu_model: String,
    logical_cores: usize,
    memory_bytes: u64,
    fixture_filesystem: String,
    fixture_disk_kind: String,
    fixture_disk_total_bytes: u64,
    fixture_disk_available_bytes: u64,
}

impl HostInfo {
    fn collect(fixture_root: &Path) -> Result<Self, String> {
        let mut system = System::new_all();
        system.refresh_all();
        let cpu_model = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(System::cpu_arch);
        let disks = Disks::new_with_refreshed_list();
        let disk = disks
            .list()
            .iter()
            .filter(|disk| fixture_root.starts_with(disk.mount_point()))
            .max_by_key(|disk| disk.mount_point().components().count());
        Ok(Self {
            os: env::consts::OS.to_string(),
            os_version: System::long_os_version(),
            architecture: env::consts::ARCH.to_string(),
            cpu_model,
            logical_cores: system.cpus().len(),
            memory_bytes: system.total_memory(),
            fixture_filesystem: disk
                .map(|disk| disk.file_system().to_string_lossy().into_owned())
                .unwrap_or_else(|| "unknown".to_string()),
            fixture_disk_kind: disk
                .map(|disk| format!("{:?}", disk.kind()))
                .unwrap_or_else(|| "unknown".to_string()),
            fixture_disk_total_bytes: disk.map_or(0, |disk| disk.total_space()),
            fixture_disk_available_bytes: disk.map_or(0, |disk| disk.available_space()),
        })
    }
}

#[derive(Clone, Debug, Serialize)]
struct MethodInfo {
    timer: String,
    percentile: String,
    rss: String,
    cache_control: String,
    interactive_samples: usize,
    cold_samples: usize,
}

#[derive(Clone, Debug, Serialize)]
struct PerformanceReport {
    schema_version: u32,
    generated_at_unix_ms: u64,
    kite_version: String,
    git_commit: String,
    profile: String,
    build_profile: String,
    host: HostInfo,
    method: MethodInfo,
    fixtures: Vec<FixtureDescription>,
    coverage: Vec<CoverageEntry>,
    summary: ReportSummary,
    results: Vec<MetricResult>,
}

#[derive(Clone, Debug, Serialize)]
struct MetricResult {
    area: String,
    scenario: String,
    boundary: String,
    status: String,
    unit: String,
    samples: Vec<BenchmarkSample>,
    summary: Option<MetricSummary>,
    details: Option<Value>,
    notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct BenchmarkSample {
    iteration: usize,
    duration_ms: f64,
    items: Option<u64>,
    bytes: Option<u64>,
    throughput_per_second: Option<f64>,
    throughput_unit: Option<String>,
    baseline_rss_bytes: u64,
    peak_rss_bytes: u64,
    peak_rss_delta_bytes: u64,
    cpu_ms: u64,
    read_bytes: u64,
    written_bytes: u64,
}

#[derive(Clone, Debug, Serialize)]
struct MetricSummary {
    sample_count: usize,
    min_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
    mean_ms: f64,
    throughput_p50: Option<f64>,
    throughput_unit: Option<String>,
    peak_rss_delta_bytes_max: u64,
    cpu_ms_p50: u64,
}

#[derive(Clone, Debug, Serialize)]
struct CoverageEntry {
    area: String,
    status: String,
    automated_scenarios: Vec<String>,
    gaps: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct ReportSummary {
    measured_scenarios: usize,
    not_automated_scenarios: usize,
    errors: usize,
    slowest_scenarios: Vec<ScenarioRank>,
    highest_variance_scenarios: Vec<ScenarioVarianceRank>,
    highest_memory_scenarios: Vec<ScenarioMemoryRank>,
}

#[derive(Clone, Debug, Serialize)]
struct ScenarioRank {
    area: String,
    scenario: String,
    p95_ms: f64,
    sample_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct ScenarioVarianceRank {
    area: String,
    scenario: String,
    p95_to_p50_ratio: f64,
    sample_count: usize,
}

#[derive(Clone, Debug, Serialize)]
struct ScenarioMemoryRank {
    area: String,
    scenario: String,
    peak_rss_delta_bytes: u64,
    sample_count: usize,
}

#[derive(Clone, Debug)]
struct WorkAmount {
    items: Option<u64>,
    bytes: Option<u64>,
    throughput_unit: Option<String>,
    details: Option<Value>,
}

impl WorkAmount {
    fn items(items: u64) -> Self {
        Self {
            items: Some(items),
            bytes: None,
            throughput_unit: Some("items/s".to_string()),
            details: None,
        }
    }

    fn bytes(bytes: u64) -> Self {
        Self {
            items: None,
            bytes: Some(bytes),
            throughput_unit: Some("MiB/s".to_string()),
            details: None,
        }
    }

    fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[allow(clippy::too_many_arguments)]
fn measure_scenario<F>(
    results: &mut Vec<MetricResult>,
    area: &str,
    scenario: &str,
    boundary: &str,
    sample_count: usize,
    warmups: usize,
    notes: Vec<String>,
    mut operation: F,
) -> bool
where
    F: FnMut(usize) -> Result<WorkAmount, String>,
{
    eprintln!("kite-perf: {area} / {scenario}");
    for warmup in 0..warmups {
        if let Err(error) = operation(warmup) {
            results.push(error_result(
                area,
                scenario,
                boundary,
                notes,
                format!("warm-up failed: {error}"),
            ));
            return false;
        }
    }

    let mut samples = Vec::with_capacity(sample_count);
    let mut details = None;
    let mut error_message = None;
    for iteration in 0..sample_count {
        let baseline = process_snapshot();
        let sampler = RssSampler::start();
        let started = Instant::now();
        let result = operation(iteration);
        let elapsed = started.elapsed();
        let sampled_peak = sampler.stop();
        let after = process_snapshot();
        match result {
            Ok(work) => {
                let duration_ms = elapsed.as_secs_f64() * 1_000.0;
                let throughput = throughput(&work, duration_ms);
                details = work.details;
                samples.push(BenchmarkSample {
                    iteration,
                    duration_ms,
                    items: work.items,
                    bytes: work.bytes,
                    throughput_per_second: throughput,
                    throughput_unit: work.throughput_unit,
                    baseline_rss_bytes: baseline.rss_bytes,
                    peak_rss_bytes: sampled_peak.max(after.rss_bytes),
                    peak_rss_delta_bytes: sampled_peak
                        .max(after.rss_bytes)
                        .saturating_sub(baseline.rss_bytes),
                    cpu_ms: after.cpu_ms.saturating_sub(baseline.cpu_ms),
                    read_bytes: after.read_bytes.saturating_sub(baseline.read_bytes),
                    written_bytes: after.write_bytes.saturating_sub(baseline.write_bytes),
                });
            }
            Err(error) => {
                error_message = Some(error);
                break;
            }
        }
    }

    if let Some(error) = error_message {
        eprintln!("kite-perf: {area} / {scenario} failed: {error}");
        let mut result = error_result(area, scenario, boundary, notes, error);
        result.samples = samples;
        results.push(result);
        false
    } else {
        let summary = summarize_samples(&samples);
        results.push(MetricResult {
            area: area.to_string(),
            scenario: scenario.to_string(),
            boundary: boundary.to_string(),
            status: "measured".to_string(),
            unit: "ms".to_string(),
            samples,
            summary: Some(summary),
            details,
            notes,
        });
        true
    }
}

fn throughput(work: &WorkAmount, duration_ms: f64) -> Option<f64> {
    if duration_ms <= f64::EPSILON {
        return None;
    }
    let seconds = duration_ms / 1_000.0;
    let value = match work.throughput_unit.as_deref() {
        Some("MiB/s") => work.bytes? as f64 / 1024.0 / 1024.0 / seconds,
        Some("items/s") => work.items? as f64 / seconds,
        _ => return None,
    };
    Some(value)
}

fn summarize_samples(samples: &[BenchmarkSample]) -> MetricSummary {
    let mut durations = samples
        .iter()
        .map(|sample| sample.duration_ms)
        .collect::<Vec<_>>();
    durations.sort_by(f64::total_cmp);
    let p50_ms = percentile(&durations, 0.50);
    let mut throughputs = samples
        .iter()
        .filter_map(|sample| sample.throughput_per_second)
        .collect::<Vec<_>>();
    throughputs.sort_by(f64::total_cmp);
    let mut cpu_times = samples
        .iter()
        .map(|sample| sample.cpu_ms)
        .collect::<Vec<_>>();
    cpu_times.sort_unstable();
    MetricSummary {
        sample_count: samples.len(),
        min_ms: durations.first().copied().unwrap_or_default(),
        p50_ms,
        p95_ms: percentile(&durations, 0.95),
        max_ms: durations.last().copied().unwrap_or_default(),
        mean_ms: if durations.is_empty() {
            0.0
        } else {
            durations.iter().sum::<f64>() / durations.len() as f64
        },
        throughput_p50: (!throughputs.is_empty()).then(|| percentile(&throughputs, 0.50)),
        throughput_unit: samples
            .first()
            .and_then(|sample| sample.throughput_unit.clone()),
        peak_rss_delta_bytes_max: samples
            .iter()
            .map(|sample| sample.peak_rss_delta_bytes)
            .max()
            .unwrap_or_default(),
        cpu_ms_p50: cpu_times
            .get(cpu_times.len().saturating_sub(1) / 2)
            .copied()
            .unwrap_or_default(),
    }
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    if values.len() == 1 {
        return values[0];
    }
    let rank = (values.len() - 1) as f64 * percentile.clamp(0.0, 1.0);
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        values[lower]
    } else {
        let weight = rank - lower as f64;
        values[lower] * (1.0 - weight) + values[upper] * weight
    }
}

fn error_result(
    area: &str,
    scenario: &str,
    boundary: &str,
    notes: Vec<String>,
    error: String,
) -> MetricResult {
    let mut notes = notes;
    notes.push(format!("error: {error}"));
    MetricResult {
        area: area.to_string(),
        scenario: scenario.to_string(),
        boundary: boundary.to_string(),
        status: "error".to_string(),
        unit: "ms".to_string(),
        samples: Vec::new(),
        summary: None,
        details: None,
        notes,
    }
}

fn record_gap(results: &mut Vec<MetricResult>, area: &str, boundary: &str, gap: &str) {
    results.push(MetricResult {
        area: area.to_string(),
        scenario: gap.to_string(),
        boundary: boundary.to_string(),
        status: "not_automated".to_string(),
        unit: "none".to_string(),
        samples: Vec::new(),
        summary: None,
        details: None,
        notes: vec![
            "This boundary requires a native UI runner or a controlled external device."
                .to_string(),
        ],
    });
}

fn measure_column_navigation(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    for (size, path) in &fixtures.directories {
        let path = path.clone();
        let expected = *size;
        measure_scenario(
            results,
            "Column navigation",
            &format!("load_directory_{size}_name_sort"),
            "core",
            config.samples,
            1,
            vec![
                "Measures directory metadata read, hidden filtering, and name sorting.".to_string(),
                "The result excludes native list rendering.".to_string(),
            ],
            |_| {
                let snapshot = kite_fs::load_directory(&path, kite_fs::SortSpec::default())?;
                if snapshot.entries.len() != expected {
                    return Err(format!(
                        "expected {expected} entries, received {}",
                        snapshot.entries.len()
                    ));
                }
                Ok(WorkAmount::items(expected as u64))
            },
        );
    }

    if let Some((size, path)) = fixtures.directories.iter().next_back() {
        let entries = kite_fs::load_directory(path, kite_fs::SortSpec::default())?.entries;
        for key in [
            kite_fs::SortKey::Name,
            kite_fs::SortKey::Modified,
            kite_fs::SortKey::Kind,
            kite_fs::SortKey::Size,
            kite_fs::SortKey::Vcs,
        ] {
            let source = entries.clone();
            let label = format!("{key:?}").to_ascii_lowercase();
            measure_scenario(
                results,
                "Column navigation",
                &format!("sort_entries_{size}_{label}"),
                "core",
                config.samples,
                1,
                vec!["Measures the Core sort step on an already loaded directory.".to_string()],
                |_| {
                    let mut values = source.clone();
                    kite_fs::sort_entries(
                        &mut values,
                        kite_fs::SortSpec {
                            key,
                            ascending: true,
                        },
                    );
                    Ok(WorkAmount::items(values.len() as u64))
                },
            );
        }
    }

    record_gap(
        results,
        "Column navigation",
        "windows_runtime",
        "native GPUI next-column, filter, group, and frame-stall measurement",
    );
    record_gap(
        results,
        "Column navigation",
        "macos_runtime",
        "native SwiftUI column rendering and frame-stall measurement",
    );
    Ok(())
}

fn measure_tabs_and_session(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    let session_path = fixtures.root.join("session/session.json");
    let state = SessionStateDto {
        schema_version: SESSION_SCHEMA_VERSION,
        active_tab: 19,
        tabs: (0..20)
            .map(|index| SessionTabDto {
                current_directory: format!("/fixture/tab-{index:02}"),
                view_mode: "columns".to_string(),
                show_path_bar: true,
            })
            .collect(),
    };
    save_session_for_performance(&session_path, &state)?;

    measure_scenario(
        results,
        "Tabs and session restore",
        "load_session_20_tabs",
        "core",
        config.samples,
        1,
        vec!["Measures Core session parsing and normalization.".to_string()],
        |_| {
            let loaded = load_session_for_performance(&session_path)
                .ok_or_else(|| "session could not be loaded".to_string())?;
            if loaded.tabs.len() != 20 {
                return Err(format!("expected 20 tabs, received {}", loaded.tabs.len()));
            }
            Ok(WorkAmount::items(loaded.tabs.len() as u64))
        },
    );

    measure_scenario(
        results,
        "Tabs and session restore",
        "save_session_20_tabs",
        "core",
        config.samples,
        1,
        vec!["Measures atomic session serialization and disk sync.".to_string()],
        |_| {
            save_session_for_performance(&session_path, &state)?;
            Ok(WorkAmount::items(state.tabs.len() as u64))
        },
    );

    record_gap(
        results,
        "Tabs and session restore",
        "windows_runtime",
        "native GPUI tab open, switch, close, and restore-render measurement",
    );
    record_gap(
        results,
        "Tabs and session restore",
        "macos_runtime",
        "native SwiftUI tab open, switch, close, and restore-render measurement",
    );
    Ok(())
}

#[cfg(target_os = "windows")]
fn measure_directory_watch(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    use kite_core::file_watcher::DirectoryWatcher;
    use std::sync::mpsc;

    let watched = fixtures.root.join("watch");
    fs::create_dir_all(&watched)
        .map_err(|error| format!("cannot create watch fixture: {error}"))?;
    let (sender, receiver) = mpsc::channel();
    let mut watcher = DirectoryWatcher::new(move |_| {
        let _ = sender.send(Instant::now());
    })?;
    watcher.set_paths([watched.clone()]);

    measure_scenario(
        results,
        "Directory watch and refresh",
        "windows_notify_create_latency",
        "windows_runtime",
        config.cold_samples,
        0,
        vec![
            "Measures the Windows ReadDirectoryChangesW-style watcher boundary.".to_string(),
            "File creation time is included because it triggers the event.".to_string(),
        ],
        |iteration| {
            let path = watched.join(format!("created-{iteration:04}.txt"));
            fs::write(&path, b"created").map_err(|error| error.to_string())?;
            receiver
                .recv_timeout(Duration::from_secs(5))
                .map_err(|error| format!("watcher event timeout: {error}"))?;
            Ok(WorkAmount::items(1))
        },
    );
    watcher.clear();
    record_gap(
        results,
        "Directory watch and refresh",
        "windows_runtime",
        "GPUI refresh application and frame-stall measurement",
    );
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn measure_directory_watch(
    _fixtures: &FixtureSet,
    _config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    record_gap(
        results,
        "Directory watch and refresh",
        "macos_runtime",
        "Swift DispatchSource event-to-refresh and frame-stall measurement",
    );
    Ok(())
}

fn measure_name_searches(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    for (size, path) in &fixtures.trees {
        let path = path.clone();
        let tree_size = *size;
        measure_scenario(
            results,
            "Column search",
            &format!("search_directory_{size}"),
            "core",
            config.samples,
            1,
            vec![
                "Recursive name search uses the same Core entry point as the column search."
                    .to_string(),
                "Throughput uses the fixture entry count and is therefore a lower bound."
                    .to_string(),
            ],
            |_| {
                let cancel = AtomicBool::new(false);
                let snapshot = kite_fs::search_directory(
                    &path,
                    "needle",
                    kite_fs::SortSpec::default(),
                    &cancel,
                )?
                .ok_or_else(|| "search was cancelled".to_string())?;
                if snapshot.entries.is_empty() {
                    return Err("column search returned no matches".to_string());
                }
                Ok(WorkAmount::items(tree_size as u64).with_details(json!({
                    "matched_entries": snapshot.entries.len(),
                    "truncated": snapshot.truncated,
                })))
            },
        );
    }

    for (size, path) in &fixtures.trees {
        let roots = vec![path.clone()];
        let tree_size = *size;
        measure_scenario(
            results,
            "Global name search",
            &format!("global_name_search_{size}"),
            "core",
            config.samples,
            1,
            vec![
                "Measures the single-root Core traversal and ranking path.".to_string(),
                "The Core search traverses the complete selected root.".to_string(),
            ],
            |_| {
                let cancel = AtomicBool::new(false);
                let snapshot = kite_fs::search_global_names(&roots, "needle", &cancel)?
                    .ok_or_else(|| "global name search was cancelled".to_string())?;
                if snapshot.entries.is_empty() {
                    return Err("global name search returned no matches".to_string());
                }
                Ok(WorkAmount::items(tree_size as u64).with_details(json!({
                    "matched_entries": snapshot.entries.len(),
                    "truncated": snapshot.truncated,
                })))
            },
        );
    }

    if let (Some((directory_size, directory)), Some((tree_size, tree))) = (
        fixtures.directories.iter().next_back(),
        fixtures.trees.iter().next(),
    ) {
        let roots = vec![directory.clone(), tree.clone()];
        let total = directory_size.saturating_add(*tree_size) as u64;
        measure_scenario(
            results,
            "Global name search",
            "global_name_search_multi_root",
            "core",
            config.samples,
            1,
            vec![
                "Measures normalization, traversal, de-duplication, and ranking for two roots."
                    .to_string(),
            ],
            |_| {
                let cancel = AtomicBool::new(false);
                let snapshot = kite_fs::search_global_names(&roots, "needle", &cancel)?
                    .ok_or_else(|| "multi-root search was cancelled".to_string())?;
                if snapshot.entries.is_empty() {
                    return Err("multi-root search returned no matches".to_string());
                }
                Ok(WorkAmount::items(total).with_details(json!({
                    "matched_entries": snapshot.entries.len(),
                    "truncated": snapshot.truncated,
                })))
            },
        );
    }

    record_gap(
        results,
        "Column search",
        "windows_runtime",
        "GPUI search-field feedback and frame-stall measurement",
    );
    record_gap(
        results,
        "Column search",
        "macos_runtime",
        "SwiftUI search-field feedback and frame-stall measurement",
    );
    Ok(())
}

fn measure_name_index(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    let Some((tree_size, tree)) = fixtures.trees.iter().next_back() else {
        return Ok(());
    };
    let tree_size = *tree_size;
    let roots = vec![tree.clone()];
    let index_root = fixtures.root.join("indexes").join("name");
    fs::create_dir_all(&index_root)
        .map_err(|error| format!("cannot create name index fixture root: {error}"))?;

    measure_scenario(
        results,
        "Global name search",
        &format!("name_index_cold_build_{tree_size}"),
        "core",
        config.cold_samples,
        0,
        vec![
            "Each sample creates a new Tantivy name index for the same fixed tree.".to_string(),
            "Build time includes traversal, metadata reads, document writes, and commit."
                .to_string(),
        ],
        |iteration| {
            let index_dir = index_root.join(format!("cold-{iteration}"));
            let service = NameIndexService::open_for_performance(index_dir.clone())?;
            service.ensure_roots(&roots);
            wait_for_name_index(&service, tree, tree_size as u64, NAME_INDEX_TIMEOUT)?;
            let status = service
                .status(std::slice::from_ref(tree))
                .into_iter()
                .next()
                .ok_or_else(|| "name index status is unavailable".to_string())?;
            let indexed_entries = status.indexed_entries;
            let index_bytes = directory_size(&index_dir)?;
            drop(service);
            Ok(
                WorkAmount::items(indexed_entries.max(1)).with_details(json!({
                    "expected_files": tree_size,
                    "indexed_entries": indexed_entries,
                    "index_bytes": index_bytes,
                    "provider": status.provider.as_str(),
                    "coverage": status.coverage.as_str(),
                })),
            )
        },
    );

    let warm_index = index_root.join("warm");
    let service = NameIndexService::open_for_performance(warm_index.clone())?;
    service.ensure_roots(&roots);
    wait_for_name_index(&service, tree, tree_size as u64, NAME_INDEX_TIMEOUT)?;
    let status = service
        .status(std::slice::from_ref(tree))
        .into_iter()
        .next()
        .ok_or_else(|| "warm name index status is unavailable".to_string())?;
    let indexed_entries = status.indexed_entries;
    let provider = status.provider.as_str();
    let coverage = status.coverage.as_str();
    let index_bytes = directory_size(&warm_index)?;
    measure_scenario(
        results,
        "Global name search",
        &format!("name_index_warm_query_{tree_size}"),
        "core",
        config.samples,
        1,
        vec![
            "Measures indexed candidate lookup, metadata validation, ranking, and result cap."
                .to_string(),
            "The query does not traverse the selected root.".to_string(),
        ],
        |_| {
            let cancel = AtomicBool::new(false);
            let snapshot = service
                .search(&roots, "needle", &cancel)?
                .ok_or_else(|| "indexed name search was cancelled".to_string())?;
            if snapshot.entries.is_empty() {
                return Err("indexed name search returned no matches".to_string());
            }
            Ok(WorkAmount::items(tree_size as u64).with_details(json!({
                "matched_entries": snapshot.entries.len(),
                "truncated": snapshot.truncated,
                "partial": snapshot.partial,
                "indexed_entries": indexed_entries,
                "index_bytes": index_bytes,
                "provider": provider,
                "coverage": coverage,
            })))
        },
    );

    measure_scenario(
        results,
        "Global name search",
        "name_index_incremental_update_to_query",
        "core",
        config.cold_samples,
        0,
        vec![
            "Writes a uniquely named file, waits for the index watcher, then queries the name."
                .to_string(),
            "The result is an end-to-end freshness latency inside the Core boundary.".to_string(),
        ],
        |iteration| {
            let token = format!("nameindexzebra{iteration:04}");
            let path = tree.join(format!("{token}.txt"));
            fs::write(&path, b"fixture")
                .map_err(|error| format!("cannot write name index fixture: {error}"))?;
            let deadline = Instant::now() + INCREMENTAL_INDEX_TIMEOUT;
            loop {
                let cancel = AtomicBool::new(false);
                let snapshot = service
                    .search(&roots, &token, &cancel)?
                    .ok_or_else(|| "incremental name search was cancelled".to_string())?;
                if snapshot.entries.iter().any(|entry| entry.path == path) {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(format!("incremental name was not found: {token}"));
                }
                thread::sleep(Duration::from_millis(25));
            }
            Ok(WorkAmount::items(1).with_details(json!({
                "query": token,
                "indexed_entries": indexed_entries,
                "index_bytes": index_bytes,
            })))
        },
    );

    Ok(())
}

fn wait_for_name_index(
    service: &NameIndexService,
    root: &Path,
    expected: u64,
    timeout: Duration,
) -> Result<(), String> {
    let roots = [root.to_path_buf()];
    let deadline = Instant::now() + timeout;
    loop {
        let status = service
            .status(&roots)
            .into_iter()
            .next()
            .ok_or_else(|| "name index status is unavailable".to_string())?;
        if status.state == NameIndexState::Ready
            && status.usable
            && status.indexed_entries >= expected
        {
            return Ok(());
        }
        if status.state == NameIndexState::Error {
            return Err(status
                .message
                .unwrap_or_else(|| "name index entered error state".to_string()));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "name index timeout: state={}, indexed_entries={}, expected={expected}",
                status.state.as_str(),
                status.indexed_entries
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn measure_content_search(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    let roots = vec![fixtures.content_dir.clone()];
    let expected = config.content_files as u64;
    let indexes_root = fixtures.root.join("indexes");
    fs::create_dir_all(&indexes_root)
        .map_err(|error| format!("cannot create index fixture root: {error}"))?;

    measure_scenario(
        results,
        "Global content search",
        "cold_content_index_build",
        "core",
        config.cold_samples,
        0,
        vec![
            "Each sample creates a new isolated Tantivy index.".to_string(),
            "Build time includes index creation, traversal, tokenization, and writer commit."
                .to_string(),
        ],
        |iteration| {
            let index_dir = indexes_root.join(format!("cold-{iteration}"));
            let service = ContentSearchService::open_for_performance(index_dir.clone())?;
            service.ensure_roots(&roots)?;
            wait_for_content_index(&service, expected, CONTENT_INDEX_TIMEOUT)?;
            let index_bytes = directory_size(&index_dir)?;
            drop(service);
            Ok(WorkAmount::items(expected).with_details(json!({
                "index_bytes": index_bytes,
                "expected_documents": expected,
            })))
        },
    );

    let warm_index = indexes_root.join("warm");
    let service = ContentSearchService::open_for_performance(warm_index.clone())?;
    service.ensure_roots(&roots)?;
    wait_for_content_index(&service, expected, CONTENT_INDEX_TIMEOUT)?;
    let index_bytes = directory_size(&warm_index)?;
    measure_scenario(
        results,
        "Global content search",
        "warm_content_query",
        "core",
        config.samples,
        1,
        vec![
            "Measures indexed candidate lookup plus exact line verification.".to_string(),
            "The query returns at most 50 files.".to_string(),
        ],
        |_| {
            let snapshot = service
                .search(&roots, "baseline_needle", false, CONTENT_SEARCH_MAX_FILES)?
                .ok_or_else(|| "content search was cancelled".to_string())?;
            if snapshot.files.is_empty() {
                return Err("content search returned no files".to_string());
            }
            Ok(
                WorkAmount::items(snapshot.files.len() as u64).with_details(json!({
                    "documents_scanned": snapshot.scanned_documents,
                    "total_matches": snapshot.total_matches,
                    "partial": snapshot.partial,
                    "truncated": snapshot.truncated,
                    "index_bytes": index_bytes,
                })),
            )
        },
    );

    measure_scenario(
        results,
        "Global content search",
        "incremental_update_to_query",
        "core",
        config.cold_samples,
        0,
        vec![
            "Writes a unique token, waits for watcher/index refresh, then queries the token."
                .to_string(),
            "The result is an end-to-end freshness latency inside the Core boundary.".to_string(),
        ],
        |iteration| {
            let path = fixtures
                .content_dir
                .join(format!("incremental-{iteration:04}.txt"));
            let token = format!("zebrafruit{iteration:04}");
            fs::write(&path, format!("baseline content\n{token}\n"))
                .map_err(|error| format!("cannot write incremental fixture: {error}"))?;
            let deadline = Instant::now() + INCREMENTAL_INDEX_TIMEOUT;
            loop {
                let snapshot = service
                    .search(&roots, &token, false, CONTENT_SEARCH_MAX_FILES)?
                    .ok_or_else(|| "incremental search was cancelled".to_string())?;
                let expected_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if snapshot.files.iter().any(|file| file.name == expected_name) {
                    break;
                }
                if Instant::now() >= deadline {
                    return Err(format!("incremental token was not found: {token}"));
                }
                thread::sleep(Duration::from_millis(25));
            }
            Ok(WorkAmount::items(1).with_details(json!({
                "query": token,
                "index_bytes": index_bytes,
            })))
        },
    );

    record_gap(
        results,
        "Global content search",
        "windows_runtime",
        "GPUI result-card rendering and frame-stall measurement",
    );
    record_gap(
        results,
        "Global content search",
        "macos_runtime",
        "SwiftUI result-card rendering and frame-stall measurement",
    );
    Ok(())
}

fn wait_for_content_index(
    service: &ContentSearchService,
    expected: u64,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let status = service.status();
        if status.state == ContentIndexState::Ready && status.indexed_files >= expected {
            return Ok(());
        }
        if status.state == ContentIndexState::Error {
            return Err(status
                .message
                .unwrap_or_else(|| "content index entered error state".to_string()));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "content index timeout: state={}, indexed_files={}, expected={expected}",
                status.state.as_str(),
                status.indexed_files
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn measure_preview_and_editor(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    for (mib, path) in &fixtures.text_files {
        let path = path.clone();
        let mib = *mib;
        let expected_rejection = mib > 8;
        measure_scenario(
            results,
            "Preview and editor",
            &format!("text_open_{mib}mib"),
            "core",
            config.cold_samples.max(1),
            1,
            vec![
                "Measures Core file read, UTF-8 decoding, language detection, and SHA-256."
                    .to_string(),
                if expected_rejection {
                    "The editor deliberately rejects files larger than 8 MiB.".to_string()
                } else {
                    "Native preview and editor rendering are excluded.".to_string()
                },
            ],
            |_| {
                let result = TextDocument::open(&path);
                if expected_rejection {
                    if result.is_ok() {
                        return Err(
                            "expected the text editor to reject the 100 MiB file".to_string()
                        );
                    }
                    Ok(WorkAmount::items(1).with_details(json!({
                        "expected_result": "rejected",
                        "declared_file_bytes": mib * 1024 * 1024,
                    })))
                } else {
                    let document = result?;
                    Ok(WorkAmount::bytes(document.byte_len).with_details(json!({
                        "language": document.language,
                        "line_ending": document.line_ending.as_str(),
                    })))
                }
            },
        );
    }

    record_gap(
        results,
        "Preview and editor",
        "windows_runtime",
        "GPUI image, Markdown, code, and log rendering plus scroll-stall measurement",
    );
    record_gap(
        results,
        "Preview and editor",
        "macos_runtime",
        "Quick Look, SwiftUI preview, NSTextView, and scroll-stall measurement",
    );
    Ok(())
}

fn measure_file_operations(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    let create_root = fixtures.root.join("file-operations/create");
    fs::create_dir_all(&create_root)
        .map_err(|error| format!("cannot create file-operation fixture: {error}"))?;
    measure_scenario(
        results,
        "File operations",
        "create_100_files",
        "core",
        config.samples,
        0,
        vec!["Measures Core create-file calls, including filesystem metadata updates.".to_string()],
        |iteration| {
            let directory = create_root.join(format!("sample-{iteration:04}"));
            fs::create_dir_all(&directory)
                .map_err(|error| format!("cannot create sample directory: {error}"))?;
            for index in 0..100 {
                kite_fs::create_file(&directory, &format!("created-{index:03}.txt"))?;
            }
            Ok(WorkAmount::items(100))
        },
    );

    let rename_source = fixtures.root.join("file-operations/rename");
    fs::create_dir_all(&rename_source)
        .map_err(|error| format!("cannot create rename fixture: {error}"))?;
    let mut rename_files = Vec::new();
    for index in 0..config.samples {
        let path = rename_source.join(format!("rename-{index:04}.txt"));
        fs::write(&path, b"rename")
            .map_err(|error| format!("cannot create rename file: {error}"))?;
        rename_files.push(path);
    }
    measure_scenario(
        results,
        "File operations",
        "rename_file",
        "core",
        config.samples,
        0,
        vec!["Measures one Core rename operation.".to_string()],
        |iteration| {
            let source = rename_files
                .get(iteration)
                .ok_or_else(|| "missing rename fixture".to_string())?;
            let name = format!("renamed-{iteration:04}.txt");
            kite_fs::rename_entry(source, &name)?;
            Ok(WorkAmount::items(1))
        },
    );

    let copy_source = fixtures.copy_root.join("copy-source");
    let copy_items = config.copy_files as u64;
    measure_scenario(
        results,
        "File operations",
        "copy_small_file_tree",
        "core",
        config.samples,
        1,
        vec![
            "Copies the full fixture tree through the Core transfer implementation.".to_string(),
            "Destination cleanup occurs after measurement.".to_string(),
        ],
        |iteration| {
            let destination = fixtures
                .copy_root
                .join("copy-destinations")
                .join(format!("sample-{iteration:04}"));
            fs::create_dir_all(&destination)
                .map_err(|error| format!("cannot create copy destination: {error}"))?;
            kite_fs::transfer_entry(&copy_source, &destination, false)?;
            Ok(WorkAmount::items(copy_items))
        },
    );

    let move_sources = fixtures.copy_root.join("move-sources");
    measure_scenario(
        results,
        "File operations",
        "move_small_file_tree",
        "core",
        config.cold_samples,
        0,
        vec![
            "Moves a precreated 100-file tree through the Core transfer implementation."
                .to_string(),
        ],
        |iteration| {
            let source = move_sources.join(format!("source-{iteration}"));
            let destination = fixtures
                .copy_root
                .join("move-destinations")
                .join(format!("sample-{iteration:04}"));
            fs::create_dir_all(&destination)
                .map_err(|error| format!("cannot create move destination: {error}"))?;
            kite_fs::transfer_entry(&source, &destination, true)?;
            Ok(WorkAmount::items(100))
        },
    );

    let large_bytes = config.large_file_mib * 1024 * 1024;
    let large_source = fixtures.large_file.clone();
    measure_scenario(
        results,
        "File operations",
        "copy_large_file",
        "core",
        config.cold_samples,
        1,
        vec!["Measures one large-file copy through the Core transfer implementation.".to_string()],
        |iteration| {
            let destination = fixtures
                .root
                .join("large/copies")
                .join(format!("sample-{iteration:04}"));
            fs::create_dir_all(&destination)
                .map_err(|error| format!("cannot create large-copy destination: {error}"))?;
            kite_fs::transfer_entry(&large_source, &destination, false)?;
            Ok(WorkAmount::bytes(large_bytes))
        },
    );

    record_gap(
        results,
        "File operations",
        "windows_runtime",
        "GPUI progress feedback, recycle-bin action, and Explorer clipboard integration",
    );
    record_gap(
        results,
        "File operations",
        "macos_runtime",
        "SwiftUI progress feedback, Finder clipboard integration, and trash action",
    );
    Ok(())
}

fn measure_archive_operations(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    let source = fixtures.archive_root.join("source");
    let prepared = fixtures.root.join("archive/prepared");
    fs::create_dir_all(&prepared)
        .map_err(|error| format!("cannot create prepared archive directory: {error}"))?;
    let mut prepared_archives = BTreeMap::<String, Vec<PathBuf>>::new();

    for format in [ArchiveFormat::Zip, ArchiveFormat::TarGz] {
        let format_name = format.extension().replace('.', "_");
        let mut archives = Vec::new();
        for iteration in 0..config.cold_samples {
            let copy_source = source.clone();
            let archive = archive::compress(&copy_source, format)?;
            let target = prepared.join(format!(
                "prepared-{format_name}-{iteration:04}.{}",
                format.extension()
            ));
            fs::rename(&archive, &target)
                .map_err(|error| format!("cannot move prepared archive: {error}"))?;
            archives.push(target);
        }
        prepared_archives.insert(format_name, archives);
    }

    for format in [ArchiveFormat::Zip, ArchiveFormat::TarGz] {
        let format_name = format.extension().replace('.', "_");
        let source = source.clone();
        measure_scenario(
            results,
            "Archive operations",
            &format!("compress_{format_name}"),
            "core",
            config.cold_samples,
            1,
            vec![
                "Compresses the deterministic text tree with the Core archive writer.".to_string(),
            ],
            |_| {
                archive::compress(&source, format)?;
                Ok(WorkAmount::items(config.archive_files as u64))
            },
        );
    }

    for format in [ArchiveFormat::Zip, ArchiveFormat::TarGz] {
        let format_name = format.extension().replace('.', "_");
        let archives = prepared_archives
            .get(&format_name)
            .ok_or_else(|| format!("missing prepared {format_name} archives"))?;
        let destination_root = fixtures
            .root
            .join(format!("archive/extracted-{format_name}"));
        fs::create_dir_all(&destination_root)
            .map_err(|error| format!("cannot create extraction root: {error}"))?;
        measure_scenario(
            results,
            "Archive operations",
            &format!("extract_{format_name}"),
            "core",
            config.cold_samples,
            0,
            vec!["Extracts a prepared archive with the Core archive reader.".to_string()],
            |iteration| {
                let archive_path = archives
                    .get(iteration)
                    .ok_or_else(|| "missing prepared archive".to_string())?;
                let destination = destination_root.join(format!("sample-{iteration:04}"));
                archive::extract(archive_path, &destination)?;
                Ok(WorkAmount::items(config.archive_files as u64))
            },
        );
    }

    let large_source = fixtures.large_file.clone();
    let large_bytes = config.large_file_mib * 1024 * 1024;
    measure_scenario(
        results,
        "Archive operations",
        "compress_large_file_zip",
        "core",
        config.cold_samples.min(5),
        0,
        vec!["Compresses the large binary fixture with ZIP.".to_string()],
        |_| {
            archive::compress(&large_source, ArchiveFormat::Zip)?;
            Ok(WorkAmount::bytes(large_bytes))
        },
    );

    record_gap(
        results,
        "Archive operations",
        "windows_runtime",
        "GPUI archive progress and frame-stall measurement",
    );
    record_gap(
        results,
        "Archive operations",
        "macos_runtime",
        "SwiftUI archive progress and frame-stall measurement",
    );
    Ok(())
}

fn measure_vcs(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    let Some(vcs_dir) = &fixtures.vcs_dir else {
        record_gap(
            results,
            "VCS and context menus",
            "core",
            "Git status benchmark requires the git executable",
        );
        return Ok(());
    };

    measure_scenario(
        results,
        "VCS and context menus",
        "git_clean_status",
        "core",
        config.cold_samples,
        1,
        vec!["Measures git and SVN discovery plus Git porcelain status.".to_string()],
        |_| {
            let snapshot = vcs::load_vcs_snapshot(vcs_dir);
            if !snapshot.git_available() {
                return Err("git status was not available".to_string());
            }
            Ok(
                WorkAmount::items(config.vcs_files as u64).with_details(json!({
                    "git_statuses": snapshot.git_status_count(),
                    "svn_statuses": snapshot.svn_status_count(),
                    "svn_available": snapshot.svn_available(),
                })),
            )
        },
    );

    for index in 0..100 {
        let path = vcs_dir.join(format!("file-{index:06}.txt"));
        fs::write(&path, b"dirty").map_err(|error| format!("cannot dirty VCS fixture: {error}"))?;
    }

    measure_scenario(
        results,
        "VCS and context menus",
        "git_dirty_status_100_files",
        "core",
        config.cold_samples,
        1,
        vec!["Measures Git status after 100 tracked files change.".to_string()],
        |_| {
            let snapshot = vcs::load_vcs_snapshot(vcs_dir);
            if snapshot.git_status_count() < 100 {
                return Err(format!(
                    "expected at least 100 dirty entries, received {}",
                    snapshot.git_status_count()
                ));
            }
            Ok(
                WorkAmount::items(config.vcs_files as u64).with_details(json!({
                    "git_statuses": snapshot.git_status_count(),
                })),
            )
        },
    );

    let entries = kite_fs::load_directory(vcs_dir, kite_fs::SortSpec::default())?.entries;
    let snapshot = vcs::load_vcs_snapshot(vcs_dir);
    measure_scenario(
        results,
        "VCS and context menus",
        "apply_vcs_statuses",
        "core",
        config.samples,
        1,
        vec!["Measures status indexing and assignment to loaded directory entries.".to_string()],
        |_| {
            let mut values = entries.clone();
            vcs::apply_statuses(&mut values, vcs_dir, &snapshot);
            Ok(WorkAmount::items(values.len() as u64))
        },
    );

    record_gap(
        results,
        "VCS and context menus",
        "windows_runtime",
        "Windows native Shell context-menu and third-party extension latency",
    );
    record_gap(
        results,
        "VCS and context menus",
        "macos_runtime",
        "AppKit menu latency and SVN fixture when svn is unavailable",
    );
    Ok(())
}

fn measure_devices(config: &ProfileConfig, results: &mut Vec<MetricResult>) -> Result<(), String> {
    measure_scenario(
        results,
        "Sidebar and devices",
        "volume_enumeration",
        "core",
        config.samples,
        1,
        vec![
            "Measures Core volume and portable-device enumeration.".to_string(),
            "External devices are reported only when present in the environment.".to_string(),
        ],
        |_| {
            let snapshot = devices::snapshot();
            let items = (snapshot.volumes.len() + snapshot.portable_devices.len()) as u64;
            Ok(WorkAmount::items(items).with_details(json!({
                "volumes": snapshot.volumes.len(),
                "portable_devices": snapshot.portable_devices.len(),
                "volume_kinds": snapshot.volumes.iter().map(|volume| format!("{:?}", volume.kind)).collect::<Vec<_>>(),
            })))
        },
    );

    record_gap(
        results,
        "Sidebar and devices",
        "windows_runtime",
        "WPD/MTP first listing, navigation, and copy throughput with a controlled device",
    );
    record_gap(
        results,
        "Sidebar and devices",
        "macos_runtime",
        "USB volume first listing and copy throughput with a controlled device",
    );
    Ok(())
}

fn measure_update_path(
    fixtures: &FixtureSet,
    config: &ProfileConfig,
    include_network: bool,
    results: &mut Vec<MetricResult>,
) -> Result<(), String> {
    let large_bytes = config.large_file_mib * 1024 * 1024;
    let large_file = fixtures.large_file.clone();
    measure_scenario(
        results,
        "Update path",
        "sha256_verify_large_file",
        "core",
        config.cold_samples.min(5),
        1,
        vec!["Measures the same streaming SHA-256 implementation used by the updater.".to_string()],
        |_| {
            updater::sha256_file_for_performance(&large_file)?;
            Ok(WorkAmount::bytes(large_bytes))
        },
    );

    if include_network {
        measure_scenario(
            results,
            "Update path",
            "github_release_check",
            "external",
            config.cold_samples.min(3),
            0,
            vec![
                "Measures the live GitHub Release API call.".to_string(),
                "Network conditions are not controlled.".to_string(),
            ],
            |_| {
                updater::check_for_update()?;
                Ok(WorkAmount::items(1))
            },
        );
    } else {
        record_gap(
            results,
            "Update path",
            "external",
            "GitHub Release API, download throughput, installer, and restart with --include-network",
        );
    }
    Ok(())
}

fn add_known_gaps(fixtures: &FixtureSet, _config: &ProfileConfig, results: &mut Vec<MetricResult>) {
    record_gap(
        results,
        "Startup and launch",
        "windows_runtime",
        "release EXE cold/warm start, CLI/default-handler launch, and first paint",
    );
    record_gap(
        results,
        "Startup and launch",
        "macos_runtime",
        "release app cold/warm start, Finder/default-handler launch, and first paint",
    );
    if !fixtures.git_available {
        record_gap(
            results,
            "VCS and context menus",
            "core",
            "Git was not available in the benchmark environment",
        );
    }
    if !fixtures.svn_available {
        record_gap(
            results,
            "VCS and context menus",
            "core",
            "SVN was not available in the benchmark environment",
        );
    }
}

fn build_coverage(results: &[MetricResult]) -> Vec<CoverageEntry> {
    let mut entries = Vec::new();
    for area in AREAS {
        let area_results = results.iter().filter(|result| result.area == area);
        let automated = area_results
            .clone()
            .filter(|result| result.status == "measured")
            .map(|result| result.scenario.clone())
            .collect::<Vec<_>>();
        let gaps = area_results
            .filter(|result| result.status == "not_automated" || result.status == "error")
            .map(|result| result.scenario.clone())
            .collect::<Vec<_>>();
        let status = if !automated.is_empty() && gaps.is_empty() {
            "measured"
        } else if !automated.is_empty() {
            "partial"
        } else {
            "not_automated"
        };
        entries.push(CoverageEntry {
            area: area.to_string(),
            status: status.to_string(),
            automated_scenarios: automated,
            gaps,
        });
    }
    entries
}

fn build_summary(results: &[MetricResult]) -> ReportSummary {
    let measured_scenarios = results
        .iter()
        .filter(|result| result.status == "measured")
        .count();
    let not_automated_scenarios = results
        .iter()
        .filter(|result| result.status == "not_automated")
        .count();
    let errors = results
        .iter()
        .filter(|result| result.status == "error")
        .count();
    let mut slowest = results
        .iter()
        .filter_map(|result| {
            result.summary.as_ref().map(|summary| ScenarioRank {
                area: result.area.clone(),
                scenario: result.scenario.clone(),
                p95_ms: summary.p95_ms,
                sample_count: summary.sample_count,
            })
        })
        .collect::<Vec<_>>();
    slowest.sort_by(|left, right| right.p95_ms.total_cmp(&left.p95_ms));
    slowest.truncate(12);

    let mut highest_variance = results
        .iter()
        .filter_map(|result| {
            let summary = result.summary.as_ref()?;
            if summary.sample_count < 3 || summary.p50_ms <= f64::EPSILON {
                return None;
            }
            Some(ScenarioVarianceRank {
                area: result.area.clone(),
                scenario: result.scenario.clone(),
                p95_to_p50_ratio: summary.p95_ms / summary.p50_ms,
                sample_count: summary.sample_count,
            })
        })
        .collect::<Vec<_>>();
    highest_variance
        .sort_by(|left, right| right.p95_to_p50_ratio.total_cmp(&left.p95_to_p50_ratio));
    highest_variance.truncate(8);

    let mut highest_memory = results
        .iter()
        .filter_map(|result| {
            let summary = result.summary.as_ref()?;
            Some(ScenarioMemoryRank {
                area: result.area.clone(),
                scenario: result.scenario.clone(),
                peak_rss_delta_bytes: summary.peak_rss_delta_bytes_max,
                sample_count: summary.sample_count,
            })
        })
        .collect::<Vec<_>>();
    highest_memory.sort_by_key(|scenario| std::cmp::Reverse(scenario.peak_rss_delta_bytes));
    highest_memory.truncate(8);

    ReportSummary {
        measured_scenarios,
        not_automated_scenarios,
        errors,
        slowest_scenarios: slowest,
        highest_variance_scenarios: highest_variance,
        highest_memory_scenarios: highest_memory,
    }
}

fn process_snapshot() -> ProcessSnapshot {
    let pid = Pid::from_u32(std::process::id());
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        false,
        ProcessRefreshKind::nothing()
            .with_memory()
            .with_cpu()
            .with_disk_usage(),
    );
    system
        .process(pid)
        .map(|process| ProcessSnapshot {
            rss_bytes: process.memory(),
            cpu_ms: process.accumulated_cpu_time(),
            read_bytes: process.disk_usage().total_read_bytes,
            write_bytes: process.disk_usage().total_written_bytes,
        })
        .unwrap_or(ProcessSnapshot {
            rss_bytes: 0,
            cpu_ms: 0,
            read_bytes: 0,
            write_bytes: 0,
        })
}

#[derive(Clone, Copy, Debug, Default)]
struct ProcessSnapshot {
    rss_bytes: u64,
    cpu_ms: u64,
    read_bytes: u64,
    write_bytes: u64,
}

struct RssSampler {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RssSampler {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(0));
        let thread_stop = Arc::clone(&stop);
        let thread_peak = Arc::clone(&peak);
        let handle = thread::Builder::new()
            .name("kite-perf-rss".to_string())
            .spawn(move || {
                let pid = Pid::from_u32(std::process::id());
                let mut system = System::new();
                loop {
                    system.refresh_processes_specifics(
                        ProcessesToUpdate::Some(&[pid]),
                        false,
                        ProcessRefreshKind::nothing().with_memory(),
                    );
                    if let Some(process) = system.process(pid) {
                        thread_peak.fetch_max(process.memory(), Ordering::Relaxed);
                    }
                    if thread_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    thread::sleep(RSS_SAMPLE_INTERVAL);
                }
            })
            .ok();
        Self { stop, peak, handle }
    }

    fn stop(self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle {
            let _ = handle.join();
        }
        self.peak.load(Ordering::Relaxed)
    }
}

fn directory_size(path: &Path) -> Result<u64, String> {
    let mut total = 0_u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("cannot read directory size: {error}")),
        };
        for entry in entries {
            let entry = entry.map_err(|error| format!("cannot read directory entry: {error}"))?;
            let metadata = entry
                .metadata()
                .map_err(|error| format!("cannot read directory metadata: {error}"))?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Ok(total)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn git_commit() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn unique_fixture_root() -> PathBuf {
    env::temp_dir().join(format!(
        "kite-perf-{}-{}",
        std::process::id(),
        now_unix_ms()
    ))
}

fn default_output_path(profile: &Profile) -> PathBuf {
    PathBuf::from("target").join("performance").join(format!(
        "kite-perf-{}-{}-{}.json",
        profile.name(),
        env::consts::OS,
        now_unix_ms()
    ))
}

fn write_json(report: &PerformanceReport, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create JSON output directory: {error}"))?;
    }
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| format!("cannot serialize performance report: {error}"))?;
    fs::write(path, bytes).map_err(|error| format!("cannot write performance report: {error}"))
}

fn write_markdown(
    report: &PerformanceReport,
    path: &Path,
    raw_report_path: &Path,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create Markdown output directory: {error}"))?;
    }
    let mut output = String::new();
    output.push_str(&format!(
        "# Kite {} performance baseline\n\n",
        report.kite_version
    ));
    output.push_str(&format!(
        "- Commit: `{}`\n- Profile: `{}`\n- Build: `{}`\n- Generated: `{}` UTC\n\n",
        report.git_commit,
        report.profile,
        report.build_profile,
        format_unix_ms(report.generated_at_unix_ms)
    ));
    output.push_str(&format!(
        "- Raw samples: [{}]({})\n\n",
        raw_report_path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default(),
        raw_report_link(path, raw_report_path)
    ));
    output.push_str("## Environment\n\n");
    output.push_str("| Field | Value |\n| --- | --- |\n");
    output.push_str(&format!(
        "| OS | {} |\n",
        report
            .host
            .os_version
            .clone()
            .unwrap_or_else(|| report.host.os.clone())
    ));
    output.push_str(&format!(
        "| Architecture | {} |\n",
        report.host.architecture
    ));
    output.push_str(&format!("| CPU | {} |\n", report.host.cpu_model));
    output.push_str(&format!(
        "| Logical cores | {} |\n",
        report.host.logical_cores
    ));
    output.push_str(&format!(
        "| Memory | {} |\n",
        format_bytes(report.host.memory_bytes)
    ));
    output.push_str(&format!(
        "| Filesystem | {} |\n",
        report.host.fixture_filesystem
    ));
    output.push_str(&format!(
        "| Disk kind | {} |\n",
        report.host.fixture_disk_kind
    ));
    output.push_str(&format!(
        "| Disk free at start | {} |\n\n",
        format_bytes(report.host.fixture_disk_available_bytes)
    ));

    output.push_str("## Coverage\n\n");
    output.push_str("| Area | Status | Automated scenarios | Gaps |\n");
    output.push_str("| --- | --- | --- | --- |\n");
    for coverage in &report.coverage {
        output.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            escape_markdown(&coverage.area),
            coverage.status,
            if coverage.automated_scenarios.is_empty() {
                "None".to_string()
            } else {
                escape_markdown(&coverage.automated_scenarios.join("<br>"))
            },
            if coverage.gaps.is_empty() {
                "None".to_string()
            } else {
                escape_markdown(&coverage.gaps.join("<br>"))
            }
        ));
    }
    output.push('\n');

    output.push_str("## Results\n\n");
    output.push_str("| Area | Scenario | Boundary | n | p50 ms | p95 ms | max ms | Throughput p50 | Peak RSS delta |\n");
    output.push_str("| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for result in &report.results {
        if let Some(summary) = &result.summary {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {:.3} | {:.3} | {:.3} | {} | {} |\n",
                escape_markdown(&result.area),
                escape_markdown(&result.scenario),
                result.boundary,
                summary.sample_count,
                summary.p50_ms,
                summary.p95_ms,
                summary.max_ms,
                format_optional_throughput(summary),
                format_bytes(summary.peak_rss_delta_bytes_max)
            ));
        } else {
            output.push_str(&format!(
                "| {} | {} | {} | 0 | - | - | - | - | {} |\n",
                escape_markdown(&result.area),
                escape_markdown(&result.scenario),
                result.boundary,
                result.status
            ));
        }
    }
    output.push('\n');

    output.push_str("## Slowest scenarios\n\n");
    if report.summary.slowest_scenarios.is_empty() {
        output.push_str("No measured scenarios.\n");
    } else {
        output.push_str("| Rank | Area | Scenario | p95 ms | n |\n");
        output.push_str("| ---: | --- | --- | ---: | ---: |\n");
        for (index, scenario) in report.summary.slowest_scenarios.iter().enumerate() {
            output.push_str(&format!(
                "| {} | {} | {} | {:.3} | {} |\n",
                index + 1,
                escape_markdown(&scenario.area),
                escape_markdown(&scenario.scenario),
                scenario.p95_ms,
                scenario.sample_count
            ));
        }
    }
    output.push('\n');

    output.push_str("## Findings\n\n");
    if let Some(scenario) = report.summary.slowest_scenarios.first() {
        output.push_str(&format!(
            "- Largest measured p95: `{} / {}` at {:.3} ms.\n",
            escape_markdown(&scenario.area),
            escape_markdown(&scenario.scenario),
            scenario.p95_ms
        ));
    }
    if let Some(scenario) = report.summary.highest_variance_scenarios.first() {
        output.push_str(&format!(
            "- Highest timing variance: `{} / {}` with p95/p50 ratio {:.2}.\n",
            escape_markdown(&scenario.area),
            escape_markdown(&scenario.scenario),
            scenario.p95_to_p50_ratio
        ));
    }
    if let Some(scenario) = report.summary.highest_memory_scenarios.first() {
        output.push_str(&format!(
            "- Largest measured RSS delta: `{} / {}` at {}.\n",
            escape_markdown(&scenario.area),
            escape_markdown(&scenario.scenario),
            format_bytes(scenario.peak_rss_delta_bytes)
        ));
    }
    let partial_areas = report
        .coverage
        .iter()
        .filter(|coverage| coverage.status != "measured")
        .map(|coverage| coverage.area.as_str())
        .collect::<Vec<_>>();
    if !partial_areas.is_empty() {
        output.push_str(&format!(
            "- Native or environment coverage remains incomplete for: {}.\n",
            escape_markdown(&partial_areas.join(", "))
        ));
    }
    output.push('\n');

    output.push_str("## Highest variance\n\n");
    output.push_str("| Rank | Area | Scenario | p95/p50 | n |\n");
    output.push_str("| ---: | --- | --- | ---: | ---: |\n");
    for (index, scenario) in report.summary.highest_variance_scenarios.iter().enumerate() {
        output.push_str(&format!(
            "| {} | {} | {} | {:.2} | {} |\n",
            index + 1,
            escape_markdown(&scenario.area),
            escape_markdown(&scenario.scenario),
            scenario.p95_to_p50_ratio,
            scenario.sample_count
        ));
    }
    if report.summary.highest_variance_scenarios.is_empty() {
        output.push_str("| - | - | No scenarios with at least 3 samples | - | - |\n");
    }
    output.push('\n');

    output.push_str("## Highest RSS delta\n\n");
    output.push_str("| Rank | Area | Scenario | Peak RSS delta | n |\n");
    output.push_str("| ---: | --- | --- | ---: | ---: |\n");
    for (index, scenario) in report.summary.highest_memory_scenarios.iter().enumerate() {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            index + 1,
            escape_markdown(&scenario.area),
            escape_markdown(&scenario.scenario),
            format_bytes(scenario.peak_rss_delta_bytes),
            scenario.sample_count
        ));
    }
    if report.summary.highest_memory_scenarios.is_empty() {
        output.push_str("| - | - | No measured scenarios | - | - |\n");
    }
    output.push('\n');

    output.push_str("## Method\n\n");
    output.push_str(&format!("- Timer: {}\n", report.method.timer));
    output.push_str(&format!("- Percentile: {}\n", report.method.percentile));
    output.push_str(&format!("- RSS: {}\n", report.method.rss));
    output.push_str(&format!(
        "- Cache control: {}\n",
        report.method.cache_control
    ));
    output.push_str(&format!(
        "- Sample counts: {} interactive, {} cold/build\n",
        report.method.interactive_samples, report.method.cold_samples
    ));
    fs::write(path, output).map_err(|error| format!("cannot write Markdown report: {error}"))
}

fn raw_report_link(markdown_path: &Path, raw_report_path: &Path) -> String {
    let markdown_directory = markdown_path.parent().unwrap_or_else(|| Path::new("."));
    raw_report_path
        .strip_prefix(markdown_directory)
        .map(Path::to_path_buf)
        .or_else(|_| raw_report_path.file_name().map(PathBuf::from).ok_or(()))
        .unwrap_or_else(|_| raw_report_path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

fn format_optional_throughput(summary: &MetricSummary) -> String {
    match (summary.throughput_p50, summary.throughput_unit.as_deref()) {
        (Some(value), Some(unit)) => format!("{value:.2} {unit}"),
        _ => "-".to_string(),
    }
}

fn format_bytes(bytes: u64) -> String {
    let mib = bytes as f64 / 1024.0 / 1024.0;
    if mib >= 1024.0 {
        format!("{:.2} GiB", mib / 1024.0)
    } else {
        format!("{mib:.2} MiB")
    }
}

fn format_unix_ms(value: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(value as i64)
        .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_else(|| value.to_string())
}

fn escape_markdown(value: &str) -> String {
    value.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_uses_linear_interpolation() {
        let values = [1.0, 2.0, 3.0, 4.0];
        assert!((percentile(&values, 0.5) - 2.5).abs() < 1e-9);
        assert!((percentile(&values, 0.95) - 3.85).abs() < 1e-9);
    }

    #[test]
    fn profile_names_round_trip() {
        for profile in [
            Profile::Smoke,
            Profile::Ci,
            Profile::Standard,
            Profile::Extended,
        ] {
            assert_eq!(Profile::parse(profile.name()).unwrap(), profile);
        }
    }

    #[test]
    fn raw_report_link_is_relative_to_markdown_directory() {
        let link = raw_report_link(
            Path::new("docs/performance/baseline.md"),
            Path::new("docs/performance/results/report.json"),
        );
        assert_eq!(link, "results/report.json");
    }

    #[test]
    fn coverage_distinguishes_partial_rows() {
        let results = vec![
            MetricResult {
                area: "Startup and launch".to_string(),
                scenario: "core".to_string(),
                boundary: "core".to_string(),
                status: "measured".to_string(),
                unit: "ms".to_string(),
                samples: Vec::new(),
                summary: None,
                details: None,
                notes: Vec::new(),
            },
            MetricResult {
                area: "Startup and launch".to_string(),
                scenario: "native".to_string(),
                boundary: "windows_runtime".to_string(),
                status: "not_automated".to_string(),
                unit: "none".to_string(),
                samples: Vec::new(),
                summary: None,
                details: None,
                notes: Vec::new(),
            },
        ];
        let coverage = build_coverage(&results);
        assert_eq!(coverage[0].status, "partial");
    }
}
