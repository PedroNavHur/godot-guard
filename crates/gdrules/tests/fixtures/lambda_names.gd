var top = func(): return 1


func f():
	var a = func(): return func(): return 2
	var b = func named(): return 3
	var c = func(): return 4


class Inner:
	var d = func(): return 5
