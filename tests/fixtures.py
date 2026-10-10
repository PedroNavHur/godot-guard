"""GDScript fixtures, kept as strings so repository scanners do not score the deliberate violations."""

COMPLEXITY = """\
extends Node

var health: int = 0:
	set(value):
		if value < 0 and health > 0 or value > 100:
			health = 0
		else:
			health = value
	get:
		return health


func chain(x: int) -> String:
	if x == 0:
		return "zero"
	elif x == 1:
		return "one"
	elif x == 2:
		return "two"
	else:
		return "many"


func matcher(x: int) -> int:
	match x:
		1:
			return 1
		2 when x > 0:
			return 2
		_ when x < 0:
			return 3
		_:
			return 4


func with_lambda(items: Array) -> Array:
	return items.filter(func(item): return item > 0 and item < 10 if item else false)


func loops(items: Array) -> int:
	var total := 0
	for item in items:
		while total < item:
			if item % 2 == 0:
				total += 1
	return total


func recurse(n: int) -> int:
	if n <= 0:
		return 0
	return recurse(n - 1) + self.recurse(n - 2)


class Inner:
	func helper(a: bool, b: bool, c: bool) -> bool:
		return a and b and c or not (a or b)
"""

# name: (line, cognitive, cyclomatic); the reference scorer agrees on each.
COMPLEXITY_SCORES = {
    "health.set": (4, 4, 4),
    "health.get": (9, 0, 1),
    "chain": (13, 4, 4),
    "matcher": (24, 3, 5),
    "with_lambda": (36, 3, 1),
    "<lambda>": (37, None, 3),
    "loops": (40, 6, 4),
    "recurse": (49, 2, 2),
    "Inner.helper": (56, 3, 5),
}

COMMENTS = """\
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
"""

NESTED_TERNARY = """\
extends Node


func pick(a: bool, b: bool) -> int:
	var flat := 1 if a else 2
	var chained := 1 if a else 2 if b else 3
	var wrapped := (1 if a else 2) if b else 3
	var in_condition := 1 if (a if b else false) else 2
	return flat + chained + wrapped + in_condition
"""

RESOURCE_BASE = """\
class_name ItemData
extends Resource

@export var name: String
"""

RESOURCE_CHILD = """\
class_name WeaponData
extends ItemData

@export var damage: int
"""

RESOURCE_GRANDCHILD = """\
extends WeaponData

@export var a: int
@export var b: int
@export var c: int
@export var d: int
@export var e: int
@export var f: int
@export var g: int
@export var h: int
@export var i: int
"""

WIDE_NODE = """\
extends Node

signal opened
signal closed

@export_group("Look")
@export var color: Color
@export var size: int
@export_range(0, 10) var level: int
@export
var late: int
@onready var label: Label = $Label
var plain := 0


func _init(a: int, b: Dictionary = {"x": 1, "y": 2}) -> void:
	pass


func setup(first: int, second: String, third := 3) -> void:
	pass
"""

BOUNDARY = """\
extends RefCounted

# A Sprite3D mention in a comment is fine.
var path := "res://scenes/battle.tscn"
var sprite: Sprite3D
"""
