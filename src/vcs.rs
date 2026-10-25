use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use crate::fs::{FileEntry, VcsStatus, worse_status};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug)]
struct VcsPathStatus {
    path: PathBuf,
    status: VcsStatus,
}

#[derive(Clone, Debug, Default)]
pub struct VcsSnapshot {
    git: Vec<VcsPathStatus>,
    svn: Vec<VcsPathStatus>,
    git_available: bool,
    svn_available: bool,
}

impl VcsSnapshot {
    pub fn git_available(&self) -> bool {
        self.git_available
    }

    pub fn svn_available(&self) -> bool {
        self.svn_available
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VcsTool {
    Git,
    Svn,
}

impl VcsTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Git => "Git",
            Self::Svn => "SVN",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VcsOperation {
    Add,
    Unstage,
    Commit(String),
    Pull,
    Push,
    Fetch,
    Revert,
    Clean,
    Update,
    Lock,
    Unlock,
    Cleanup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VcsQuery {
    Diff,
    Log,
}

impl VcsQuery {
    pub fn label(self) -> &'static str {
        match self {
            Self::Diff => "差异",
            Self::Log => "日志",
        }
    }
}

pub fn load_vcs_snapshot(view_root: &Path) -> VcsSnapshot {
    let (git_available, git) = git_statuses(view_root);
    let (svn_available, svn) = svn_statuses(view_root);
    VcsSnapshot {
        git,
        svn,
        git_available,
        svn_available,
    }
}

pub fn apply_statuses(entries: &mut [FileEntry], view_root: &Path, snapshot: &VcsSnapshot) {
    let (git_exact, git_directories) = index_statuses(&snapshot.git, view_root);
    let (svn_exact, svn_directories) = index_statuses(&snapshot.svn, view_root);

    for entry in entries {
        let key = path_key(&entry.path);
        let git_aggregate = entry
            .is_dir()
            .then(|| git_directories.get(&key).copied())
            .flatten();
        let svn_aggregate = entry
            .is_dir()
            .then(|| svn_directories.get(&key).copied())
            .flatten();
        entry.git_status = worse_status(git_exact.get(&key).copied(), git_aggregate);
        entry.svn_status = worse_status(svn_exact.get(&key).copied(), svn_aggregate);
    }
}

fn index_statuses(
    statuses: &[VcsPathStatus],
    view_root: &Path,
) -> (HashMap<String, VcsStatus>, HashMap<String, VcsStatus>) {
    let mut exact = HashMap::new();
    let mut directories = HashMap::new();

    for item in statuses {
        let Ok(relative) = item.path.strip_prefix(view_root) else {
            continue;
        };
        let mut components = relative.components();
        let Some(first) = components.next() else {
            continue;
        };
        if !matches!(first, Component::Normal(_)) {
            continue;
        }

        merge_status(&mut exact, path_key(&item.path), item.status);

        if components.next().is_some() {
            let directory = view_root.join(first.as_os_str());
            merge_status(&mut directories, path_key(&directory), item.status);
        }
    }

    (exact, directories)
}

fn merge_status(map: &mut HashMap<String, VcsStatus>, key: String, status: VcsStatus) {
    let merged = worse_status(map.get(&key).copied(), Some(status));
    if let Some(merged) = merged {
        map.insert(key, merged);
    }
}

fn git_statuses(view_root: &Path) -> (bool, Vec<VcsPathStatus>) {
    let Some(root_output) = run_command("git", view_root, &["rev-parse", "--show-toplevel"]) else {
        return (false, Vec::new());
    };
    if !root_output.status.success() {
        return (false, Vec::new());
    }

    let root_text = String::from_utf8_lossy(&root_output.stdout)
        .trim()
        .to_string();
    if root_text.is_empty() {
        return (false, Vec::new());
    }
    let root = dunce::canonicalize(&root_text).unwrap_or_else(|_| PathBuf::from(root_text));

    let Some(status_output) = run_command(
        "git",
        &root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignored=matching",
            "--",
            ".",
        ],
    ) else {
        return (true, Vec::new());
    };
    if !status_output.status.success() {
        return (true, Vec::new());
    }

    (true, parse_git_status(&status_output.stdout, &root))
}

fn svn_statuses(view_root: &Path) -> (bool, Vec<VcsPathStatus>) {
    let Some(info_output) = run_command(
        "svn",
        view_root,
        &["info", "--show-item", "wc-root", "--non-interactive"],
    ) else {
        return (false, Vec::new());
    };
    if !info_output.status.success()
        || String::from_utf8_lossy(&info_output.stdout)
            .trim()
            .is_empty()
    {
        return (false, Vec::new());
    }

    let Some(output) = run_command(
        "svn",
        view_root,
        &["status", "--non-interactive", "--ignore-externals", "."],
    ) else {
        return (true, Vec::new());
    };
    if !output.status.success() {
        return (true, Vec::new());
    }
    (true, parse_svn_status(&output.stdout, view_root))
}

fn parse_git_status(output: &[u8], root: &Path) -> Vec<VcsPathStatus> {
    let mut statuses = Vec::new();
    let mut records = output.split(|byte| *byte == 0);

    while let Some(record) = records.next() {
        if record.len() < 4 || record.get(2) != Some(&b' ') {
            continue;
        }
        let code = &record[..2];
        let path = String::from_utf8_lossy(&record[3..]).into_owned();
        if code.contains(&b'R') || code.contains(&b'C') {
            let _ = records.next();
        }
        if let Some(status) = git_status_from_code(code) {
            statuses.push(VcsPathStatus {
                path: root.join(path),
                status,
            });
        }
    }

    statuses
}

fn parse_svn_status(output: &[u8], view_root: &Path) -> Vec<VcsPathStatus> {
    let mut statuses = Vec::new();
    let text = String::from_utf8_lossy(output);

    for line in text.lines() {
        if line.len() < 8 {
            continue;
        }
        let item_code = line.as_bytes()[0] as char;
        let property_code = line.as_bytes()[1] as char;
        let path = line[8..].trim();
        if path.is_empty() {
            continue;
        }
        if let Some(status) = svn_status_from_codes(item_code, property_code) {
            statuses.push(VcsPathStatus {
                path: resolve_path(view_root, path),
                status,
            });
        }
    }

    statuses
}

fn git_status_from_code(code: &[u8]) -> Option<VcsStatus> {
    if code.contains(&b'?') {
        Some(VcsStatus::Untracked)
    } else if code.contains(&b'!') {
        Some(VcsStatus::Ignored)
    } else if code.contains(&b'U') {
        Some(VcsStatus::Conflicted)
    } else if code.contains(&b'R') {
        Some(VcsStatus::Renamed)
    } else if code.contains(&b'D') {
        Some(VcsStatus::Deleted)
    } else if code.contains(&b'A') || code.contains(&b'C') {
        Some(VcsStatus::Added)
    } else if code.contains(&b'M') || code.contains(&b'T') {
        Some(VcsStatus::Modified)
    } else {
        None
    }
}

fn svn_status_from_codes(item_code: char, property_code: char) -> Option<VcsStatus> {
    match item_code {
        'M' => Some(VcsStatus::Modified),
        'A' => Some(VcsStatus::Added),
        'D' => Some(VcsStatus::Deleted),
        'R' => Some(VcsStatus::Renamed),
        'C' => Some(VcsStatus::Conflicted),
        '?' => Some(VcsStatus::Untracked),
        '!' => Some(VcsStatus::Missing),
        'I' => Some(VcsStatus::Ignored),
        'X' => Some(VcsStatus::External),
        '~' => Some(VcsStatus::Missing),
        ' ' if property_code == 'M' => Some(VcsStatus::PropertyModified),
        _ if property_code == 'M' => Some(VcsStatus::PropertyModified),
        _ => None,
    }
}

fn resolve_path(base: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn path_key(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(target_os = "windows")]
    {
        value.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(target_os = "windows"))]
    {
        value.into_owned()
    }
}

pub fn run_vcs_operation(
    tool: VcsTool,
    operation: VcsOperation,
    target: &Path,
) -> Result<String, String> {
    match tool {
        VcsTool::Git => run_git_operation(operation, target),
        VcsTool::Svn => run_svn_operation(operation, target),
    }
}

pub fn run_vcs_query(tool: VcsTool, query: VcsQuery, target: &Path) -> Result<String, String> {
    match tool {
        VcsTool::Git => {
            let (root, target) = git_context(target)?;
            let relative = if target.as_os_str().is_empty() {
                Path::new(".")
            } else {
                target.as_path()
            };

            match query {
                VcsQuery::Diff => {
                    let mut command = configured_command("git", &root);
                    command.args(["diff", "--no-ext-diff", "--"]).arg(relative);
                    let working = run_query_command("Git 差异", command)?;
                    if working != "没有可显示内容。" {
                        return Ok(working);
                    }
                    let mut command = configured_command("git", &root);
                    command
                        .args(["diff", "--cached", "--no-ext-diff", "--"])
                        .arg(relative);
                    run_query_command("Git 暂存差异", command)
                }
                VcsQuery::Log => {
                    let mut command = configured_command("git", &root);
                    command
                        .args([
                            "log",
                            "-n",
                            "100",
                            "--date=short",
                            "--pretty=format:%h  %ad  %an  %s",
                            "--",
                        ])
                        .arg(relative);
                    run_query_command("Git 日志", command)
                }
            }
        }
        VcsTool::Svn => {
            let start = working_directory(target);
            let mut command = configured_command("svn", start);
            match query {
                VcsQuery::Diff => {
                    command.arg("diff").arg(target);
                    run_query_command("SVN 差异", command)
                }
                VcsQuery::Log => {
                    command.args(["log", "--limit", "100"]).arg(target);
                    run_query_command("SVN 日志", command)
                }
            }
        }
    }
}

fn git_context(target: &Path) -> Result<(PathBuf, PathBuf), String> {
    let start = working_directory(target);
    let root_output = run_command("git", start, &["rev-parse", "--show-toplevel"])
        .ok_or_else(|| "未找到 Git。请确认 Git 已安装并位于 PATH 中。".to_string())?;
    if !root_output.status.success() {
        return Err(output_error("Git", &root_output));
    }
    let root_text = String::from_utf8_lossy(&root_output.stdout)
        .trim()
        .to_string();
    let root = dunce::canonicalize(&root_text).unwrap_or_else(|_| PathBuf::from(root_text));
    let target = dunce::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let relative = target
        .strip_prefix(&root)
        .map_err(|_| "所选项目不在 Git 工作区中".to_string())?
        .to_path_buf();
    Ok((root, relative))
}

fn run_query_command(label: &str, mut command: Command) -> Result<String, String> {
    let output = command.output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("未找到 {label} 所需命令。请确认命令已安装并位于 PATH 中。")
        } else {
            format!("{label}读取失败：{error}")
        }
    })?;
    if output.status.success() {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if value.is_empty() {
            Ok("没有可显示内容。".to_string())
        } else {
            Ok(value)
        }
    } else {
        Err(output_error(label, &output))
    }
}

fn run_git_operation(operation: VcsOperation, target: &Path) -> Result<String, String> {
    if matches!(
        &operation,
        VcsOperation::Update | VcsOperation::Lock | VcsOperation::Unlock | VcsOperation::Cleanup
    ) {
        return Err("Git 不支持更新操作".to_string());
    }

    let start = working_directory(target);
    let root_output = run_command("git", start, &["rev-parse", "--show-toplevel"])
        .ok_or_else(|| "未找到 Git。请确认 Git 已安装并位于 PATH 中。".to_string())?;
    if !root_output.status.success() {
        return Err(output_error("Git", &root_output));
    }
    let root_text = String::from_utf8_lossy(&root_output.stdout)
        .trim()
        .to_string();
    let root = dunce::canonicalize(&root_text).unwrap_or_else(|_| PathBuf::from(root_text));
    let target = dunce::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let relative = target
        .strip_prefix(&root)
        .map_err(|_| "所选项目不在 Git 工作区中".to_string())?;
    let relative = if relative.as_os_str().is_empty() {
        Path::new(".")
    } else {
        relative
    };
    let mut command = configured_command("git", &root);

    let label = match operation {
        VcsOperation::Add => {
            command.args(["add", "--"]).arg(relative);
            "Git 添加"
        }
        VcsOperation::Unstage => {
            command.args(["restore", "--staged", "--"]).arg(relative);
            "Git 取消暂存"
        }
        VcsOperation::Commit(message) => {
            command
                .args(["commit", "-m"])
                .arg(message)
                .arg("--")
                .arg(relative);
            "Git 提交"
        }
        VcsOperation::Pull => {
            command.args(["pull", "--no-edit"]);
            "Git 拉取"
        }
        VcsOperation::Push => {
            command.arg("push");
            "Git 推送"
        }
        VcsOperation::Fetch => {
            command.args(["fetch", "--prune"]);
            "Git 获取"
        }
        VcsOperation::Revert => {
            command
                .args(["restore", "--source=HEAD", "--staged", "--worktree", "--"])
                .arg(relative);
            "Git 还原"
        }
        VcsOperation::Clean => {
            command.args(["clean", "-fd", "--"]).arg(relative);
            "Git 清理"
        }
        VcsOperation::Update
        | VcsOperation::Lock
        | VcsOperation::Unlock
        | VcsOperation::Cleanup => unreachable!(),
    };
    run_operation_command(label, command)
}

fn run_svn_operation(operation: VcsOperation, target: &Path) -> Result<String, String> {
    if matches!(
        &operation,
        VcsOperation::Unstage
            | VcsOperation::Pull
            | VcsOperation::Push
            | VcsOperation::Fetch
            | VcsOperation::Clean
    ) {
        return Err("SVN 不支持此操作".to_string());
    }
    let start = working_directory(target);
    let mut command = configured_command("svn", start);

    let label = match operation {
        VcsOperation::Add => {
            command.args(["add", "--force"]).arg(target);
            "SVN 添加"
        }
        VcsOperation::Commit(message) => {
            command
                .args(["commit", "--non-interactive", "-m"])
                .arg(message)
                .arg(target);
            "SVN 提交"
        }
        VcsOperation::Revert => {
            command.args(["revert", "--depth=infinity"]).arg(target);
            "SVN 还原"
        }
        VcsOperation::Update => {
            command.args(["update", "--non-interactive"]).arg(target);
            "SVN 更新"
        }
        VcsOperation::Lock => {
            command.args(["lock", "--non-interactive"]).arg(target);
            "SVN 锁定"
        }
        VcsOperation::Unlock => {
            command.args(["unlock", "--non-interactive"]).arg(target);
            "SVN 解锁"
        }
        VcsOperation::Cleanup => {
            command.arg("cleanup").arg(target);
            "SVN 清理"
        }
        VcsOperation::Unstage
        | VcsOperation::Pull
        | VcsOperation::Push
        | VcsOperation::Fetch
        | VcsOperation::Clean => unreachable!(),
    };
    run_operation_command(label, command)
}

fn working_directory(target: &Path) -> &Path {
    if target.is_dir() {
        target
    } else {
        target.parent().unwrap_or(target)
    }
}

fn configured_command(program: &str, current_dir: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(current_dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never");
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

fn run_operation_command(label: &str, mut command: Command) -> Result<String, String> {
    let output = command.output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            format!("未找到 {}。请确认命令已安装并位于 PATH 中。", label)
        } else {
            format!("{label}失败：{error}")
        }
    })?;
    if output.status.success() {
        Ok(format!("{label}完成"))
    } else {
        Err(output_error(label, &output))
    }
}

fn output_error(label: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    let detail = detail
        .replace(['\r', '\n'], " ")
        .chars()
        .take(500)
        .collect::<String>();
    if detail.is_empty() {
        format!(
            "{label}操作失败，退出码 {}",
            output.status.code().unwrap_or(-1)
        )
    } else {
        format!("{label}操作失败：{detail}")
    }
}

fn run_command(program: &str, current_dir: &Path, args: &[&str]) -> Option<Output> {
    let mut command = configured_command(program, current_dir);
    command.args(args);
    command.output().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::EntryKind;
    use std::time::SystemTime;

    fn entry(name: &str, kind: EntryKind) -> FileEntry {
        FileEntry {
            path: PathBuf::from(name),
            name: name.to_string(),
            kind,
            size: 0,
            modified: None,
            extension: String::new(),
            git_status: None,
            svn_status: None,
        }
    }

    #[test]
    fn parses_git_porcelain_statuses() {
        let root = Path::new("C:\\repo");
        let output =
            b"M  changed.rs\0?? new.txt\0!! ignored.log\0UU conflict.rs\0R  renamed.rs\0old.rs\0";
        let statuses = parse_git_status(output, root);

        assert_eq!(statuses.len(), 5);
        assert_eq!(statuses[0].status, VcsStatus::Modified);
        assert_eq!(statuses[1].status, VcsStatus::Untracked);
        assert_eq!(statuses[2].status, VcsStatus::Ignored);
        assert_eq!(statuses[3].status, VcsStatus::Conflicted);
        assert_eq!(statuses[4].status, VcsStatus::Renamed);
        assert!(statuses[0].path.ends_with("changed.rs"));
    }

    #[test]
    fn parses_svn_statuses_and_property_changes() {
        let root = Path::new("C:\\repo");
        let output = b" M      prop.txt\nM       changed.txt\n?       new.txt\nC       conflict.txt\nA       added.txt\n";
        let statuses = parse_svn_status(output, root);

        assert_eq!(statuses.len(), 5);
        assert_eq!(statuses[0].status, VcsStatus::PropertyModified);
        assert_eq!(statuses[1].status, VcsStatus::Modified);
        assert_eq!(statuses[2].status, VcsStatus::Untracked);
        assert_eq!(statuses[3].status, VcsStatus::Conflicted);
        assert_eq!(statuses[4].status, VcsStatus::Added);
    }

    #[test]
    fn does_not_report_svn_for_a_regular_directory() {
        if !command_available("svn", &["--version", "--quiet"]) {
            return;
        }
        let root = unique_temp_dir("not-svn");
        std::fs::create_dir_all(&root).expect("create regular directory");
        let snapshot = load_vcs_snapshot(&root);
        assert!(!snapshot.svn_available());
        std::fs::remove_dir_all(root).expect("clean regular directory");
    }

    #[test]
    fn aggregates_nested_git_changes_on_parent_directory() {
        let root = PathBuf::from("C:\\repo");
        let snapshot = VcsSnapshot {
            git: vec![
                VcsPathStatus {
                    path: root.join("src").join("nested").join("file.rs"),
                    status: VcsStatus::Modified,
                },
                VcsPathStatus {
                    path: root.join("readme.md"),
                    status: VcsStatus::Untracked,
                },
            ],
            svn: Vec::new(),
            git_available: true,
            svn_available: false,
        };
        let mut entries = vec![
            entry("C:\\repo\\src", EntryKind::Directory),
            entry("C:\\repo\\readme.md", EntryKind::File),
            entry("C:\\repo\\clean.md", EntryKind::File),
        ];

        apply_statuses(&mut entries, &root, &snapshot);

        assert_eq!(entries[0].git_status, Some(VcsStatus::Modified));
        assert_eq!(entries[1].git_status, Some(VcsStatus::Untracked));
        assert_eq!(entries[2].git_status, None);
    }

    fn command_available(program: &str, args: &[&str]) -> bool {
        Command::new(program)
            .args(args)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn command_ok(program: &str, current_dir: &Path, args: &[&str]) {
        let status = Command::new(program)
            .current_dir(current_dir)
            .args(args)
            .status()
            .expect("spawn command");
        assert!(status.success(), "{program} {args:?} failed");
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kite-vcs-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn detects_git_status_in_a_working_tree() {
        if !command_available("git", &["--version"]) {
            return;
        }
        let root = unique_temp_dir("git");
        std::fs::create_dir_all(&root).expect("create git test root");
        command_ok("git", &root, &["init", "--quiet"]);
        command_ok(
            "git",
            &root,
            &["config", "user.email", "kite@example.invalid"],
        );
        command_ok("git", &root, &["config", "user.name", "Kite Test"]);
        std::fs::write(root.join("tracked.txt"), b"before").expect("write tracked file");
        std::fs::create_dir_all(root.join("src")).expect("create git subdirectory");
        std::fs::write(root.join("src").join("nested.txt"), b"before").expect("write nested file");
        command_ok("git", &root, &["add", "tracked.txt", "src/nested.txt"]);
        command_ok("git", &root, &["commit", "--quiet", "-m", "initial"]);
        std::fs::write(root.join("tracked.txt"), b"after").expect("modify tracked file");
        std::fs::write(root.join("src").join("nested.txt"), b"after").expect("modify nested file");
        std::fs::write(root.join("new.txt"), b"new").expect("write untracked file");

        let nested_root = root.join("src");
        let nested_snapshot = load_vcs_snapshot(&nested_root);
        let expected_nested = nested_root.join("nested.txt");
        assert!(nested_snapshot.git.iter().any(|item| {
            path_key(&item.path) == path_key(&expected_nested) && item.status == VcsStatus::Modified
        }));

        let snapshot = load_vcs_snapshot(&root);
        assert!(snapshot.git.iter().any(|item| {
            item.path.ends_with("tracked.txt") && item.status == VcsStatus::Modified
        }));
        assert!(
            snapshot.git.iter().any(|item| {
                item.path.ends_with("new.txt") && item.status == VcsStatus::Untracked
            })
        );
        std::fs::remove_dir_all(root).expect("clean git test root");
    }

    #[test]
    fn runs_git_add_commit_and_revert_operations() {
        if !command_available("git", &["--version"]) {
            return;
        }
        let root = unique_temp_dir("git-operations");
        std::fs::create_dir_all(&root).expect("create git operations root");
        command_ok("git", &root, &["init", "--quiet"]);
        command_ok(
            "git",
            &root,
            &["config", "user.email", "kite@example.invalid"],
        );
        command_ok("git", &root, &["config", "user.name", "Kite Test"]);
        let file = root.join("note.txt");
        std::fs::write(&file, b"before").expect("write note");

        assert!(
            run_vcs_operation(VcsTool::Git, VcsOperation::Add, &file)
                .expect("git add")
                .contains("完成")
        );
        assert!(
            run_vcs_operation(
                VcsTool::Git,
                VcsOperation::Commit("initial".to_string()),
                &file,
            )
            .expect("git commit")
            .contains("完成")
        );
        std::fs::write(&file, b"after").expect("modify note");
        let diff = run_vcs_query(VcsTool::Git, VcsQuery::Diff, &file).expect("git diff");
        assert!(diff.contains("+after"));
        let log = run_vcs_query(VcsTool::Git, VcsQuery::Log, &file).expect("git log");
        assert!(log.contains("initial"));
        run_vcs_operation(VcsTool::Git, VcsOperation::Add, &file).expect("git stage change");
        assert!(
            run_vcs_operation(VcsTool::Git, VcsOperation::Unstage, &file)
                .expect("git unstage")
                .contains("完成")
        );
        assert!(
            run_vcs_operation(VcsTool::Git, VcsOperation::Revert, &file)
                .expect("git revert")
                .contains("完成")
        );
        assert_eq!(std::fs::read(&file).expect("read note"), b"before");

        let untracked = root.join("junk.tmp");
        std::fs::write(&untracked, b"junk").expect("write untracked file");
        run_vcs_operation(VcsTool::Git, VcsOperation::Clean, &untracked).expect("git clean");
        assert!(!untracked.exists());
        std::fs::remove_dir_all(root).expect("clean git operations root");
    }

    #[test]
    fn detects_svn_status_in_a_working_copy() {
        if !command_available("svn", &["--version", "--quiet"])
            || !command_available("svnadmin", &["--version", "--quiet"])
        {
            return;
        }
        let base = unique_temp_dir("svn");
        let repository = base.join("repository");
        let working_copy = base.join("working-copy");
        std::fs::create_dir_all(&base).expect("create svn test base");
        command_ok(
            "svnadmin",
            &base,
            &["create", repository.to_string_lossy().as_ref()],
        );
        let url = format!(
            "file:///{}",
            repository.to_string_lossy().replace('\\', "/")
        );
        command_ok(
            "svn",
            &base,
            &[
                "checkout",
                "--quiet",
                &url,
                working_copy.to_string_lossy().as_ref(),
            ],
        );
        std::fs::write(working_copy.join("tracked.txt"), b"before").expect("write svn file");
        std::fs::create_dir_all(working_copy.join("src")).expect("create svn subdirectory");
        std::fs::write(working_copy.join("src").join("nested.txt"), b"before")
            .expect("write svn nested file");
        command_ok("svn", &working_copy, &["add", "--quiet", "tracked.txt"]);
        command_ok(
            "svn",
            &working_copy,
            &["add", "--parents", "--quiet", "src/nested.txt"],
        );
        command_ok(
            "svn",
            &working_copy,
            &["commit", "--quiet", "-m", "initial"],
        );
        std::fs::write(working_copy.join("tracked.txt"), b"after").expect("modify svn file");
        std::fs::write(working_copy.join("src").join("nested.txt"), b"after")
            .expect("modify svn nested file");
        std::fs::write(working_copy.join("new.txt"), b"new").expect("write svn untracked file");

        let diff = run_vcs_query(VcsTool::Svn, VcsQuery::Diff, &working_copy).expect("svn diff");
        assert!(diff.contains("+after"));
        run_vcs_query(VcsTool::Svn, VcsQuery::Log, &working_copy).expect("svn log");

        let nested_root = working_copy.join("src");
        let nested_snapshot = load_vcs_snapshot(&nested_root);
        let expected_nested = nested_root.join("nested.txt");
        assert!(nested_snapshot.svn.iter().any(|item| {
            path_key(&item.path) == path_key(&expected_nested) && item.status == VcsStatus::Modified
        }));

        let snapshot = load_vcs_snapshot(&working_copy);
        assert!(snapshot.svn.iter().any(|item| {
            item.path.ends_with("tracked.txt") && item.status == VcsStatus::Modified
        }));
        assert!(
            snapshot.svn.iter().any(|item| {
                item.path.ends_with("new.txt") && item.status == VcsStatus::Untracked
            })
        );
        let new_file = working_copy.join("new.txt");
        assert!(
            run_vcs_operation(VcsTool::Svn, VcsOperation::Add, &new_file)
                .expect("svn add")
                .contains("完成")
        );
        assert!(
            run_vcs_operation(VcsTool::Svn, VcsOperation::Revert, &new_file)
                .expect("svn revert")
                .contains("完成")
        );
        assert!(
            run_vcs_operation(VcsTool::Svn, VcsOperation::Cleanup, &working_copy)
                .expect("svn cleanup")
                .contains("完成")
        );
        std::fs::remove_dir_all(base).expect("clean svn test base");
    }
}
