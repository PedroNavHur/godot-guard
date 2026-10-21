//! Matcher expectations, each checked against Python's `regex` on the pattern lark compiles.

use super::*;

fn m(matcher: Matcher, text: &str) -> Option<&str> {
    match_at(matcher, text).map(|len| &text[..len])
}

#[test]
fn strings_stop_at_the_first_unescaped_quote() {
    assert_eq!(
        m(Matcher::RegularString, r#""a\\" + "b""#),
        Some(r#""a\\""#)
    );
    assert_eq!(m(Matcher::RegularString, r#""a\"b" x"#), Some(r#""a\"b""#));
    assert_eq!(m(Matcher::RegularString, r#""\\\"" x"#), Some(r#""\\\"""#));
    assert_eq!(m(Matcher::RegularString, "\"\" x"), Some("\"\""));
    assert_eq!(m(Matcher::RegularString, "\"\"\"doc\"\"\""), None);
    assert_eq!(m(Matcher::RegularString, "\"a\nb\""), None);
    assert_eq!(m(Matcher::RegularRString, "R'x' y"), Some("R'x'"));
    assert_eq!(
        m(Matcher::LongString, "'''a\n'b'''' x"),
        Some("'''a\n'b'''")
    );
    assert_eq!(
        m(Matcher::LongRString, "r\"\"\"\\\"\"\"\"\"\" x"),
        Some("r\"\"\"\\\"\"\"\"")
    );
}

#[test]
fn numbers_take_lark_alternatives_in_order() {
    assert_eq!(m(Matcher::Number, "1e5.5"), Some("1e5"));
    assert_eq!(m(Matcher::Number, "1.e5x"), Some("1.e5"));
    assert_eq!(m(Matcher::Number, "1.x"), Some("1."));
    assert_eq!(m(Matcher::Number, "-.5e"), Some("-.5"));
    assert_eq!(m(Matcher::Number, "1_000e+"), Some("1_000"));
    assert_eq!(m(Matcher::Number, "+."), None);
    assert_eq!(m(Matcher::Hex, "-0x_1"), None);
    assert_eq!(m(Matcher::Bin, "0b01_2"), Some("0b01_"));
}

#[test]
fn type_hints_take_brackets_whole_or_not_at_all() {
    assert_eq!(m(Matcher::TypeHint, "Array[int] ="), Some("Array[int]"));
    assert_eq!(
        m(Matcher::TypeHint, "Dictionary[A.B , C . D]"),
        Some("Dictionary[A.B , C . D]")
    );
    assert_eq!(m(Matcher::TypeHint, "Array[int"), Some("Array"));
    assert_eq!(m(Matcher::TypeHint, "a . b"), Some("a . b"));
    assert_eq!(m(Matcher::TypeHint, "a .  b"), Some("a"));
    assert_eq!(m(Matcher::TypeHint, "a. 1"), Some("a"));
    assert_eq!(m(Matcher::TypeHint, "A[B, ]"), Some("A"));
}

#[test]
fn newlines_absorb_comments_and_indentation() {
    assert_eq!(m(Matcher::Newline, "# c\r\n\t x"), Some("# c\r\n\t "));
    assert_eq!(m(Matcher::Newline, "\r x"), None);
    assert_eq!(m(Matcher::LineCont, "\\ \x0c\r\nx"), Some("\\ \x0c\r\n"));
    assert_eq!(m(Matcher::Name, "héllo_1 x"), Some("héllo_1"));
    assert_eq!(m(Matcher::Name, "1a"), None);
}

const ALL: [Matcher; 14] = [
    Matcher::WsInline,
    Matcher::Hex,
    Matcher::Bin,
    Matcher::TypeHint,
    Matcher::Number,
    Matcher::LongString,
    Matcher::LongRString,
    Matcher::RegularString,
    Matcher::RegularRString,
    Matcher::Newline,
    Matcher::Comment,
    Matcher::LineCont,
    Matcher::Name,
    Matcher::Anything,
];

/// The lexer skips a terminal whose `can_start` rejects the first byte, so that must never hide a match.
#[test]
fn can_start_admits_every_match() {
    let suffixes = [
        "", "a", "1", "x1", "b0", "\"\"\"", "'''", "\"x\"", "'x'", "\n", " ", ".5", "e1", "ok]",
    ];
    let firsts = (0u8..128).map(char::from).chain(['é', '日', '\u{37a}']);
    for first in firsts {
        for suffix in suffixes {
            let text = format!("{first}{suffix}");
            let class = (text.as_bytes()[0] as usize).min(NON_ASCII);
            for matcher in ALL {
                if match_at(matcher, &text).is_some() {
                    assert!(can_start(matcher, class), "{matcher:?} matches {text:?}");
                }
            }
        }
    }
}
