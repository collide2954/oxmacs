use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use iced::futures::channel::oneshot;
use oxmacs_core::external::managed::{self, Install};

const TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const DOWNLOAD_LIMIT: u64 = 256 * 1024 * 1024;
const USER_AGENT: &str = concat!("oxmacs/", env!("CARGO_PKG_VERSION"));

pub async fn get(url: String) -> Result<String, String> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || sender.send(blocking_get(&url)));
    receiver
        .await
        .unwrap_or_else(|_| Err("the request was dropped".to_owned()))
}

pub async fn install(install: Install) -> Result<PathBuf, String> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || sender.send(blocking_install(&install)));
    receiver
        .await
        .unwrap_or_else(|_| Err("the download was dropped".to_owned()))
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .user_agent(USER_AGENT)
        .build()
        .into()
}

fn blocking_get(url: &str) -> Result<String, String> {
    agent(TIMEOUT)
        .get(url)
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    agent(DOWNLOAD_TIMEOUT)
        .get(url)
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .with_config()
        .limit(DOWNLOAD_LIMIT)
        .read_to_vec()
        .map_err(|error| error.to_string())
}

fn blocking_install(install: &Install) -> Result<PathBuf, String> {
    let checksum = blocking_get(&install.checksum.url)?;
    let archive = download(&install.archive.url)?;
    managed::install(
        install.tool,
        &install.version,
        &install.archive.name,
        &archive,
        &checksum,
        &install.root,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_addresses_fail_without_a_connection() {
        assert!(blocking_get("not a url").is_err());
        assert!(download("not a url").is_err());
    }

    #[test]
    #[ignore = "downloads air from GitHub; set OXMACS_TOOLS_ROOT to keep it"]
    fn air_installs_from_its_latest_release() {
        let air = managed::AIR;
        let release = managed::parse_release(&blocking_get(&air.latest_url()).unwrap()).unwrap();
        let name = air.archive_here().unwrap();
        let kept = std::env::var_os("OXMACS_TOOLS_ROOT").map(PathBuf::from);
        let scratch = tempfile::tempdir().unwrap();
        let root = kept.unwrap_or_else(|| scratch.path().to_owned());

        let installed = blocking_install(&Install {
            tool: air,
            version: release.version.clone(),
            archive: release.asset(&name).unwrap().clone(),
            checksum: release.asset(&format!("{name}.sha256")).unwrap().clone(),
            root: root.clone(),
        })
        .unwrap();

        assert_eq!(
            air.installed(&root),
            Some((release.version.clone(), installed.clone()))
        );
        let version = oxmacs_core::external::tools::version(&installed).unwrap();
        assert!(version.contains(&release.version), "{version}");
        let formatted =
            oxmacs_core::external::tools::air_format(&installed, "x<-c(1,2)", &root.join("a.R"));
        assert_eq!(formatted, Ok("x <- c(1, 2)\n".to_owned()));
    }
}
