#!/usr/bin/env python3
"""Check the exact bare-WASM view import and function-export contract, the
signatures included: what `crates/sdk/view-wire/src/abi.rs` documents."""
import pathlib
import re
import subprocess
import sys

# export name -> (params, results), as `wasm-tools print` spells a func type
expected_exports = {
    "alloc": ("i32", "i32"),
    "init": ("", ""),
    "tick": ("i32 i32", "i64"),
    "snapshot": ("", "i64"),
    "restore": ("i32 i32", "i64"),
}


def signatures(text):
    """Each function export's (params, results), by its type index."""
    types = {}
    for index, body in re.findall(r"^\s*\(type \(;(\d+);\) \(func(.*)\)\)$", text, re.M):
        params = " ".join(re.findall(r"\(param ([^)]*)\)", body))
        results = " ".join(re.findall(r"\(result ([^)]*)\)", body))
        types[index] = (params, results)
    # a function is named in a debug build and only numbered in a stripped one
    by_name, by_index = {}, {}
    for name, index, kind in re.findall(r"^\s*\(func (?:\$(\S+) )?\(;(\d+);\) \(type (\d+)\)", text, re.M):
        by_index[index] = types.get(kind)
        if name:
            by_name[name] = types.get(kind)
    found = {}
    for export, name, index in re.findall(r'\(export "([^"]+)" \(func (?:\$(\S+)|(\d+))\)\)', text):
        found[export] = by_name.get(name) if name else by_index.get(index)
    return found


failed = False
for argument in sys.argv[1:]:
    path = pathlib.Path(argument)
    result = subprocess.run(["wasm-tools", "print", str(path)], capture_output=True, text=True)
    if result.returncode:
        print(f"{path.name}: wasm-tools exit={result.returncode}: {result.stderr[:200]}")
        failed = True
        continue
    imports = re.findall(r'\(import "([^"]+)" "([^"]+)"', result.stdout)
    exports = signatures(result.stdout)
    good = imports == [("ducktape_view", "panicked")] and exports == expected_exports
    print(f"{path.name}: {'PASS' if good else 'FAIL'}; imports={len(imports)}, function_exports={len(exports)}")
    if not good:
        print(f"  imports: {imports}")
        for name in sorted(set(exports) | set(expected_exports)):
            if exports.get(name) != expected_exports.get(name):
                print(f"  {name}: found {exports.get(name)}, expected {expected_exports.get(name)}")
        failed = True
sys.exit(1 if failed else 0)
