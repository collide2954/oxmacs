mod benches;
mod counting;
mod sampler;
mod tables;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

pub use counting::Counting;

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: Counting = Counting;

const USAGE: &str = "usage: ox perf bench FILE [NAME…] [--root DIR…]
       ox perf sample FILE NAME [--seconds N] [--root DIR…]";

#[derive(Debug, Default)]
struct Arguments {
    positional: Vec<String>,
    roots: Vec<PathBuf>,
    seconds: Option<f64>,
}

fn parse(given: impl IntoIterator<Item = String>) -> Result<Arguments, String> {
    let mut parsed = Arguments::default();
    let mut given = given.into_iter();
    while let Some(argument) = given.next() {
        match argument.as_str() {
            "--root" => parsed
                .roots
                .push(given.next().ok_or("--root needs a folder")?.into()),
            "--seconds" => {
                let seconds = given.next().ok_or("--seconds needs a number")?;
                parsed.seconds = Some(
                    seconds
                        .parse()
                        .map_err(|_| format!("--seconds needs a number, got {seconds}"))?,
                );
            }
            _ => parsed.positional.push(argument),
        }
    }
    Ok(parsed)
}

fn run(arguments: &Arguments) -> Result<String, String> {
    match arguments.positional.as_slice() {
        [command, file, names @ ..] if command == "bench" => {
            benches::bench(&PathBuf::from(file), names, &arguments.roots)
        }
        [command, file, name] if command == "sample" => sampler::profile(
            &PathBuf::from(file),
            name,
            &arguments.roots,
            Duration::from_secs_f64(arguments.seconds.unwrap_or(4.0)),
        ),
        _ => Err(USAGE.to_owned()),
    }
}

pub fn main(arguments: impl IntoIterator<Item = String>) -> ExitCode {
    match parse(arguments).and_then(|arguments| run(&arguments)) {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(problem) => {
            eprintln!("{problem}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn options_can_sit_anywhere_among_the_arguments() {
        let parsed = parse(
            [
                "bench",
                "--root",
                "plugins",
                "vm.ox",
                "loop",
                "--seconds",
                "2",
            ]
            .map(String::from),
        )
        .unwrap();

        assert_eq!(parsed.positional, ["bench", "vm.ox", "loop"]);
        assert_eq!(parsed.roots, [std::path::PathBuf::from("plugins")]);
        assert_eq!(parsed.seconds, Some(2.0));
        assert_eq!(
            parse(["--seconds", "soon"].map(String::from)).unwrap_err(),
            "--seconds needs a number, got soon"
        );
    }
}
