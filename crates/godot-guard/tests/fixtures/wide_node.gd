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
