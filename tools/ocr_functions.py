#!/usr/bin/env python3
"""从游戏录屏里批量提取 AI 生成的函数文本（docs/68 §5）。

用法:
    python3 tools/ocr_functions.py <video.mp4> [--step 4] [--out .rea-work/funcs.txt]

依赖: ffmpeg（抽帧）、tesseract + eng.traineddata（识别）。
本机若缺 eng.traineddata，脚本会明确报错而不是静默产出空结果。
"""
import argparse
import collections
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

from PIL import Image, ImageOps

# 函数文本在画面底部：1728x1080 下大约 y∈[940, 1060]
CROP_TOP_FRAC = 0.87
CROP_BOT_FRAC = 0.99
CROP_LEFT_FRAC = 0.20   # 左侧 "f(x) =" 要排除
CROP_RIGHT_FRAC = 0.86  # 右侧修正值 "+ 3.109" 要排除
THRESHOLD = 140

# 保留的字符（数学表达式 + 函数名）
WHITELIST = "0123456789.xX+-*/()abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ^ "


def check_tesseract() -> None:
    if not shutil.which("tesseract"):
        sys.exit("缺少 tesseract")
    out = subprocess.run(["tesseract", "--list-langs"], capture_output=True, text=True)
    langs = out.stdout + out.stderr
    if "eng" not in langs.split():
        sys.exit(
            "tesseract 缺少 eng 语言包。Arch: sudo pacman -S tesseract-data-eng\n"
            f"当前可用: {langs.strip()}"
        )


def extract_frames(video: str, step: int, outdir: str) -> int:
    cmd = [
        "ffmpeg", "-v", "error", "-i", video,
        "-vf", f"fps=1/{step}", "-q:v", "2",
        os.path.join(outdir, "%05d.png"),
    ]
    subprocess.run(cmd, check=True)
    return len([f for f in os.listdir(outdir) if f.endswith(".png")])


def preprocess(path: str) -> Image.Image:
    im = Image.open(path).convert("L")
    w, h = im.size
    top = int(h * CROP_TOP_FRAC)
    bot = int(h * CROP_BOT_FRAC)
    c = im.crop((int(w * CROP_LEFT_FRAC), top, int(w * CROP_RIGHT_FRAC), bot))
    c = ImageOps.invert(c).point(lambda v: 0 if v < THRESHOLD else 255)
    # 放大 2 倍再轻微加粗，tesseract 对这种手写风字体更稳
    c = c.resize((c.width * 2, c.height * 2), Image.LANCZOS)
    return c


def ocr(image: Image.Image) -> str:
    with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
        tmp = f.name
    try:
        image.save(tmp)
        out = subprocess.run(
            ["tesseract", tmp, "stdout", "--psm", "7", "-l", "eng",
             "-c", f"tessedit_char_whitelist={WHITELIST}"],
            capture_output=True, text=True, timeout=60,
        )
        return out.stdout.strip()
    finally:
        os.unlink(tmp)


# 明显的"非 AI"特征：用户手打的超大常数 / sigmoid 叠加
USER_PATTERNS = [
    re.compile(r"\d{12,}"),                     # 99999999999999x
    re.compile(r"[eE]\(?-?\s*9{3,}"),           # e^(-999x) 这类阶跃（OCR 常丢掉 ^）
    re.compile(r"9{3,}\s*[(*]"),                # 999( 或 999*
]


def cleanup(t: str) -> str:
    """去掉 OCR 噪声：空格、前缀垃圾、尾部的竖线与修正值。"""
    t = t.strip()
    t = t.split("|")[0]                 # 右侧修正值在竖线之后
    t = t.replace(" ", "")
    # 丢掉第一个"像表达式开头"之前的字符
    m = re.search(r"[(\-0-9.]", t)
    if m:
        t = t[m.start():]
    # 去掉首尾不平衡的括号
    while t and t.count(")") > t.count("(") and t.endswith(")"):
        t = t[:-1]
    return t


def looks_like_function(t: str) -> bool:
    if len(t) < 2:
        return False
    if "x" not in t.lower() and not re.fullmatch(r"[-+0-9. ]+", t):
        return False
    # 括号要基本配平
    return abs(t.count("(") - t.count(")")) <= 1


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("video")
    ap.add_argument("--step", type=int, default=4)
    ap.add_argument("--out", default=".rea-work/funcs.txt")
    args = ap.parse_args()

    check_tesseract()
    with tempfile.TemporaryDirectory() as d:
        n = extract_frames(args.video, args.step, d)
        print(f"抽帧 {n} 张（每 {args.step} 秒一张）", file=sys.stderr)
        results = []
        for i, name in enumerate(sorted(os.listdir(d)), 1):
            if not name.endswith(".png"):
                continue
            try:
                text = ocr(preprocess(os.path.join(d, name)))
            except Exception as e:  # 单帧失败不影响整体
                print(f"  {name}: {e}", file=sys.stderr)
                continue
            text = cleanup(text)
            if not looks_like_function(text):
                continue
            user_made = any(p.search(text) for p in USER_PATTERNS)
            # 用户那套 sigmoid 叠加会出现很多个 e( 项
            if not user_made and len(re.findall(r"[eE]\(", text)) >= 3:
                user_made = True
            results.append({"frame": name, "text": text, "user_made": user_made})
            if i % 50 == 0:
                print(f"  已处理 {i}/{n}", file=sys.stderr)

    os.makedirs(os.path.dirname(args.out) or ".", exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        for r in results:
            f.write(("USER\t" if r["user_made"] else "AI\t") + r["text"] + "\n")
    with open(args.out.replace(".txt", ".json"), "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=1)

    ai = [r for r in results if not r["user_made"]]
    uniq = collections.Counter(r["text"] for r in ai)
    print(f"\n识别到 {len(results)} 条，其中 AI 生成 {len(ai)} 条，去重 {len(uniq)} 条", file=sys.stderr)
    for t, c in uniq.most_common(15):
        print(f"  {c:3d}× {t}", file=sys.stderr)


if __name__ == "__main__":
    main()
