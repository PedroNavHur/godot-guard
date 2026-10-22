"""Record how 0.4.0's argparse reads each argument list, for crates/godot-guard/tests/cli.rs.

Usage: python argv_cases.py > crates/godot-guard/tests/data/argv.jsonl (with the oracle's godot-guard installed).
"""

import contextlib
import io
import json

from godot_guard.cli import build_parser

cases = [
 [], ["--version"], ["-h"], ["--help"], ["check","-h"], ["baseline","--help"], ["format","-h"], ["run","-h"], ["gate","-h"], ["init","-h"],
 ["nope"], ["--bogus"], ["check","--bogus"], ["check","a","--json","b"], ["check","--json","a","b"], ["check","a","b","--json"],
 ["check","--js","x"], ["check","--json=1"], ["check","--","--json"], ["check","-x"], ["check","--json","--json"],
 ["gate","--jobs","0"], ["gate","--jobs"], ["gate","--jobs=3"], ["gate","--jo","auto"], ["gate","--j","2"], ["gate","--f"], ["gate","--s","--fast","a.gd"],
 ["gate","a.gd","--fast","b.gd"], ["gate","--jobs","2.5"], ["gate","--jobs","-1"], ["gate","--jobs","--fast"],
 ["run"], ["run","--","--headless"], ["run","--headless","--version"], ["run","a","-h"], ["run","--version"], ["run","-h","x"],
 ["baseline","--i"], ["baseline","--init","--force","x"], ["baseline","--"], ["init","x"], ["--version","check"], ["check","--version"],
 ["-v"], ["--ver"], ["--v"], ["--h"], ["--he"], ["check","--h"], ["gate","--"], ["gate","--","--fast"], ["check","-"], ["check","-","--json"],
 ["format","--","a","--b"], ["check","--json","--","-x"], ["gate","--jobs","auto","--jobs","3"], ["gate","--fast=1"],["check","--json","a","--","b"],
 ["run","--","--","x"], ["gate","-j","2"], ["--","check"], ["check","--j"],
 ["-hx"], ["-hhx"], ["-hh"], ["--=x"], ["check","--=x"], ["gate","--jobs="], ["gate","--jobs","03"], ["-x"], ["check","-hh"],
 ["gate","--jo=auto"], ["gate","--jobs","auto","--bogus","x"], ["gate","--jobs","0","--bogus"], ["check","x","--bogus","y"], ["run","-x","--","y"],
 ["format","-"], ["check","-1"], ["gate","--jobs","1 "], ["check","a b"], ["check","-a b"], ["gate","--jobs","\u00b2"], ["check","--json","-h"],
]
for argv in cases:
    out, err = io.StringIO(), io.StringIO()
    code = None; ns = None
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            ns = vars(build_parser().parse_args(argv))
        except SystemExit as e:
            code = e.code
    print(json.dumps({"argv": argv, "ns": ns, "exit": code, "out": out.getvalue(), "err": err.getvalue()}))
