#!/usr/bin/env python3
"""gen_posy_fixture.py —— 把 `tools/posy_batches.py` / `tools/posy_corpus.py` 的输出压成
**纯文本样本摘要**（`fixtures/effectinfo_posy_sample.jsonl`），入库用。

规矩（`AGENTS.md`）：只放数值与位模式，**不放路径/用户名/原始回放**。

用法：
    ../.venv/bin/python tools/gen_posy_fixture.py \
        --batches b8844.jsonl b8991.jsonl --corpus corpus.jsonl \
        --out fixtures/effectinfo_posy_sample.jsonl
"""

import argparse
import json
import struct


BUILD = ("libgraphwarrust.so sha256 "
         "9a53811518e74b7575c1b9dd1f21cdd1f4f015bfdcd8eaeee4dfdf0bcabed187 (steam/2026-08-28)")


def bits(x):
    try:
        return struct.pack("<d", float(x)).hex()
    except Exception:
        return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--batches", nargs="*", default=[])
    ap.add_argument("--corpus", default=None)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    lines = []
    lines.append({
        "type": "provenance",
        "schema": "gw2-effectinfo-posy-sample/1",
        "build": BUILD,
        "what": ("每发：真二进制（真解析器 0x4ed670 → 真采样器 0x3c2530 → 末点；真求值器 0x4eb9d0 → "
                 "f(报文 pos_x)）与报文 `FunctionPoints.end_x` / `EffectInfo(Explosion).pos_*` 的**逐位**对照。"
                 "写入指令见 docs/06 §35：`gw+0x4729a0` / `gw+0x472e14` 的 16 字节 movdqu（逐字节拷贝 pts[n-1]）。"),
        "how": ("tools/posy_batches.py --replay <gunzip 后的回放 JSONL>（一步一发，step=0.01，"
                "flip 按队伍，mode 按 FunctionModeInfo）；tools/posy_corpus.py 做语料普查。"),
        "note": ("`last_y_ulp_vs_wire` = 真二进制末点 y − 报文 pos_y（ulp，正=我方更大）；"
                 "`f_ulp_vs_wire` = f(报文 pos_x) − 报文 pos_y（ulp）；0 表示逐位相同。"
                 "两列都为 0 才说明报文那个 pos 既等于采样器末点、又落在曲线上。"),
    })
    # 文件名 -> 回放 id（不把路径写进样本）
    SERIES = {"b8844": "1794158844", "b8991": "1794158991"}
    for path in a.batches:
        stem = path.rsplit("/", 1)[-1].split(".")[0]
        series = SERIES.get(stem, stem)
        with open(path) as f:
            for line in f:
                r = json.loads(line)
                row = {
                    "type": "batch",
                    "series": series,
                    "batch": r.get("batch"),
                    "owner": r.get("owner"),
                    "team": (r.get("team") or "")[-1:],
                    "flip": r.get("flip"),
                    "mode": r.get("mode"),
                    "n_points": r.get("n"),
                    "end_x_bits": bits(r.get("end_x")),
                    "last_x_bits": bits(r.get("last_x")),
                    "last_y_bits": bits(r.get("last_y")),
                    "wire_pos_x_bits": bits(r.get("wire_pos_x")),
                    "wire_pos_y_bits": bits(r.get("wire_pos_y")),
                    "f_at_pos_x_bits": bits(r.get("f_at_pos_x")),
                }
                # ulp 差
                def ulp(u, v):
                    if u is None or v is None:
                        return None
                    iu = struct.unpack("<q", struct.pack("<d", float(u)))[0]
                    iv = struct.unpack("<q", struct.pack("<d", float(v)))[0]
                    return iu - iv
                row["last_x_ulp_vs_wire"] = ulp(r.get("last_x"), r.get("end_x"))
                row["last_y_ulp_vs_wire"] = ulp(r.get("last_y"), r.get("wire_pos_y"))
                row["f_ulp_vs_wire"] = ulp(r.get("f_at_pos_x"), r.get("wire_pos_y"))
                lines.append(row)
    if a.corpus:
        rows = [json.loads(l) for l in open(a.corpus)]
        ok = [r for r in rows
              if abs(r["wire_pos_x"] - r["end_x"]) <= 1e-12 * max(1, abs(r["end_x"]))]
        on = [r for r in ok if r["on_curve"]]
        small = [r for r in ok if (not r["on_curve"]) and abs(r["ulp"]) <= 2]
        lines.append({
            "type": "corpus",
            "scope": "Mode::Normal 且有 ExplosionEffect、pos_x≈end_x（配对可信）",
            "shots_comparable": len(ok),
            "pos_on_curve_bitwise": len(on),
            "pos_off_curve": len(ok) - len(on),
            "pos_off_curve_within_2ulp": len(small),
            "pos_x_equals_end_x_bitwise": sum(
                1 for r in ok if struct.pack("<d", r["wire_pos_x"]) == struct.pack("<d", r["end_x"])),
            "pm1ulp_counts": {
                "+1": sum(1 for r in ok if (not r["on_curve"]) and r["ulp"] == 1),
                "-1": sum(1 for r in ok if (not r["on_curve"]) and r["ulp"] == -1),
            },
        })
    with open(a.out, "w") as f:
        for r in lines:
            f.write(json.dumps(r, sort_keys=True, ensure_ascii=False) + "\n")
    print("写出 %s（%d 行）" % (a.out, len(lines)))


if __name__ == "__main__":
    main()
