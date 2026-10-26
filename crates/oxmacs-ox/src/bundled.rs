use crate::plugins::Bundle;

pub const BUNDLED: &[Bundle] = &[
    WEATHER,
    DIRED,
    GIT,
    SCREENSAVER,
    PROFILER,
    PROCESSES,
    CUSTOMIZE,
    VAULT,
    IRC,
    CALENDAR,
    LIFE,
    REMOTE,
    MARKDOWN,
    MANUAL,
    R,
    ZAP,
    KEYCAST,
    LANGUAGES,
    TABS,
    EXPLORER,
    ZEN,
    EPUB,
];

const EPUB: Bundle = Bundle {
    name: "epub",
    files: &[
        ("plugin.ox", include_str!("../plugins/epub/plugin.ox")),
        ("epub.ox", include_str!("../plugins/epub/epub.ox")),
        ("package.ox", include_str!("../plugins/epub/package.ox")),
        ("html.ox", include_str!("../plugins/epub/html.ox")),
        ("reading.ox", include_str!("../plugins/epub/reading.ox")),
        ("library.ox", include_str!("../plugins/epub/library.ox")),
    ],
};

const ZEN: Bundle = Bundle {
    name: "zen",
    files: &[
        ("plugin.ox", include_str!("../plugins/zen/plugin.ox")),
        ("zen.ox", include_str!("../plugins/zen/zen.ox")),
        ("look.ox", include_str!("../plugins/zen/look.ox")),
    ],
};

const EXPLORER: Bundle = Bundle {
    name: "explorer",
    files: &[
        ("plugin.ox", include_str!("../plugins/explorer/plugin.ox")),
        (
            "explorer.ox",
            include_str!("../plugins/explorer/explorer.ox"),
        ),
        ("tree.ox", include_str!("../plugins/explorer/tree.ox")),
    ],
};

const TABS: Bundle = Bundle {
    name: "tabs",
    files: &[
        ("plugin.ox", include_str!("../plugins/tabs/plugin.ox")),
        ("tabs.ox", include_str!("../plugins/tabs/tabs.ox")),
        ("strip.ox", include_str!("../plugins/tabs/strip.ox")),
    ],
};

const LANGUAGES: Bundle = Bundle {
    name: "languages",
    files: &[
        ("plugin.ox", include_str!("../plugins/languages/plugin.ox")),
        (
            "languages.ox",
            include_str!("../plugins/languages/languages.ox"),
        ),
        ("lexer.ox", include_str!("../plugins/languages/lexer.ox")),
        ("specs.ox", include_str!("../plugins/languages/specs.ox")),
    ],
};

const ZAP: Bundle = Bundle {
    name: "zap",
    files: &[
        ("plugin.ox", include_str!("../plugins/zap/plugin.ox")),
        ("zap.ox", include_str!("../plugins/zap/zap.ox")),
    ],
};

const KEYCAST: Bundle = Bundle {
    name: "keycast",
    files: &[
        ("plugin.ox", include_str!("../plugins/keycast/plugin.ox")),
        ("keycast.ox", include_str!("../plugins/keycast/keycast.ox")),
        ("stack.ox", include_str!("../plugins/keycast/stack.ox")),
    ],
};

const R: Bundle = Bundle {
    name: "r",
    files: &[
        ("plugin.ox", include_str!("../plugins/r/plugin.ox")),
        ("r.ox", include_str!("../plugins/r/r.ox")),
        ("session.ox", include_str!("../plugins/r/session.ox")),
        ("helper.ox", include_str!("../plugins/r/helper.ox")),
        ("help.ox", include_str!("../plugins/r/help.ox")),
        ("pane.ox", include_str!("../plugins/r/pane.ox")),
        ("plots.ox", include_str!("../plugins/r/plots.ox")),
        ("data.ox", include_str!("../plugins/r/data.ox")),
        ("complete.ox", include_str!("../plugins/r/complete.ox")),
        ("definition.ox", include_str!("../plugins/r/definition.ox")),
        ("layout.ox", include_str!("../plugins/r/layout.ox")),
        ("package.ox", include_str!("../plugins/r/package.ox")),
        ("markdown.ox", include_str!("../plugins/r/markdown.ox")),
        ("language.ox", include_str!("../plugins/r/language.ox")),
        ("jobs.ox", include_str!("../plugins/r/jobs.ox")),
        ("debug.ox", include_str!("../plugins/r/debug.ox")),
    ],
};

const MANUAL: Bundle = Bundle {
    name: "manual",
    files: &[
        ("plugin.ox", include_str!("../plugins/manual/plugin.ox")),
        ("manual.ox", include_str!("../plugins/manual/manual.ox")),
        ("start.ox", include_str!("../plugins/manual/start.ox")),
        ("language.ox", include_str!("../plugins/manual/language.ox")),
        ("data.ox", include_str!("../plugins/manual/data.ox")),
        ("flow.ox", include_str!("../plugins/manual/flow.ox")),
        ("modules.ox", include_str!("../plugins/manual/modules.ox")),
        ("tasks.ox", include_str!("../plugins/manual/tasks.ox")),
        ("editor.ox", include_str!("../plugins/manual/editor.ox")),
        ("ui.ox", include_str!("../plugins/manual/ui.ox")),
        ("plugins.ox", include_str!("../plugins/manual/plugins.ox")),
        ("tools.ox", include_str!("../plugins/manual/tools.ox")),
        ("tutorial.ox", include_str!("../plugins/manual/tutorial.ox")),
    ],
};

const MARKDOWN: Bundle = Bundle {
    name: "markdown",
    files: &[
        ("plugin.ox", include_str!("../plugins/markdown/plugin.ox")),
        (
            "markdown.ox",
            include_str!("../plugins/markdown/markdown.ox"),
        ),
        ("render.ox", include_str!("../plugins/markdown/render.ox")),
        ("style.ox", include_str!("../plugins/markdown/style.ox")),
        ("preview.ox", include_str!("../plugins/markdown/preview.ox")),
    ],
};

const REMOTE: Bundle = Bundle {
    name: "remote",
    files: &[
        ("plugin.ox", include_str!("../plugins/remote/plugin.ox")),
        ("remote.ox", include_str!("../plugins/remote/remote.ox")),
        ("hosts.ox", include_str!("../plugins/remote/hosts.ox")),
        ("shell.ox", include_str!("../plugins/remote/shell.ox")),
        ("ops.ox", include_str!("../plugins/remote/ops.ox")),
    ],
};

const LIFE: Bundle = Bundle {
    name: "life",
    files: &[
        ("plugin.ox", include_str!("../plugins/life/plugin.ox")),
        ("life.ox", include_str!("../plugins/life/life.ox")),
        ("rules.ox", include_str!("../plugins/life/rules.ox")),
    ],
};

const CALENDAR: Bundle = Bundle {
    name: "calendar",
    files: &[
        ("plugin.ox", include_str!("../plugins/calendar/plugin.ox")),
        (
            "calendar.ox",
            include_str!("../plugins/calendar/calendar.ox"),
        ),
        ("store.ox", include_str!("../plugins/calendar/store.ox")),
        ("ics.ox", include_str!("../plugins/calendar/ics.ox")),
        ("rrule.ox", include_str!("../plugins/calendar/rrule.ox")),
        ("caldav.ox", include_str!("../plugins/calendar/caldav.ox")),
        (
            "reminders.ox",
            include_str!("../plugins/calendar/reminders.ox"),
        ),
    ],
};

const IRC: Bundle = Bundle {
    name: "irc",
    files: &[
        ("plugin.ox", include_str!("../plugins/irc/plugin.ox")),
        ("irc.ox", include_str!("../plugins/irc/irc.ox")),
        ("client.ox", include_str!("../plugins/irc/client.ox")),
        ("protocol.ox", include_str!("../plugins/irc/protocol.ox")),
        ("format.ox", include_str!("../plugins/irc/format.ox")),
        ("logs.ox", include_str!("../plugins/irc/logs.ox")),
    ],
};

const VAULT: Bundle = Bundle {
    name: "vault",
    files: &[
        ("plugin.ox", include_str!("../plugins/vault/plugin.ox")),
        ("vault.ox", include_str!("../plugins/vault/vault.ox")),
        ("store.ox", include_str!("../plugins/vault/store.ox")),
        ("import.ox", include_str!("../plugins/vault/import.ox")),
        ("words.ox", include_str!("../plugins/vault/words.ox")),
    ],
};

const CUSTOMIZE: Bundle = Bundle {
    name: "customize",
    files: &[
        ("plugin.ox", include_str!("../plugins/customize/plugin.ox")),
        (
            "customize.ox",
            include_str!("../plugins/customize/customize.ox"),
        ),
        ("form.ox", include_str!("../plugins/customize/form.ox")),
        (
            "written.ox",
            include_str!("../plugins/customize/written.ox"),
        ),
        ("save.ox", include_str!("../plugins/customize/save.ox")),
    ],
};

const PROFILER: Bundle = Bundle {
    name: "profiler",
    files: &[
        ("plugin.ox", include_str!("../plugins/profiler/plugin.ox")),
        (
            "profiler.ox",
            include_str!("../plugins/profiler/profiler.ox"),
        ),
        ("report.ox", include_str!("../plugins/profiler/report.ox")),
    ],
};

const PROCESSES: Bundle = Bundle {
    name: "processes",
    files: &[
        ("plugin.ox", include_str!("../plugins/processes/plugin.ox")),
        (
            "processes.ox",
            include_str!("../plugins/processes/processes.ox"),
        ),
        ("table.ox", include_str!("../plugins/processes/table.ox")),
    ],
};

const SCREENSAVER: Bundle = Bundle {
    name: "screensaver",
    files: &[
        (
            "plugin.ox",
            include_str!("../plugins/screensaver/plugin.ox"),
        ),
        (
            "screensaver.ox",
            include_str!("../plugins/screensaver/screensaver.ox"),
        ),
        ("noise.ox", include_str!("../plugins/screensaver/noise.ox")),
        (
            "starfield.ox",
            include_str!("../plugins/screensaver/starfield.ox"),
        ),
        (
            "matrix.ox",
            include_str!("../plugins/screensaver/matrix.ox"),
        ),
    ],
};

const GIT: Bundle = Bundle {
    name: "git",
    files: &[
        ("plugin.ox", include_str!("../plugins/git/plugin.ox")),
        ("git.ox", include_str!("../plugins/git/git.ox")),
        ("repo.ox", include_str!("../plugins/git/repo.ox")),
        ("diff.ox", include_str!("../plugins/git/diff.ox")),
        ("status.ox", include_str!("../plugins/git/status.ox")),
        ("pane.ox", include_str!("../plugins/git/pane.ox")),
        ("commit.ox", include_str!("../plugins/git/commit.ox")),
    ],
};

const DIRED: Bundle = Bundle {
    name: "dired",
    files: &[
        ("plugin.ox", include_str!("../plugins/dired/plugin.ox")),
        ("dired.ox", include_str!("../plugins/dired/dired.ox")),
        ("listing.ox", include_str!("../plugins/dired/listing.ox")),
    ],
};

const WEATHER: Bundle = Bundle {
    name: "weather",
    files: &[
        ("plugin.ox", include_str!("../plugins/weather/plugin.ox")),
        ("weather.ox", include_str!("../plugins/weather/weather.ox")),
        (
            "forecast.ox",
            include_str!("../plugins/weather/forecast.ox"),
        ),
        ("page.ox", include_str!("../plugins/weather/page.ox")),
    ],
};

#[cfg(test)]
mod weather;

#[cfg(test)]
mod catalog;

#[cfg(test)]
mod git;

#[cfg(test)]
mod screensaver;

#[cfg(test)]
mod profiler;

#[cfg(test)]
mod processes;

#[cfg(test)]
mod customize;

#[cfg(test)]
mod vault;

#[cfg(test)]
mod irc;

#[cfg(test)]
mod calendar;

#[cfg(test)]
mod life;

#[cfg(test)]
mod remote;

#[cfg(test)]
mod markdown;

#[cfg(test)]
mod manual;

#[cfg(test)]
mod r;

#[cfg(test)]
mod zap;

#[cfg(test)]
mod keycast;

#[cfg(test)]
mod dired;

#[cfg(test)]
mod languages;

#[cfg(test)]
mod tabs;

#[cfg(test)]
mod explorer;

#[cfg(test)]
mod zen;

#[cfg(test)]
mod epub;
