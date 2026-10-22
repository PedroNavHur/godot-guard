//! gdformat's docopt command line, parsed by hand: the same options, long ones abbreviable.

use std::slice::Iter;

use crate::cli::ArgumentsExit;

pub const USAGE: &str = "Usage:\n  gdformat <path>... [options]\n  gdformat --dump-default-config";

pub const HELP: &str = "GDScript formatter

Uncompromising GDScript code formatter. The only configurable thing is
max line length allowed and tabs/spaces indent. The rest will be taken
care of by gdformat in a one, consistent way.

Usage:
  gdformat <path>... [options]
  gdformat --dump-default-config

Options:
  -c --check                 Don't write the files back,
                             just check if formatting is possible.
  -d --diff                  Don't write the files back,
                             just suggest formatting changes
                             (implies --check).
  -f --fast                  Skip safety checks.
  -l --line-length=<int>     How many characters per line to allow.
  -s --use-spaces=<int>      Use spaces for indent instead of tabs.
  -h --help                  Show this screen.
  --version                  Show version.
  --dump-default-config      Dump default config to 'gdformatrc' file.

Examples:
  echo 'pass' | gdformat -   # reads from STDIN";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Arguments {
    pub paths: Vec<String>,
    pub check: bool,
    pub diff: bool,
    pub fast: bool,
    pub line_length: Option<i64>,
    pub use_spaces: Option<usize>,
    pub dump_default_config: bool,
}

const LONG_OPTIONS: [&str; 8] = [
    "--check",
    "--diff",
    "--fast",
    "--line-length",
    "--use-spaces",
    "--help",
    "--version",
    "--dump-default-config",
];

/// The long option `given` names: itself or the one option it is a prefix of.
fn long_option(given: &str) -> Result<&'static str, ArgumentsExit> {
    if let Some(exact) = LONG_OPTIONS.iter().find(|option| **option == given) {
        return Ok(exact);
    }
    let mut candidates = LONG_OPTIONS
        .iter()
        .filter(|option| option.starts_with(given));
    match (candidates.next(), candidates.next()) {
        (Some(only), None) => Ok(only),
        _ => Err(ArgumentsExit::Usage(format!(
            "{given} is not a unique option\n"
        ))),
    }
}

fn short_option(letter: char) -> Option<&'static str> {
    Some(match letter {
        'c' => "--check",
        'd' => "--diff",
        'f' => "--fast",
        'l' => "--line-length",
        's' => "--use-spaces",
        'h' => "--help",
        _ => return None,
    })
}

fn takes_value(option: &str) -> bool {
    matches!(option, "--line-length" | "--use-spaces")
}

/// An option and its value, if it takes one.
type OptionValue = (&'static str, Option<String>);

/// Every option and its value, if it takes one, then the positional paths.
type Tokens = (Vec<OptionValue>, Vec<String>);

fn tokenize(args: &[String]) -> Result<Tokens, ArgumentsExit> {
    let mut options = Vec::new();
    let mut paths = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        if arg == "--" {
            paths.extend(rest.by_ref().cloned());
        } else if let Some(long) = arg.strip_prefix("--") {
            options.push(parse_long(long, &mut rest)?);
        } else if arg.len() > 1 && arg.starts_with('-') {
            parse_shorts(arg, &mut rest, &mut options)?;
        } else {
            paths.push(arg.clone());
        }
    }
    Ok((options, paths))
}

/// The option `--name[=value]` gives (`long` is what follows the dashes); a value it takes and was not
/// given inline is the next argument.
fn parse_long(long: &str, rest: &mut Iter<String>) -> Result<OptionValue, ArgumentsExit> {
    let (name, inline_value) = match long.split_once('=') {
        Some((name, value)) => (name, Some(value.to_owned())),
        None => (long, None),
    };
    let option = long_option(&format!("--{name}"))?;
    let value = match (takes_value(option), inline_value) {
        (true, Some(value)) => Some(value),
        (true, None) => Some(rest.next().cloned().ok_or_else(|| missing(option))?),
        (false, Some(_)) => {
            return Err(ArgumentsExit::Usage(format!(
                "{option} must not have an argument\n"
            )));
        }
        (false, None) => None,
    };
    Ok((option, value))
}

/// The options of the cluster `arg` (`-cfl80`): the first that takes a value ends it, with the rest of
/// the cluster as the value, or the next argument when nothing is attached.
fn parse_shorts(
    arg: &str,
    rest: &mut Iter<String>,
    options: &mut Vec<OptionValue>,
) -> Result<(), ArgumentsExit> {
    for (at, letter) in arg[1..].char_indices() {
        let option = short_option(letter)
            .ok_or_else(|| ArgumentsExit::Usage(format!("-{letter} is not recognized\n")))?;
        if !takes_value(option) {
            options.push((option, None));
            continue;
        }
        let attached = &arg[1 + at + letter.len_utf8()..];
        let value = match attached.is_empty() {
            true => rest.next().cloned().ok_or_else(|| missing(option))?,
            false => attached.to_owned(),
        };
        options.push((option, Some(value)));
        break;
    }
    Ok(())
}

fn missing(option: &str) -> ArgumentsExit {
    ArgumentsExit::Usage(format!("{option} requires argument\n"))
}

/// The arguments after the program name.
pub fn parse_arguments(args: &[String]) -> Result<Arguments, ArgumentsExit> {
    let (options, paths) = tokenize(args)?;
    let mut arguments = Arguments {
        paths,
        ..Arguments::default()
    };
    for (option, value) in options {
        match option {
            "--help" => return Err(ArgumentsExit::Help),
            "--version" => return Err(ArgumentsExit::Version),
            "--check" => arguments.check = true,
            "--diff" => arguments.diff = true,
            "--fast" => arguments.fast = true,
            "--dump-default-config" => arguments.dump_default_config = true,
            "--line-length" => arguments.line_length = Some(integer(option, value)?),
            "--use-spaces" => {
                let spaces = integer(option, value)?;
                let spaces = usize::try_from(spaces).ok().filter(|n| *n > 0);
                arguments.use_spaces = Some(spaces.ok_or_else(|| {
                    ArgumentsExit::Usage("--use-spaces must be a positive integer\n".to_owned())
                })?);
            }
            _ => unreachable!("unknown option {option}"),
        }
    }
    if arguments.diff {
        arguments.check = true;
    }
    if arguments.paths.is_empty() && !arguments.dump_default_config {
        return Err(ArgumentsExit::Usage(String::new()));
    }
    Ok(arguments)
}

fn integer(option: &str, value: Option<String>) -> Result<i64, ArgumentsExit> {
    let value = value.unwrap_or_default();
    value.trim().parse().map_err(|_| {
        ArgumentsExit::Usage(format!(
            "{option}: invalid literal for int() with base 10: {value:?}\n"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(args: &[&str]) -> Arguments {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        parse_arguments(&args).unwrap_or_else(|_| panic!("{args:?} did not parse"))
    }

    #[test]
    fn reads_every_spelling_of_the_options() {
        let expected = Arguments {
            paths: vec!["a.gd".into(), "-".into()],
            check: true,
            fast: true,
            line_length: Some(80),
            use_spaces: Some(2),
            ..Arguments::default()
        };
        assert_eq!(parsed(&["-cfl80", "a.gd", "-s", "2", "-"]), expected);
        assert_eq!(
            parsed(&[
                "--che",
                "--fast",
                "--line-length=80",
                "a.gd",
                "--use-spaces",
                "2",
                "-"
            ]),
            expected
        );
    }

    #[test]
    fn diff_implies_check() {
        assert!(parsed(&["-d", "a.gd"]).check);
    }

    #[test]
    fn rejects_unknown_options_and_missing_paths() {
        for args in [
            &["--bogus", "a.gd"][..],
            &["-x", "a.gd"],
            &[],
            &["-l"],
            &["--d", "a.gd"],
        ] {
            let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
            assert!(
                matches!(parse_arguments(&args), Err(ArgumentsExit::Usage(_))),
                "{args:?}"
            );
        }
    }
}
