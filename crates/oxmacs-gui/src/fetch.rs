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
}
