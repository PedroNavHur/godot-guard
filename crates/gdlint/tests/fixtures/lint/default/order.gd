extends Node
## docstring after extends
@tool
signal s
@export var exported = 1
var pub = 1
@warning_ignore("unused_parameter")
var _prv = 2
@onready var ready_pub = 3
static var late_static = 4
const LATE = 5
@export_group("Group")
@onready @export var both = 6
class Inner:
	var x
	signal late
func _init():
	pass
