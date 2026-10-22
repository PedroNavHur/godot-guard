extends Node

@export var _hidden := 1
@export_group("Group")
var _grouped := 2
@onready @export var _late: Node
@export var shown := 3
var _plain := 4


func _process(_delta: float) -> void:
	var a := $Path/To
	var b := %Unique
	var c := get_node("X")
	var d := get_tree().get_nodes_in_group(&"units")
	var e := load("res://e.tres")
	var f := ResourceLoader.load("res://f.tres")
	var g := Thing.new()
	var h := [1, 2]
	var later := func(): return get_node("Y")
	shown = shown
	self.shown = self.shown
	shown += shown
	var m := $%Name


func _unhandled_input(_event: InputEvent) -> void:
	var keys := [KEY_ENTER]
	var n := find_child("N")


func _ready() -> void:
	var cached := get_node("Z")
	var data := load("res://d.tres")


class Inner:
	@export var _inner := 5

	func _physics_process(_delta: float) -> void:
		var r := self.get_node_or_null("R")

@export_storage var _stored := 5
