use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub html_url: String,
    pub body: String,
    pub assets: Vec<ReleaseAsset>,
    pub release_date: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AvailableUpdate {
    pub current_version: String,
    pub target_version: String,
    pub release_url: String,
    pub asset_name: String,
    pub download_url: String,
    pub release_notes: String,
    pub release_date: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    UpToDate {
        current_version: String,
        release_date: Option<String>,
    },
    Available(AvailableUpdate),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking {
        manual: bool,
    },
    Available {
        version: String,
    },
    Installing {
        version: String,
        manual: bool,
    },
    Updated {
        version: String,
    },
    UpToDate,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateMessage {
    CheckFinished {
        manual: bool,
        result: Result<CheckOutcome, String>,
    },
    InstallFinished {
        manual: bool,
        version: String,
        result: Result<(), String>,
    },
}

#[derive(serde::Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(serde::Deserialize)]
struct GitHubReleaseResponse {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    assets: Vec<GitHubReleaseAsset>,
    published_at: Option<String>,
}

pub fn release_api_url() -> Result<String, String> {
    let repo = repository_slug()?;
    Ok(format!(
        "https://api.github.com/repos/{repo}/releases/latest"
    ))
}

pub fn evaluate_release_json(json: &str) -> Result<CheckOutcome, String> {
    let release = parse_release_json(json)?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let latest_version = normalize_version(&release.tag_name);

    match compare_versions(&latest_version, &current_version) {
        Ordering::Greater => {
            let target = target_triple()
                .ok_or_else(|| "unsupported platform for self-update".to_string())?;
            let release_url = release.html_url.clone();
            let asset = select_release_asset(&release, target)
                .ok_or_else(|| format!("no release asset found for target {target}"))?;

            Ok(CheckOutcome::Available(AvailableUpdate {
                current_version,
                target_version: latest_version,
                release_url,
                asset_name: asset.name.clone(),
                download_url: asset.download_url.clone(),
                release_notes: normalize_release_notes(&release.body),
                release_date: release.release_date.unwrap_or_default(),
            }))
        }
        _ => Ok(CheckOutcome::UpToDate {
            current_version,
            release_date: release.release_date,
        }),
    }
}

pub fn install_update(update: &AvailableUpdate, current_exe: &Path) -> Result<(), String> {
    let temp_root = create_temp_update_dir()?;
    let archive_path = temp_root.join(&update.asset_name);
    let extract_root = temp_root.join("extract");
    fs::create_dir_all(&extract_root).map_err(|e| e.to_string())?;

    let curl_status = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&archive_path)
        .arg(&update.download_url)
        .status()
        .map_err(|e| format!("failed to start curl: {e}"))?;
    if !curl_status.success() {
        let _ = fs::remove_dir_all(&temp_root);
        return Err(format!("curl download failed with status {curl_status}"));
    }

    if update.asset_name.ends_with(".tar.gz") {
        let tar_status = Command::new("tar")
            .args(["-xzf"])
            .arg(&archive_path)
            .args(["-C"])
            .arg(&extract_root)
            .status()
            .map_err(|e| format!("failed to start tar: {e}"))?;
        if !tar_status.success() {
            let _ = fs::remove_dir_all(&temp_root);
            return Err(format!("tar extraction failed with status {tar_status}"));
        }
    } else {
        let _ = fs::remove_dir_all(&temp_root);
        return Err(format!(
            "unsupported archive format for asset {}",
            update.asset_name
        ));
    }

    let binary_name = if cfg!(windows) {
        "lazyifconfig.exe"
    } else {
        "lazyifconfig"
    };
    let extracted_binary = find_file_named(&extract_root, binary_name)
        .ok_or_else(|| format!("could not find {binary_name} in extracted archive"))?;

    let install_dir = current_exe
        .parent()
        .ok_or_else(|| "failed to locate executable directory".to_string())?;
    let staged_binary = install_dir.join(format!("{binary_name}.update"));

    fs::copy(&extracted_binary, &staged_binary).map_err(|e| e.to_string())?;

    let permissions = current_exe
        .metadata()
        .map_err(|e| e.to_string())?
        .permissions();
    fs::set_permissions(&staged_binary, permissions).map_err(|e| e.to_string())?;

    #[cfg(windows)]
    {
        fs::remove_file(current_exe).map_err(|e| e.to_string())?;
    }

    fs::rename(&staged_binary, current_exe).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&temp_root);
    Ok(())
}

fn repository_slug() -> Result<String, String> {
    let repo = env!("CARGO_PKG_REPOSITORY")
        .trim_end_matches(".git")
        .trim_end_matches('/');
    let marker = "github.com/";
    let idx = repo
        .find(marker)
        .ok_or_else(|| "repository URL is not a GitHub repository".to_string())?;
    Ok(repo[idx + marker.len()..].to_string())
}

fn parse_release_json(json: &str) -> Result<ReleaseInfo, String> {
    let release: GitHubReleaseResponse =
        serde_json::from_str(json).map_err(|e| format!("invalid GitHub release JSON: {e}"))?;
    Ok(ReleaseInfo {
        tag_name: release.tag_name,
        html_url: release.html_url,
        body: release.body.unwrap_or_default(),
        release_date: release.published_at,
        assets: release
            .assets
            .into_iter()
            .map(|asset| ReleaseAsset {
                name: asset.name,
                download_url: asset.browser_download_url,
            })
            .collect(),
    })
}

fn normalize_release_notes(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        "Release notes not provided.".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
fn summarize_release_notes(body: &str) -> String {
    let mut lines = Vec::new();

    for raw_line in normalize_release_notes(body).lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cleaned = trimmed
            .trim_start_matches('#')
            .trim_start_matches('-')
            .trim_start_matches('*')
            .trim_start_matches(' ')
            .trim();

        if cleaned.is_empty() {
            continue;
        }

        lines.push(cleaned.to_string());
        if lines.len() == 2 {
            break;
        }
    }

    if lines.is_empty() {
        "Release notes not provided.".to_string()
    } else {
        lines.join(" | ")
    }
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = parse_version_parts(left);
    let right_parts = parse_version_parts(right);
    let max_len = left_parts.len().max(right_parts.len());

    for idx in 0..max_len {
        let left_part = *left_parts.get(idx).unwrap_or(&0);
        let right_part = *right_parts.get(idx).unwrap_or(&0);
        match left_part.cmp(&right_part) {
            Ordering::Equal => {}
            non_eq => return non_eq,
        }
    }

    Ordering::Equal
}

fn parse_version_parts(version: &str) -> Vec<u64> {
    normalize_version(version)
        .split(['.', '-'])
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn target_triple() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

fn select_release_asset<'a>(release: &'a ReleaseInfo, target: &str) -> Option<&'a ReleaseAsset> {
    let tarball_suffix = format!("-{target}.tar.gz");
    let zip_suffix = format!("-{target}.zip");

    release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(&tarball_suffix))
        .or_else(|| {
            release
                .assets
                .iter()
                .find(|asset| asset.name.ends_with(&zip_suffix))
        })
}

fn create_temp_update_dir() -> Result<PathBuf, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("lazyifconfig-update-{}-{now}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn find_file_named(root: &Path, name: &str) -> Option<PathBuf> {
    if root.file_name().and_then(|value| value.to_str()) == Some(name) {
        return Some(root.to_path_buf());
    }

    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_named(&path, name) {
                return Some(found);
            }
        } else if path.file_name().and_then(|value| value.to_str()) == Some(name) {
            return Some(path);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_release_json_returns_available_update_for_newer_version() {
        let target = target_triple().unwrap();
        let json = format!(
            "{{\
                \"tag_name\":\"v9.9.9\",\
                \"html_url\":\"https://github.com/hunchulchoi/lazyifconfig/releases/tag/v9.9.9\",\
                \"body\":\"## Highlights\\n- Faster scans\\n- Better update UI\",\
                \"published_at\":\"2026-01-01T12:34:56Z\",\
                \"assets\":[{{\
                    \"name\":\"lazyifconfig-v9.9.9-{target}.tar.gz\",\
                    \"browser_download_url\":\"https://example.com/lazyifconfig-v9.9.9-{target}.tar.gz\"\
                }}]\
            }}"
        );

        let result = evaluate_release_json(&json).unwrap();
        let CheckOutcome::Available(update) = result else {
            panic!("expected available update");
        };

        assert_eq!(update.target_version, "9.9.9");
        assert_eq!(
            update.asset_name,
            format!("lazyifconfig-v9.9.9-{target}.tar.gz")
        );
        assert!(update.release_notes.contains("Highlights"));
        assert!(update.release_notes.contains("Faster scans"));
        assert_eq!(update.release_date, "2026-01-01T12:34:56Z");
    }

    #[test]
    fn evaluate_release_json_returns_up_to_date_for_same_version() {
        let current = env!("CARGO_PKG_VERSION");
        let target = target_triple().unwrap();
        let json = format!(
            "{{\
                \"tag_name\":\"v{current}\",\
                \"html_url\":\"https://github.com/hunchulchoi/lazyifconfig/releases/tag/v{current}\",\
                \"body\":\"\",\
                \"published_at\":\"2026-01-01T12:34:56Z\",\
                \"assets\":[{{\
                    \"name\":\"lazyifconfig-v{current}-{target}.tar.gz\",\
                    \"browser_download_url\":\"https://example.com/lazyifconfig-v{current}-{target}.tar.gz\"\
                }}]\
            }}"
        );

        let result = evaluate_release_json(&json).unwrap();
        assert_eq!(
            result,
            CheckOutcome::UpToDate {
                current_version: current.to_string(),
                release_date: Some("2026-01-01T12:34:56Z".to_string())
            }
        );
    }

    #[test]
    fn summarize_release_notes_uses_first_non_empty_lines() {
        let summary = summarize_release_notes(
            "\n## Highlights\n- Faster scans\n- Better update UI\nMore details later",
        );

        assert_eq!(summary, "Highlights | Faster scans");
    }

    #[test]
    fn normalize_release_notes_falls_back_when_empty() {
        assert_eq!(
            normalize_release_notes("   \n"),
            "Release notes not provided."
        );
    }

    #[test]
    fn release_api_url_points_to_latest_release_endpoint() {
        let url = release_api_url().unwrap();
        assert_eq!(
            url,
            "https://api.github.com/repos/choihunchul/lazyifconfig/releases/latest"
        );
    }
}
