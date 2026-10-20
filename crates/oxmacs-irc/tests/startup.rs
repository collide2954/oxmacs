use oxmacs_core::config::Config;
use oxmacs_gui::app::App;
use oxmacs_irc::Irc;

fn config() -> Config {
    Config::parse(
        "[[irc.server]]\nname = \"libera\"\nhost = \"irc.libera.chat\"\nnick = \"oxtest\"\nchannels = [\"#c\"]\n\n[irc.bouncer]\nmode = \"local\"\n",
    )
    .unwrap()
}

#[test]
fn startup_shows_the_splash_and_irc_waits_for_m_x_irc() {
    let (app, _) = App::boot(&Ok(config()), &[], vec![Box::new(Irc)]);

    assert_eq!(app.editor().buffer().name(), "*oxmacs*");
    assert!(!app.editor().buffer_names().contains(&"libera"));
}
