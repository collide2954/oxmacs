use std::process::{Command, Stdio};

use crate::external::tools;

const TOAST: &str = "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null; \
$template = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
$lines = $template.GetElementsByTagName('text'); \
$lines.Item(0).AppendChild($template.CreateTextNode($env:OXMACS_TITLE)) > $null; \
$lines.Item(1).AppendChild($template.CreateTextNode($env:OXMACS_BODY)) > $null; \
$toast = [Windows.UI.Notifications.ToastNotification]::new($template); \
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe').Show($toast)";

const BANNER: &str = "display notification (system attribute \"OXMACS_BODY\") with title (system attribute \"OXMACS_TITLE\")";

pub fn command(os: &str, title: &str, body: &str) -> Option<Command> {
    let mut command = match os {
        "windows" => {
            let mut command = Command::new("powershell");
            command.args(["-NoProfile", "-NonInteractive", "-Command", TOAST]);
            command
        }
        "macos" => {
            let mut command = Command::new("osascript");
            command.args(["-e", BANNER]);
            command
        }
        "linux" | "freebsd" | "openbsd" | "netbsd" => {
            let mut command = Command::new("notify-send");
            command.args(["--app-name=oxmacs", title, body]);
            command
        }
        _ => return None,
    };
    command
        .env("OXMACS_TITLE", title)
        .env("OXMACS_BODY", body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    tools::hide_console(&mut command);
    Some(command)
}

pub fn send(title: &str, body: &str) {
    if let Some(mut command) = command(std::env::consts::OS, title, body)
        && let Ok(mut child) = command.spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::command;

    fn parts(os: &str) -> (String, Vec<String>, Vec<(String, String)>) {
        let command = command(os, "#oxmacs", "ana: hi \"there\"").unwrap();
        (
            command.get_program().to_string_lossy().into_owned(),
            command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
            command
                .get_envs()
                .filter_map(|(key, value)| {
                    Some((
                        key.to_string_lossy().into_owned(),
                        value?.to_string_lossy().into_owned(),
                    ))
                })
                .collect(),
        )
    }

    #[test]
    fn each_system_gets_its_own_notifier_with_the_text_kept_out_of_the_script() {
        let (program, args, env) = parts("linux");
        assert_eq!(program, "notify-send");
        assert_eq!(args, ["--app-name=oxmacs", "#oxmacs", "ana: hi \"there\""]);

        let (program, args, env_mac) = parts("macos");
        assert_eq!(program, "osascript");
        assert!(!args.concat().contains("ana: hi"));

        let (program, args, _) = parts("windows");
        assert_eq!(program, "powershell");
        assert!(args.last().unwrap().contains("$env:OXMACS_BODY"));
        assert!(!args.concat().contains("ana: hi"));

        for env in [env, env_mac] {
            assert!(env.contains(&("OXMACS_BODY".to_owned(), "ana: hi \"there\"".to_owned())));
            assert!(env.contains(&("OXMACS_TITLE".to_owned(), "#oxmacs".to_owned())));
        }
        assert!(command("plan9", "a", "b").is_none());
    }
}
