const SERVICE: &str = "oxmacs-irc";

fn entry(server: &str, account: &str) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, &format!("{account}@{server}"))
}

pub fn read(server: &str, account: &str) -> Result<Option<String>, String> {
    match entry(server, account).and_then(|entry| entry.get_password()) {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub fn write(server: &str, account: &str, password: &str) -> Result<(), String> {
    entry(server, account)
        .and_then(|entry| entry.set_password(password))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "stores a throwaway credential in the OS keychain"]
    fn passwords_round_trip_through_the_os_keychain() {
        let server = "oxmacs-test-server";
        write(server, "tester", "hunter2").unwrap();

        assert_eq!(read(server, "tester").unwrap().as_deref(), Some("hunter2"));

        entry(server, "tester")
            .unwrap()
            .delete_credential()
            .unwrap();
        assert_eq!(read(server, "tester").unwrap(), None);
    }
}
