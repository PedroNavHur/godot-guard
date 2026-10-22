var nombre_ñ = "héllo wörld"	# tab before comment
var ünïcödé := 1 + \
	2
class Inner extends "res://base.gd":
	enum Named { A = 1, B, }
	signal changed(value: int)
	var p: int:
		set(value):
			p = value
		get:
			return p
	@export var q := 3
func m():
	match nombre_ñ:
		[1, var rest, ..]:
			pass
		{"k": _} when rest > 1:
			pass
