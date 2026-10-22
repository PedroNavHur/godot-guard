@tool
extends Node
# File header block: the first line of prose,
# the second line of prose,
# and a third that the header exemption allows.

## A doc comment may run long:
## one, two, three, four lines of documentation
## without counting as a comment run.
## Still a doc comment.

# Two lines of prose
# are within the cap.
var a := 1

# One line of prose
# https://example.com/docs/thing
# ---
# gdlint: disable=max-line-length
# Run: godot --headless --script res://tools/x.gd
# ENG-123
#
var b := 2

# First prose line,
# second prose line,
# third prose line: over the cap.
var c := 3  # a trailing comment is exempt
var d := 4  # even when it continues
var e := 5  # onto a third line
