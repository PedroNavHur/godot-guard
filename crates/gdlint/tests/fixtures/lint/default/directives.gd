# gdlint: enable=function-name
var Early = 1  # gdlint: disable=class-variable-name
var Later = 2
func Bad_Name():  # gdlint:ignore = function-name , unused-argument
	pass
func Next_Line():
	pass
# gdlint: disable=function-name
func Off_Here():
	pass
# gdlint: enable=function-name
func On_Again():
	pass
