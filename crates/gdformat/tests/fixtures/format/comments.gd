var d = {
	# standalone inside a dict forces it onto lines
	"a": 1, # after a key
	"b": [1, 2], # after the last
}
func f():
# column zero inside a block
	var x = call(a, # inline after an argument
		b)
	return x  # trailing  form feed: lark sees one line, splitlines two
