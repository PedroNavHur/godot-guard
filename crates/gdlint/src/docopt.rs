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

use pycompat::builtins::repr;

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
            Item::Argument(value) => format!("Argument(None, {})", repr(value)),
            Item::Option(opt, value) => {
                let text = |s: &Option<String>| s.as_deref().map_or("None".to_owned(), repr);
                let value = match value {
                    OptionValue::True => "True".to_owned(),
                    OptionValue::Text(text) => repr(text),
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
    let lint_outcome = (!paths.is_empty()).then(|| lint_left(&items));
    let dump_outcome = dump_left(&items);
    let (command, left) = match (lint_outcome, dump_outcome) {
        (Some((lint, _)), Some(dump)) if dump.len() < lint.len() => (Command::Dump, dump),
        (Some((lint, verbose)), _) => (Command::Lint { paths, verbose }, lint),
        (None, Some(dump)) => (Command::Dump, dump),
        (None, None) => return usage_with(""),
    };
    if left.is_empty() {
        return command;
    }
    let reprs: Vec<String> = left.iter().map(Item::repr).collect();
    usage_with(&format!(
        "Warning: found unmatched (duplicate?) arguments [{}]",
        reprs.join(", ")
    ))
}

/// The options left once the lint line has taken `--verbose`, `--help` and `--version`, and whether
/// `--verbose` was among them.
fn lint_left(items: &[Item]) -> (Vec<Item>, bool) {
    let mut left: Vec<Item> = items
        .iter()
        .filter(|item| matches!(item, Item::Option(..)))
        .cloned()
        .collect();
    let mut verbose = false;
    for name in ["--verbose", "--help", "--version"] {
        if let Some(position) = left
            .iter()
            .position(|item| item.option_name() == Some(name))
        {
            left.remove(position);
            verbose |= name == "--verbose";
        }
    }
    (left, verbose)
}

/// The items left once the dump line has taken `--dump-default-config`, if it is there.
fn dump_left(items: &[Item]) -> Option<Vec<Item>> {
    let position = items
        .iter()
        .position(|item| item.option_name() == Some("--dump-default-config"))?;
    let mut left = items.to_vec();
    left.remove(position);
    Some(left)
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
        } else if is_short_cluster(token) {
            parsed.extend(parse_shorts(token, &mut options));
        } else {
            parsed.push(Item::Argument(token.clone()));
        }
    }
    Ok(parsed)
}

/// Whether `token` is `-` followed by short options, not `-`, a negative number or a long option.
fn is_short_cluster(token: &str) -> bool {
    token.starts_with('-') && !token.starts_with("--") && token != "-" && !is_a_number(token)
}

/// The `-x` items of a cluster like `-dv`.
fn parse_shorts(token: &str, options: &mut Vec<Opt>) -> Vec<Item> {
    token
        .chars()
        .skip(1)
        .map(|short| parse_short(&format!("-{short}"), options))
        .collect()
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
        return Ok(unknown_long(options, longer, has_eq, given));
    };
    let value = long_value(&found, given, tokens)?;
    Ok((found, value))
}

/// A long option docopt has not seen: it takes an argument only if the token had an `=`.
fn unknown_long(
    options: &mut Vec<Opt>,
    longer: &str,
    has_eq: bool,
    given: Option<String>,
) -> (Opt, OptionValue) {
    let argcount = usize::from(has_eq);
    options.push(Opt::new(None, Some(longer), argcount));
    let value = match (argcount, given) {
        (1, Some(value)) => OptionValue::Text(value),
        _ => OptionValue::True,
    };
    (Opt::new(None, Some(longer), argcount), value)
}

/// The value of a known long option: after `=`, or the next token unless that is `--`.
fn long_value<'a>(
    found: &Opt,
    given: Option<String>,
    tokens: &mut std::iter::Peekable<impl Iterator<Item = &'a String>>,
) -> Result<OptionValue, String> {
    let name = found.longer.clone().unwrap_or_default();
    match (found.argcount, given) {
        (0, Some(_)) => Err(format!("{name} must not have an argument")),
        (0, None) => Ok(OptionValue::True),
        (_, Some(value)) => Ok(OptionValue::Text(value)),
        (_, None) => match tokens.next_if(|next| next.as_str() != "--") {
            Some(next) => Ok(OptionValue::Text(next.clone())),
            None => Err(format!("{name} requires argument")),
        },
    }
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
    let trimmed = pycompat::string::strip(text);
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

    #[test]
    fn only_dash_letters_make_a_short_cluster() {
        assert!(is_short_cluster("-dv"));
        for token in ["-", "--", "--verbose", "-1", "-1e3", "ok.gd"] {
            assert!(!is_short_cluster(token), "{token}");
        }
    }
}
