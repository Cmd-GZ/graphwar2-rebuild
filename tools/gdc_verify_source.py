#!/usr/bin/env python3
"""Verify GDRE-decompiled GDScript against the shipped `.gdc` bytecode, token by token.

Why: once a decompiler exists (`docs/104`), the question "can I trust its output?"
becomes answerable instead of rhetorical. The `.gdc` carries an exact token stream
(`tools/gdc_disasm.py` §3 of `docs/101`); the decompiled `.gd` is only a *rendering*
of that stream. So: lex the `.gd`, map both sides to the same (kind, value) sequence,
and require them to be identical. A match means the decompiler neither invented nor
dropped a token — formatting (indentation, blank lines, `- 3` vs `-3`) may differ.

Found real differences are reported, never smoothed over:
  * `-3` — the compiler folds a negative literal into one constant; source text has
    `-` + `3`, so the verifier folds unary minus back before comparing;
  * triple-quoted strings — handled, because the AdMob addon uses them.

Usage:
    tools/gdc_verify_source.py --gdc ../rsc --gd <recovered>/        # whole tree
    tools/gdc_verify_source.py --gdc ../rsc --gd <recovered>/ --only CampaingLevelsButtons
    tools/gdc_verify_source.py --gdc ../rsc --gd <recovered>/ --json out.json

Exit code 0 when every pair matches, 1 otherwise.

The recovered `.gd` files are game assets: this tool only reads them, and the repo
must never contain them (`docs/104` §2).
"""
import argparse
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gdc_disasm as G                                   # noqa: E402


# ---------------------------------------------------------------------------
# lexer for the decompiled source (a rendering of the same token stream)
# ---------------------------------------------------------------------------
REV = {}
for _t, _txt in G.TOKEN_NAMES.items():
    if _t in (G.IDENT, G.LITERAL, G.ANNOTATION):
        continue
    REV.setdefault(_txt, _t)
OPS = sorted([k for k in REV if k and k[0] not in "abcdefghijklmnopqrstuvwxyzPITN_"],
             key=len, reverse=True)
NUM_RE = re.compile(r'0x[0-9a-fA-F_]+|0b[01_]+|\d[\d_]*\.\d*(?:[eE][-+]?\d+)?'
                    r'|\.\d+(?:[eE][-+]?\d+)?|\d[\d_]*(?:[eE][-+]?\d+)?')
STR_RE = re.compile(r'"((?:[^"\\]|\\.)*)"')
ID_RE = re.compile(r'[A-Za-z_][A-Za-z0-9_]*')


def _unescape(s):
    out, i = [], 0
    table = {"n": "\n", "t": "\t", "r": "\r", '"': '"', "\\": "\\", "'": "'"}
    while i < len(s):
        c = s[i]
        if c == "\\" and i + 1 < len(s):
            n = s[i + 1]
            if n == "u" and i + 5 < len(s) + 1:
                try:
                    out.append(chr(int(s[i + 2:i + 6], 16)))
                    i += 6
                    continue
                except ValueError:
                    pass
            out.append(table.get(n, n))
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


def lex(src):
    toks, i, n = [], 0, len(src)
    while i < n:
        c = src[i]
        if c in " \t\r\n":
            i += 1
            continue
        if c == "#":
            while i < n and src[i] != "\n":
                i += 1
            continue
        if src.startswith('"""', i) or src.startswith('&"""', i) or src.startswith('^"""', i):
            pre = "" if src[i] == '"' else src[i]
            start = i + len(pre)
            end = src.find('"""', start + 3)
            if end < 0:
                raise ValueError("unterminated triple-quoted string at %d" % i)
            body = src[start + 3:end]
            toks.append(("str", pre + '"' + body.replace("\\", "\\\\")
                         .replace('"', '\\"') + '"'))
            i = end + 3
            continue
        if c == '"' or (c in "&^" and i + 1 < n and src[i + 1] == '"'):
            pre = "" if c == '"' else c
            if pre:
                i += 1
            m = STR_RE.match(src, i)
            if not m:
                raise ValueError("bad string at %d" % i)
            toks.append(("str", pre + m.group(0)))
            i = m.end()
            continue
        if c.isdigit() or (c == "." and i + 1 < n and src[i + 1].isdigit()):
            m = NUM_RE.match(src, i)
            if not m:
                raise ValueError("bad number at %d" % i)
            toks.append(("num", m.group(0)))
            i = m.end()
            continue
        m = ID_RE.match(src, i)
        if m:
            w = m.group(0)
            i = m.end()
            toks.append(("op", w) if w in REV else ("id", w))
            continue
        if c == "@":
            m = re.match(r'@[A-Za-z_][A-Za-z0-9_]*', src[i:])
            if m:
                toks.append(("ann", m.group(0)))
                i += m.end()
                continue
        hit = next((op for op in OPS if src.startswith(op, i)), None)
        if hit is None:
            raise ValueError("cannot lex %r at %d" % (src[i:i + 20], i))
        toks.append(("op", hit))
        i += len(hit)
    return toks


def _numval(t):
    t = t.replace("_", "")
    if t.startswith("0x"):
        return int(t, 16)
    if t.startswith("0b"):
        return int(t, 2)
    return float(t) if ("." in t or "e" in t.lower()) else int(t)


def _fold_unary_minus(seq):
    out = []
    for tok in seq:
        if (tok[0] == "lit" and tok[1][0] in ("int", "float") and out
                and out[-1] == ("op", "-")
                and (len(out) == 1 or (out[-2][0] == "op"
                                       and out[-2][1] not in (")", "]", "}")))):
            out.pop()
            out.append(("lit", (tok[1][0], -tok[1][1])))
        else:
            out.append(tok)
    return out


def gdc_sequence(script):
    out = []
    for i in range(len(script.tokens)):
        t = script.token_type(i)
        op = script.token_operand(i)
        if t == G.IDENT:
            out.append(("id", script.identifiers[op]))
        elif t == G.ANNOTATION:
            out.append(("ann", script.identifiers[op]))
        elif t == G.LITERAL:
            out.append(("lit", script.constants[op]))
        else:
            out.append(("op", G.TOKEN_NAMES.get(t)))
    return out


def source_sequence(toks):
    out = []
    for kind, txt in toks:
        if kind == "id":
            if txt in ("true", "false"):
                out.append(("lit", ("bool", txt == "true")))
            elif txt == "null":
                out.append(("lit", ("nil", None)))
            else:
                out.append(("id", txt))
        elif kind == "ann":
            out.append(("ann", txt))
        elif kind == "op":
            out.append(("op", txt))
        elif kind == "str":
            pre = txt[0] if txt[0] in "&^" else ""
            body = txt[1:] if pre else txt
            out.append(("lit", ("stringname" if pre == "&" else
                                ("nodepath" if pre == "^" else "string"),
                                _unescape(body[1:-1]))))
        else:
            v = _numval(txt)
            out.append(("lit", ("int" if isinstance(v, int) else "float", v)))
    return _fold_unary_minus(out)


def _literal_equal(a, b):
    if a[0] != b[0]:
        return False
    if a[0] in ("int", "float"):
        return abs(float(a[1]) - float(b[1])) < 1e-12
    return a[1] == b[1]


def compare(gdc_path, gd_path):
    script = G.parse_gdc(gdc_path)
    with open(gd_path, "r", encoding="utf-8") as fh:
        src = fh.read()
    a = gdc_sequence(script)
    b = source_sequence(lex(src))
    if len(a) != len(b):
        return False, "token count %d (.gdc) vs %d (.gd)" % (len(a), len(b))
    for i, (x, y) in enumerate(zip(a, b)):
        if x[0] != y[0]:
            return False, "token %d: kind %s vs %s (%r vs %r)" % (i, x[0], y[0], x[1], y[1])
        if x[0] == "lit":
            if not _literal_equal(x[1], y[1]):
                return False, "token %d: literal %r vs %r" % (i, x[1], y[1])
        elif x[1] != y[1]:
            return False, "token %d: %r vs %r" % (i, x[1], y[1])
    return True, "%d tokens" % len(a)


def main(argv):
    ap = argparse.ArgumentParser(description="Verify decompiled .gd against .gdc bytecode")
    ap.add_argument("--gdc", required=True, help="directory with .gdc files (recursed)")
    ap.add_argument("--gd", required=True, help="directory with recovered .gd files")
    ap.add_argument("--only", help="substring filter on the relative script name")
    ap.add_argument("--json", metavar="OUT")
    a = ap.parse_args(argv)

    pairs = []
    for root, _d, files in os.walk(a.gdc):
        for f in sorted(files):
            if not f.endswith(".gdc"):
                continue
            gdc = os.path.join(root, f)
            rel = os.path.relpath(gdc, a.gdc)[:-4]
            if a.only and a.only not in rel:
                continue
            pairs.append((rel, gdc, os.path.join(a.gd, rel + ".gd")))
    pairs.sort()

    ok = 0
    results = []
    for rel, gdc, gd in pairs:
        if not os.path.exists(gd):
            results.append({"script": rel, "ok": False, "reason": "no .gd recovered"})
            print("!! %-58s missing .gd" % rel)
            continue
        try:
            good, msg = compare(gdc, gd)
        except Exception as exc:                     # one bad pair must not hide the rest
            good, msg = False, "error: %s" % exc
        results.append({"script": rel, "ok": good, "detail": msg})
        if good:
            ok += 1
        else:
            print("!! %-58s %s" % (rel, msg))
    print("verified %d/%d scripts token-for-token" % (ok, len(pairs)))
    if a.json:
        with open(a.json, "w", encoding="utf-8") as fh:
            json.dump({"total": len(pairs), "verified": ok, "results": results},
                      fh, ensure_ascii=False, indent=1)
        print("wrote %s" % a.json)
    return 0 if ok == len(pairs) else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
