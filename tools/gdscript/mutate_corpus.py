"""Write damaged copies of corpus scripts, so the parser parity test reaches gdtoolkit's error paths.

Usage: python mutate_corpus.py OUT_DIR COUNT SEED FILE...

Each mutant is one source file with one random edit: a character deleted or inserted, a line indented,
dedented, duplicated or dropped, or the file cut short. Most mutants are rejected, through every error
lark and the indenter raise; the rest still parse, often into unusual trees. Then run the parity test
on them: GG_PARITY_DIRS=OUT_DIR cargo test -p gdscript --features oracle --test corpus_parity
"""

import random
import sys
from pathlib import Path

INSERTS = ["(", ")", "[", "]", "{", "}", '"', "'", '"""', "\t", " ", "    ", "\n", "\r", ":", ";", ",", "#",
           "\\", "\\\n", "$", "%", "@", "^", "&", "é", "?", "`", "->", "func():", "func():\n\t", "r", "0x",
           "1e", ".", "is not ", "not ", "await ", "\x00", "\x0c"]


def mutate(text, rng):
    lines = text.split("\n")
    kind = rng.randrange(7)
    if kind == 0 and text:
        at = rng.randrange(len(text))
        return text[:at] + text[at + 1:]
    if kind == 1:
        at = rng.randrange(len(text) + 1)
        return text[:at] + rng.choice(INSERTS) + text[at:]
    line = rng.randrange(len(lines))
    if kind == 2:
        lines[line] = rng.choice(["\t", " ", "  "]) + lines[line]
    elif kind == 3:
        lines[line] = lines[line][1:]
    elif kind == 4:
        lines.insert(line, lines[line])
    elif kind == 5:
        del lines[line]
    else:
        return text[: rng.randrange(len(text) + 1)]
    return "\n".join(lines)


def main():
    out_dir, count, seed, files = Path(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3]), sys.argv[4:]
    out_dir.mkdir(parents=True, exist_ok=True)
    rng = random.Random(seed)
    for index in range(count):
        source = Path(rng.choice(files)).read_text(encoding="utf-8", errors="replace")
        (out_dir / ("mutant_%05d.gd" % index)).write_bytes(mutate(source, rng).encode("utf-8"))


if __name__ == "__main__":
    main()
