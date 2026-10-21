//! gdlint's command line, read the way docopt-ng 0.9.0 reads it against gdlint's usage:
//!
//! ```text
//! Usage:
//!   gdlint <path>... [options]
//!   gdlint -d
//! ```
//!
//! `[options]` stands for `-v`, `-h` and `--version`: docopt leaves `-d` out because the usage names it, so
//! `-d` is accepted only alone. This ports `parse_argv`, `extras` and the pattern match for that one usage,
//! including docopt's quirks: `--` is itself a path, long options match by unique prefix, a number such as
//! `-1` is a path, and a repeated option is an error.

use crate::repr::str_repr;

pub const DOC: &str = "GDScript linter

A tool for diagnosing typical GDScript code problems.
On success and the exitcode is 0.
On failure, python exception or list of problems is shown and exitcode is non-zero.

Usage:
  gdlint <path>... [options]
  gdlint -d

Options:
  -d --dump-default-config   Dump default config to 'gdlintrc' file
  -v --verbose               Show extra prints
  -h --help                  Show this screen.
  --version                  Show version.";

pub const USAGE: &str = "Usage:\n  gdlint <path>... [options]\n  gdlint -d";

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Version,
    /// A usage error: the text docopt prints to stderr before exiting with status 1.
    Usage(String),
    Dump,
    Lint {
        paths: Vec<String>,
        verbose: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum OptionValue {
    True,
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
struct Opt {
    short: Option<String>,
    longer: Option<String>,
    argcount: usize,
}

#[derive(Clone, Debug, PartialEq)]
enum Item {
    Argument(String),
    Option(Opt, OptionValue),
}

impl Opt {
    fn new(short: Option<&str>, longer: Option<&str>, argcount: usize) -> Opt {
        Opt {
            short: short.map(str::to_owned),
            longer: longer.map(str::to_owned),
            argcount,
        }
    }

    fn name(&self) -> &str {
        self.longer
            .as_deref()
            .or(self.short.as_deref())
            .unwrap_or_default()
    }
}

impl Item {
    fn repr(&self) -> String {
        match self {
            Item::Argument(value) => format!("Argument(None, {})", str_repr(value)),
            Item::Option(opt, value) => {
                let text = |s: &Option<String>| s.as_deref().map_or("None".to_owned(), str_repr);
                let value = match value {
                    OptionValue::True => "True".to_owned(),
                    OptionValue::Text(text) => str_repr(text),
                };
                format!(
                    "Option({}, {}, {}, {value})",
                    text(&opt.short),
                    text(&opt.longer),
                    opt.argcount
                )
            }
        }
    }

    fn option_name(&self) -> Option<&str> {
        match self {
            Item::Option(opt, _) => Some(opt.name()),
            Item::Argument(_) => None,
        }
    }
}

pub fn parse(argv: &[String]) -> Command {
    let items = match parse_argv(argv) {
        Ok(items) => items,
        Err(message) => return usage_with(&message),
    };
    // `extras`: help, then version, wherever they appear and whatever else is wrong.
    let has = |name: &str| items.iter().any(|item| item.option_name() == Some(name));
    if has("--help") {
        return Command::Help;
    }
    if has("--version") {
        return Command::Version;
    }
    match_usage(items)
}

fn usage_with(message: &str) -> Command {
    Command::Usage(format!("{message}\n{USAGE}").trim().to_owned())
}

/// The `Either` of gdlint's two usage lines, which keeps the match that leaves fewest items, the first on a tie.
fn match_usage(items: Vec<Item>) -> Command {
    let paths: Vec<String> = items
        .iter()
        .filter_map(|item| match item {
            Item::Argument(path) => Some(path.clone()),
            Item::Option(..) => None,
        })
        .collect();
    let mut lint_left: Vec<Item> = items
        .iter()
        .filter(|item| matches!(item, Item::Option(..)))
        .cloned()
        .collect();
    let mut verbose = false;
    for name in ["--verbose", "--help", "--version"] {
        if let Some(position) = lint_left
            .iter()
            .position(|item| item.option_name() == Some(name))
        {
            lint_left.remove(position);
            verbose |= name == "--verbose";
        }
    }
    let dump_left = items
        .iter()
        .position(|item| item.option_name() == Some("--dump-default-config"))
        .map(|position| {
            let mut left = items.clone();
            left.remove(position);
            left
        });
    let lint_outcome = (!paths.is_empty()).then_some(lint_left);
    let best = match (lint_outcome, dump_left) {
        (Some(lint), Some(dump)) if dump.len() < lint.len() => (false, dump),
        (Some(lint), _) => (true, lint),
        (None, Some(dump)) => (false, dump),
        (None, None) => return usage_with(""),
    };
    match best {
        (_, left) if !left.is_empty() => {
            let reprs: Vec<String> = left.iter().map(Item::repr).collect();
            usage_with(&format!(
                "Warning: found unmatched (duplicate?) arguments [{}]",
                reprs.join(", ")
            ))
        }
        (true, _) => Command::Lint { paths, verbose },
        (false, _) => Command::Dump,
    }
}

/// `parse_argv`, without `options_first` or `more_magic`, as gdlint calls it.
fn parse_argv(argv: &[String]) -> Result<Vec<Item>, String> {
    let mut options = vec![
        Opt::new(Some("-d"), Some("--dump-default-config"), 0),
        Opt::new(Some("-v"), Some("--verbose"), 0),
        Opt::new(Some("-h"), Some("--help"), 0),
        Opt::new(None, Some("--version"), 0),
    ];
    let mut parsed = Vec::new();
    let mut tokens = argv.iter().peekable();
    while let Some(token) = tokens.next() {
        if token == "--" {
            parsed.push(Item::Argument(token.clone()));
            parsed.extend(tokens.by_ref().map(|t| Item::Argument(t.clone())));
        } else if token.starts_with("--") {
            let (opt, value) = parse_longer(token, &mut tokens, &mut options)?;
            parsed.push(Item::Option(opt, value));
        } else if token.starts_with('-') && token != "-" && !is_a_number(token) {
            for short in token.chars().skip(1) {
                parsed.push(parse_short(&format!("-{short}"), &mut options));
            }
        } else {
            parsed.push(Item::Argument(token.clone()));
        }
    }
    Ok(parsed)
}

fn parse_longer<'a>(
    token: &str,
    tokens: &mut std::iter::Peekable<impl Iterator<Item = &'a String>>,
    options: &mut Vec<Opt>,
) -> Result<(Opt, OptionValue), String> {
    let (longer, has_eq, given) = match token.split_once('=') {
        Some((longer, value)) => (longer, true, Some(value.to_owned())),
        None => (token, false, None),
    };
    let is_prefix = |o: &&Opt| o.longer.as_deref().is_some_and(|l| l.starts_with(longer));
    let mut similar: Vec<Opt> = options
        .iter()
        .filter(|o| o.longer.as_deref() == Some(longer))
        .cloned()
        .collect();
    let start_collision = options.iter().filter(is_prefix).count() > 1;
    if similar.is_empty() && !start_collision {
        similar = options.iter().filter(is_prefix).cloned().collect();
    }
    let Some(found) = similar.first().cloned() else {
        let argcount = usize::from(has_eq);
        options.push(Opt::new(None, Some(longer), argcount));
        let value = match (argcount, given) {
            (1, Some(value)) => OptionValue::Text(value),
            _ => OptionValue::True,
        };
        return Ok((Opt::new(None, Some(longer), argcount), value));
    };
    let name = found.longer.clone().unwrap_or_default();
    let value = match (found.argcount, given) {
        (0, Some(_)) => return Err(format!("{name} must not have an argument")),
        (0, None) => OptionValue::True,
        (_, Some(value)) => OptionValue::Text(value),
        (_, None) => match tokens.peek() {
            None => return Err(format!("{name} requires argument")),
            Some(next) if next.as_str() == "--" => return Err(format!("{name} requires argument")),
            Some(_) => OptionValue::Text(tokens.next().expect("peeked").clone()),
        },
    };
    Ok((found, value))
}

/// One `-x` of a cluster. gdlint has no short option that takes an argument.
fn parse_short(short: &str, options: &mut Vec<Opt>) -> Item {
    match options.iter().find(|o| o.short.as_deref() == Some(short)) {
        Some(found) => Item::Option(found.clone(), OptionValue::True),
        None => {
            options.push(Opt::new(Some(short), None, 0));
            Item::Option(Opt::new(Some(short), None, 0), OptionValue::True)
        }
    }
}

/// Whether Python's `float(text)` accepts it: Rust's float syntax, plus surrounding whitespace and
/// underscores between digits.
fn is_a_number(text: &str) -> bool {
    let trimmed = crate::text::python_strip(text);
    let chars: Vec<char> = trimmed.chars().collect();
    let underscores_ok = chars.iter().enumerate().all(|(i, &c)| {
        c != '_'
            || (i > 0
                && chars[i - 1].is_ascii_digit()
                && chars.get(i + 1).is_some_and(char::is_ascii_digit))
    });
    underscores_ok && trimmed.replace('_', "").parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Command {
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    fn warning(left: &str) -> Command {
        Command::Usage(format!(
            "Warning: found unmatched (duplicate?) arguments [{left}]\n{USAGE}"
        ))
    }

    #[test]
    fn matches_the_oracle_on_edge_cases() {
        assert_eq!(run(&[]), Command::Usage(USAGE.into()));
        assert_eq!(run(&["ok.gd", "-h", "--bogus"]), Command::Help);
        assert_eq!(run(&["ok.gd", "--version"]), Command::Version);
        assert_eq!(
            run(&["--bogus", "ok.gd"]),
            warning("Option(None, '--bogus', 0, True)")
        );
        assert_eq!(
            run(&["-d", "-v"]),
            warning("Option('-v', '--verbose', 0, True)")
        );
        assert_eq!(
            run(&["ok.gd", "-d"]),
            warning("Option('-d', '--dump-default-config', 0, True)")
        );
        assert_eq!(
            run(&["-vv", "ok.gd"]),
            warning("Option('-v', '--verbose', 0, True)")
        );
        assert_eq!(
            run(&["--ver", "ok.gd"]),
            warning("Option(None, '--ver', 0, True)")
        );
        assert_eq!(run(&["-xd"]), warning("Option('-x', None, 0, True)"));
        assert_eq!(
            run(&["--verbose=1", "ok.gd"]),
            Command::Usage(format!("--verbose must not have an argument\n{USAGE}"))
        );
        assert_eq!(run(&["-d"]), Command::Dump);
        let lint = |paths: &[&str], verbose| Command::Lint {
            paths: paths.iter().map(|s| s.to_string()).collect(),
            verbose,
        };
        assert_eq!(run(&["--verb", "ok.gd"]), lint(&["ok.gd"], true));
        assert_eq!(run(&["--", "-x.gd"]), lint(&["--", "-x.gd"], false));
        assert_eq!(run(&["-", "-1", "-1e3"]), lint(&["-", "-1", "-1e3"], false));
    }
}
