use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use ring::digest::{SHA256, digest};
use serde::Deserialize;

use crate::external::tools;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Managed {
    pub name: &'static str,
    pub repository: &'static str,
    pub summary: &'static str,
}

pub const AIR: Managed = Managed {
    name: "air",
    repository: "posit-dev/air",
    summary: "R formatter",
};

pub const MANAGED: [Managed; 1] = [AIR];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub assets: Vec<Asset>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.name == name)
    }
}

#[derive(Deserialize)]
struct Latest {
    tag_name: String,
    assets: Vec<Published>,
}

#[derive(Deserialize)]
struct Published {
    name: String,
    browser_download_url: String,
    size: u64,
}

impl Managed {
    pub fn binary(self) -> String {
        if cfg!(windows) {
            format!("{}.exe", self.name)
        } else {
            self.name.to_owned()
        }
    }

    pub fn latest_url(self) -> String {
        format!(
            "https://api.github.com/repos/{}/releases/latest",
            self.repository
        )
    }

    pub fn archive(self, os: &str, arch: &str) -> Option<String> {
        let arch = match arch {
            "x86_64" | "aarch64" => arch,
            _ => return None,
        };
        let (target, extension) = match os {
            "windows" => ("pc-windows-msvc", "zip"),
            "macos" => ("apple-darwin", "tar.gz"),
            "linux" => ("unknown-linux-gnu", "tar.gz"),
            _ => return None,
        };
        Some(format!("{}-{arch}-{target}.{extension}", self.name))
    }

    pub fn archive_here(self) -> Option<String> {
        self.archive(env::consts::OS, env::consts::ARCH)
    }

    pub fn installed(self, root: &Path) -> Option<(String, PathBuf)> {
        fs::read_dir(root.join(self.name))
            .ok()?
            .flatten()
            .filter_map(|entry| {
                let version = entry.file_name().to_str()?.to_owned();
                let binary = entry.path().join(self.binary());
                (plain(&version) && binary.is_file()).then_some((version, binary))
            })
            .max_by(|(a, _), (b, _)| numbers(a).cmp(&numbers(b)))
    }
}

fn plain(version: &str) -> bool {
    !version.is_empty()
        && !version.starts_with('.')
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

fn numbers(version: &str) -> Vec<u64> {
    version
        .split(['.', '-', '+'])
        .map_while(|part| part.parse().ok())
        .collect()
}

pub fn parse_release(json: &str) -> Result<Release, String> {
    let latest: Latest = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let version = latest.tag_name.trim_start_matches('v').to_owned();
    if !plain(&version) {
        return Err(format!("unexpected release name {:?}", latest.tag_name));
    }
    Ok(Release {
        version,
        assets: latest
            .assets
            .into_iter()
            .map(|asset| Asset {
                name: asset.name,
                url: asset.browser_download_url,
                size: asset.size,
            })
            .collect(),
    })
}

pub fn newer(candidate: &str, current: &str) -> bool {
    let (mut candidate, mut current) = (numbers(candidate), numbers(current));
    let length = candidate.len().max(current.len());
    candidate.resize(length, 0);
    current.resize(length, 0);
    candidate > current
}

pub fn install(
    tool: Managed,
    version: &str,
    archive_name: &str,
    archive: &[u8],
    checksum: &str,
    root: &Path,
) -> Result<PathBuf, String> {
    if !plain(version) {
        return Err(format!("unexpected version {version:?}"));
    }
    let expected = checksum
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if hex(archive) != expected {
        return Err(format!(
            "{archive_name} does not match its published checksum"
        ));
    }
    let folder = root.join(tool.name);
    fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let unpacking = tempfile::Builder::new()
        .prefix(".unpacking")
        .tempdir_in(&folder)
        .map_err(|error| error.to_string())?;
    let saved = unpacking.path().join(archive_name);
    fs::write(&saved, archive).map_err(|error| error.to_string())?;
    let contents = unpacking.path().join("contents");
    fs::create_dir_all(&contents).map_err(|error| error.to_string())?;
    unpack(&saved, &contents)?;
    let binary = find_file(&contents, &tool.binary())
        .ok_or_else(|| format!("{archive_name} has no {}", tool.binary()))?;
    let target = folder.join(version);
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&target).map_err(|error| error.to_string())?;
    let installed = target.join(tool.binary());
    fs::rename(&binary, &installed).map_err(|error| error.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(&installed, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
    }
    for entry in fs::read_dir(&folder).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with('.') && name != version && entry.path().is_dir() {
            fs::remove_dir_all(entry.path()).map_err(|error| error.to_string())?;
        }
    }
    Ok(installed)
}

pub fn remove(tool: Managed, root: &Path) -> Result<(), String> {
    let folder = root.join(tool.name);
    if !folder.exists() {
        return Ok(());
    }
    fs::remove_dir_all(folder).map_err(|error| error.to_string())
}

fn hex(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .fold(String::new(), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

fn tar() -> PathBuf {
    if cfg!(windows)
        && let Some(system) = env::var_os("SystemRoot")
    {
        let bundled = PathBuf::from(system).join("System32").join("tar.exe");
        if bundled.is_file() {
            return bundled;
        }
    }
    PathBuf::from("tar")
}

fn unpack(archive: &Path, into: &Path) -> Result<(), String> {
    let mut command = Command::new(tar());
    command.arg("-xf").arg(archive).arg("-C").arg(into);
    tools::hide_console(&mut command);
    let output = command.output().map_err(|error| format!("tar: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let error = String::from_utf8_lossy(&output.stderr);
    Err(format!("tar: {}", error.lines().next().unwrap_or("failed")))
}

fn find_file(directory: &Path, name: &str) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    entries
        .iter()
        .find(|path| path.is_file() && path.file_name().is_some_and(|file| file == name))
        .cloned()
        .or_else(|| {
            entries
                .iter()
                .filter(|path| path.is_dir())
                .find_map(|path| find_file(path, name))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LATEST: &str = r#"{
        "tag_name": "0.12.0",
        "assets": [
            {"name": "air-x86_64-pc-windows-msvc.zip", "browser_download_url": "https://example.com/air.zip", "size": 5242880},
            {"name": "air-x86_64-pc-windows-msvc.zip.sha256", "browser_download_url": "https://example.com/air.zip.sha256", "size": 96}
        ]
    }"#;

    #[test]
    fn releases_give_their_version_and_assets() {
        let release = parse_release(LATEST).unwrap();

        assert_eq!(release.version, "0.12.0");
        assert_eq!(
            release.asset("air-x86_64-pc-windows-msvc.zip"),
            Some(&Asset {
                name: "air-x86_64-pc-windows-msvc.zip".to_owned(),
                url: "https://example.com/air.zip".to_owned(),
                size: 5_242_880,
            })
        );
        assert!(release.asset("air.tar.gz").is_none());
    }

    #[test]
    fn versions_lose_a_leading_v_and_must_be_plain() {
        let tagged =
            |tag: &str| parse_release(&format!(r#"{{"tag_name": "{tag}", "assets": []}}"#));

        assert_eq!(tagged("v1.2.3").unwrap().version, "1.2.3");
        assert!(tagged("../1.2.3").is_err());
        assert!(tagged("").is_err());
        assert!(parse_release("not json").is_err());
    }

    #[test]
    fn each_platform_has_its_archive() {
        assert_eq!(
            AIR.archive("windows", "x86_64").as_deref(),
            Some("air-x86_64-pc-windows-msvc.zip")
        );
        assert_eq!(
            AIR.archive("macos", "aarch64").as_deref(),
            Some("air-aarch64-apple-darwin.tar.gz")
        );
        assert_eq!(
            AIR.archive("linux", "x86_64").as_deref(),
            Some("air-x86_64-unknown-linux-gnu.tar.gz")
        );
        assert_eq!(AIR.archive("freebsd", "x86_64"), None);
        assert_eq!(AIR.archive("linux", "riscv64"), None);
    }

    #[test]
    fn the_latest_release_comes_from_github() {
        assert_eq!(
            AIR.latest_url(),
            "https://api.github.com/repos/posit-dev/air/releases/latest"
        );
    }

    #[test]
    fn versions_compare_number_by_number() {
        assert!(newer("0.10.0", "0.9.2"));
        assert!(newer("1.0", "0.99.9"));
        assert!(newer("0.12.1", "0.12"));
        assert!(!newer("0.12.0", "0.12.0"));
        assert!(!newer("0.9.9", "0.10.0"));
    }

    fn place(root: &Path, version: &str) -> PathBuf {
        let folder = root.join("air").join(version);
        fs::create_dir_all(&folder).unwrap();
        let binary = folder.join(AIR.binary());
        fs::write(&binary, "binary").unwrap();
        binary
    }

    #[test]
    fn the_newest_installed_version_is_used() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.9.0");
        let newest = place(root.path(), "0.12.0");
        fs::create_dir_all(root.path().join("air").join("0.13.0")).unwrap();

        assert_eq!(
            AIR.installed(root.path()),
            Some(("0.12.0".to_owned(), newest))
        );
        assert_eq!(AIR.installed(&root.path().join("missing")), None);
    }

    fn archive_of(binary: &str) -> (tempfile::TempDir, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let inside = dir.path().join("air-x86_64-unknown-linux-gnu");
        fs::create_dir_all(&inside).unwrap();
        fs::write(inside.join(AIR.binary()), binary).unwrap();
        let archive = dir.path().join("air.tar.gz");
        let status = Command::new(tar())
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(dir.path())
            .arg("air-x86_64-unknown-linux-gnu")
            .status()
            .unwrap();
        assert!(status.success());
        let bytes = fs::read(&archive).unwrap();
        (dir, bytes)
    }

    fn checksum_of(bytes: &[u8]) -> String {
        format!("{} *air.tar.gz\n", hex(bytes))
    }

    #[test]
    fn digests_are_lowercase_sha_256() {
        assert_eq!(
            hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn installing_checks_the_digest_unpacks_and_replaces_older_versions() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.11.0");
        let (_dir, bytes) = archive_of("new binary");

        let installed = install(
            AIR,
            "0.12.0",
            "air.tar.gz",
            &bytes,
            &checksum_of(&bytes),
            root.path(),
        )
        .unwrap();

        assert_eq!(
            installed,
            root.path().join("air").join("0.12.0").join(AIR.binary())
        );
        assert_eq!(fs::read_to_string(&installed).unwrap(), "new binary");
        assert!(!root.path().join("air").join("0.11.0").exists());
        assert_eq!(
            AIR.installed(root.path()),
            Some(("0.12.0".to_owned(), installed))
        );
    }

    #[test]
    fn a_wrong_digest_installs_nothing() {
        let root = tempfile::tempdir().unwrap();
        let old = place(root.path(), "0.11.0");
        let (_dir, bytes) = archive_of("new binary");

        let result = install(
            AIR,
            "0.12.0",
            "air.tar.gz",
            &bytes,
            &checksum_of(b"something else"),
            root.path(),
        );

        assert_eq!(
            result,
            Err("air.tar.gz does not match its published checksum".to_owned())
        );
        assert_eq!(AIR.installed(root.path()), Some(("0.11.0".to_owned(), old)));
    }

    #[test]
    fn removing_deletes_every_version() {
        let root = tempfile::tempdir().unwrap();
        place(root.path(), "0.11.0");
        place(root.path(), "0.12.0");

        remove(AIR, root.path()).unwrap();

        assert_eq!(AIR.installed(root.path()), None);
        assert!(!root.path().join("air").exists());
    }
}
