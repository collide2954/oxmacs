use std::thread;
use std::time::Duration;

use iced::futures::channel::oneshot;

const TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!("oxmacs/", env!("CARGO_PKG_VERSION"));

pub async fn get(url: String) -> Result<String, String> {
    let (sender, receiver) = oneshot::channel();
    thread::spawn(move || sender.send(blocking_get(&url)));
    receiver
        .await
        .unwrap_or_else(|_| Err("the request was dropped".to_owned()))
}

fn blocking_get(url: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .user_agent(USER_AGENT)
        .build()
        .into();
    agent
        .get(url)
        .call()
        .map_err(|error| error.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_addresses_fail_without_a_connection() {
        assert!(blocking_get("not a url").is_err());
    }
}
