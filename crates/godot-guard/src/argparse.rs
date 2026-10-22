//! The slice of Python 3.9's argparse that 0.4.0's command line uses, so that every argument list
//! parses, fails, or prints help exactly as it does there.
//!
//! argparse classifies each argument as an option (`O`), a positional (`A`), or `--` (`-`), then matches
//! regexes over that pattern string to hand arguments to each action. Positionals are consumed greedily
//! before each option and once at the end; a positional whose pattern matches nothing yet waits.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use pycompat::builtins;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Help,
    Version,
    Flag,
    /// One value, checked by the function: an error message, or the accepted value.
    Value(fn(&str) -> Result<String, String>),
}

pub struct Opt {
    pub strings: &'static [&'static str],
    pub dest: &'static str,
    pub kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nargs {
    /// `*`: a list, `--` removed.
    Star,
    /// `argparse.REMAINDER`: everything left, `--` kept.
    Remainder,
    /// A subcommand and its arguments.
    Parser,
}

pub struct Parser {
    pub prog: &'static str,
    /// The usage line, as 0.4.0 prints it, and the full help text.
    pub usage: &'static str,
    pub help: &'static str,
    pub options: Vec<Opt>,
    pub positional: Option<(&'static str, Nargs)>,
    pub commands: Vec<(&'static str, Parser)>,
}

/// What a parse produced: values by dest, the subcommand, and its positionals.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Namespace {
    pub command: Option<String>,
    pub flags: Vec<&'static str>,
    pub values: HashMap<&'static str, String>,
    pub positionals: Vec<String>,
}

impl Namespace {
    pub fn flag(&self, dest: &str) -> bool {
        self.flags.contains(&dest)
    }
}

/// Why parsing stopped: help or version printed (status 0), or an error (status 2).
#[derive(Debug, PartialEq, Eq)]
pub struct Exit {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

static NEGATIVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-\d+$|^-\d*\.\d+$").expect("a valid pattern"));

fn nargs_pattern(nargs: Nargs) -> &'static str {
    match nargs {
        Nargs::Star => "^(-*[A-]*)",
        Nargs::Remainder => "^([-AO]*)",
        Nargs::Parser => "^(-*A[-AO]*)",
    }
}

/// An option argument: its action (None when unknown), the option string, and any `=` value.
type Found = (Option<usize>, String, Option<String>);

struct State<'p> {
    parser: &'p Parser,
    version: &'p str,
    args: Vec<String>,
    pattern: Vec<u8>,
    options: HashMap<usize, Found>,
    namespace: Namespace,
    extras: Vec<String>,
    positional_done: bool,
}

impl Parser {
    fn error(&self, message: &str) -> Exit {
        Exit {
            status: 2,
            stdout: String::new(),
            stderr: format!("{}\n{}: error: {message}\n", self.usage, self.prog),
        }
    }

    fn lookup(&self, text: &str) -> Option<usize> {
        self.options
            .iter()
            .position(|opt| opt.strings.contains(&text))
    }

    /// `_get_option_tuples`: the options `text` abbreviates.
    fn abbreviations(&self, text: &str) -> Vec<Found> {
        if !text.starts_with("--") {
            return self.short_abbreviations(text);
        }
        match text.split_once('=') {
            Some((prefix, value)) => self.long_abbreviations(prefix, Some(value)),
            None => self.long_abbreviations(text, None),
        }
    }

    /// Every option string, with the index of its option.
    fn option_strings(&self) -> impl Iterator<Item = (usize, &'static str)> + '_ {
        let strings = |(index, opt): (usize, &Opt)| opt.strings.iter().map(move |s| (index, *s));
        self.options.iter().enumerate().flat_map(strings)
    }

    fn long_abbreviations(&self, prefix: &str, explicit: Option<&str>) -> Vec<Found> {
        self.option_strings()
            .filter(|(_, string)| string.starts_with(prefix))
            .map(|(index, string)| (Some(index), string.to_owned(), explicit.map(str::to_owned)))
            .collect()
    }

    fn short_abbreviations(&self, text: &str) -> Vec<Found> {
        let short_match = |(index, string): (usize, &'static str)| {
            if text.get(..2) == Some(string) {
                let rest = &text[2..];
                let explicit = (!rest.is_empty()).then(|| rest.to_owned());
                return Some((Some(index), string.to_owned(), explicit));
            }
            string
                .starts_with(text)
                .then(|| (Some(index), string.to_owned(), None))
        };
        self.option_strings().filter_map(short_match).collect()
    }

    /// The one option `text` abbreviates; several is an error.
    fn one_abbreviation(&self, text: &str) -> Result<Option<Found>, Exit> {
        let mut found = self.abbreviations(text);
        if found.len() > 1 {
            let names: Vec<&str> = found.iter().map(|(_, string, _)| string.as_str()).collect();
            return Err(self.error(&format!(
                "ambiguous option: {text} could match {}",
                names.join(", ")
            )));
        }
        Ok(found.pop())
    }

    /// `_parse_optional`: None for a positional.
    fn classify(&self, text: &str) -> Result<Option<Found>, Exit> {
        if !text.starts_with('-') {
            return Ok(None);
        }
        if let Some(index) = self.lookup(text) {
            return Ok(Some((Some(index), text.to_owned(), None)));
        }
        if text.len() == 1 {
            return Ok(None);
        }
        if let Some((option, value)) = text.split_once('=')
            && let Some(index) = self.lookup(option)
        {
            return Ok(Some((
                Some(index),
                option.to_owned(),
                Some(value.to_owned()),
            )));
        }
        if let Some(one) = self.one_abbreviation(text)? {
            return Ok(Some(one));
        }
        if NEGATIVE.is_match(text) || text.contains(' ') {
            return Ok(None);
        }
        Ok(Some((None, text.to_owned(), None)))
    }

    /// Parse `args` as `parse_known_args` does: the namespace and the unrecognized arguments.
    pub fn parse_known(
        &self,
        args: &[String],
        version: &str,
    ) -> Result<(Namespace, Vec<String>), Exit> {
        let mut options = HashMap::new();
        let mut pattern = Vec::with_capacity(args.len());
        let mut after_dashes = false;
        for (index, arg) in args.iter().enumerate() {
            if after_dashes {
                pattern.push(b'A');
            } else if arg == "--" {
                after_dashes = true;
                pattern.push(b'-');
            } else if let Some(found) = self.classify(arg)? {
                options.insert(index, found);
                pattern.push(b'O');
            } else {
                pattern.push(b'A');
            }
        }
        let mut state = State {
            parser: self,
            version,
            args: args.to_vec(),
            pattern,
            options,
            namespace: Namespace::default(),
            extras: Vec::new(),
            positional_done: self.positional.is_none(),
        };
        state.run()?;
        if self
            .positional
            .is_some_and(|(_, nargs)| nargs == Nargs::Parser)
            && state.namespace.command.is_none()
        {
            let (dest, _) = self.positional.expect("checked above");
            return Err(self.error(&format!("the following arguments are required: {dest}")));
        }
        Ok((state.namespace, state.extras))
    }

    /// `parse_args`: unrecognized arguments are an error.
    pub fn parse(&self, args: &[String], version: &str) -> Result<Namespace, Exit> {
        let (namespace, extras) = self.parse_known(args, version)?;
        if !extras.is_empty() {
            return Err(self.error(&format!("unrecognized arguments: {}", extras.join(" "))));
        }
        Ok(namespace)
    }
}

impl State<'_> {
    fn run(&mut self) -> Result<(), Exit> {
        let mut indices: Vec<usize> = self.options.keys().copied().collect();
        indices.sort_unstable();
        let mut start = 0;
        while let Some(&next) = indices.iter().find(|&&index| index >= start) {
            if start != next {
                let end = self.consume_positional(start)?;
                if end > start {
                    start = end;
                    continue;
                }
            }
            if !self.options.contains_key(&start) {
                self.extras.extend(self.args[start..next].iter().cloned());
                start = next;
            }
            start = self.consume_option(start)?;
        }
        let stop = self.consume_positional(start)?;
        self.extras.extend(self.args[stop..].iter().cloned());
        Ok(())
    }

    fn consume_positional(&mut self, start: usize) -> Result<usize, Exit> {
        let Some((dest, nargs)) = self.parser.positional.filter(|_| !self.positional_done) else {
            return Ok(start);
        };
        let pattern = std::str::from_utf8(&self.pattern[start..]).expect("an ASCII pattern");
        let Some(found) = Regex::new(nargs_pattern(nargs))
            .expect("a valid pattern")
            .captures(pattern)
        else {
            return Ok(start);
        };
        let end = start + found[1].len();
        self.positional_done = true;
        let mut values: Vec<String> = self.args[start..end].to_vec();
        if nargs == Nargs::Star
            && let Some(dashes) = values.iter().position(|value| value == "--")
        {
            values.remove(dashes);
        }
        if nargs == Nargs::Parser {
            self.subcommand(dest, values)?;
        } else {
            self.namespace.positionals = values;
        }
        Ok(end)
    }

    fn subcommand(&mut self, dest: &str, values: Vec<String>) -> Result<(), Exit> {
        let name = &values[0];
        let Some((_, parser)) = self
            .parser
            .commands
            .iter()
            .find(|(command, _)| command == name)
        else {
            let names: Vec<String> = self
                .parser
                .commands
                .iter()
                .map(|(command, _)| builtins::repr(command))
                .collect();
            let message = format!(
                "argument {dest}: invalid choice: {} (choose from {})",
                builtins::repr(name),
                names.join(", ")
            );
            return Err(self.parser.error(&message));
        };
        let (mut namespace, extras) = parser.parse_known(&values[1..], self.version)?;
        namespace.command = Some(name.clone());
        self.namespace = namespace;
        self.extras.extend(extras);
        Ok(())
    }

    fn consume_option(&mut self, start: usize) -> Result<usize, Exit> {
        let (action, mut string, mut explicit) = self.options[&start].clone();
        let Some(mut index) = action else {
            self.extras.push(self.args[start].clone());
            return Ok(start + 1);
        };
        let mut taken = Vec::new();
        let stop = loop {
            let opt = &self.parser.options[index];
            let takes_value = matches!(opt.kind, Kind::Value(_));
            match explicit {
                Some(value) if takes_value => {
                    taken.push((index, vec![value]));
                    break start + 1;
                }
                // `-hx`: a single-dash flag followed by more single-dash flags.
                Some(rest) if string.as_bytes()[1] != b'-' => {
                    taken.push((index, Vec::new()));
                    (index, string, explicit) = self.next_short_flag(opt, &rest)?;
                }
                Some(value) => return Err(self.ignored(opt, &value)),
                None if takes_value => {
                    taken.push((index, vec![self.take_argument(start, opt)?]));
                    break start + 2;
                }
                None => {
                    taken.push((index, Vec::new()));
                    break start + 1;
                }
            }
        };
        for (index, values) in taken {
            self.take(index, values)?;
        }
        Ok(stop)
    }

    fn ignored(&self, opt: &Opt, explicit: &str) -> Exit {
        let message = format!(
            "argument {}: ignored explicit argument {}",
            opt.strings.join("/"),
            builtins::repr(explicit)
        );
        self.parser.error(&message)
    }

    /// `-hx`: the flag after `opt`, and what follows it.
    fn next_short_flag(
        &self,
        opt: &Opt,
        rest: &str,
    ) -> Result<(usize, String, Option<String>), Exit> {
        let first = rest.chars().next().expect("a nonempty explicit argument");
        let next = format!("-{first}");
        let Some(found) = self.parser.lookup(&next) else {
            return Err(self.ignored(opt, rest));
        };
        let remainder = &rest[first.len_utf8()..];
        Ok((
            found,
            next,
            (!remainder.is_empty()).then(|| remainder.to_owned()),
        ))
    }

    /// The argument that follows an option string.
    fn take_argument(&self, start: usize, opt: &Opt) -> Result<String, Exit> {
        if self.pattern.get(start + 1) != Some(&b'A') {
            let message = format!("argument {}: expected one argument", opt.strings.join("/"));
            return Err(self.parser.error(&message));
        }
        Ok(self.args[start + 1].clone())
    }

    fn take(&mut self, index: usize, values: Vec<String>) -> Result<(), Exit> {
        let opt = &self.parser.options[index];
        match opt.kind {
            Kind::Help => Err(Exit {
                status: 0,
                stdout: self.parser.help.to_owned(),
                stderr: String::new(),
            }),
            Kind::Version => Err(Exit {
                status: 0,
                stdout: format!("{}\n", self.version),
                stderr: String::new(),
            }),
            Kind::Flag => {
                if !self.namespace.flag(opt.dest) {
                    self.namespace.flags.push(opt.dest);
                }
                Ok(())
            }
            Kind::Value(check) => {
                let value = check(&values[0]).map_err(|message| {
                    self.parser
                        .error(&format!("argument {}: {message}", opt.strings.join("/")))
                })?;
                self.namespace.values.insert(opt.dest, value);
                Ok(())
            }
        }
    }
}
