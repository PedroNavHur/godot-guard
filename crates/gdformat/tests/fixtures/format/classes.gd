class_name Thing extends "res://base.gd".Inner
signal changed(old_value: int, new_value: int)
signal empty()
enum { A, B }
enum Named { X = 1, Y = 2 << 1, }
const C := 1
static var count: int = 0
class Inner extends Node:
	var x = 1
	func f(): pass
static func make(a, ...rest) -> Thing:
	return null
func lambdas():
	var l = func(a: int, b := 2) -> int:
		var c = a + b
		return c
	var m = func named(): pass
