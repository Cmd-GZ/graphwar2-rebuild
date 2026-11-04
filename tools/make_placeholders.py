#!/usr/bin/env python3
"""为"只有 .import 没有源文件"的资源生成占位文件。

导出态资源包里只有 `.godot/imported/*.ctex` 和 `*.import`，
但场景按**文本路径**回退加载（如 `res://return.svg`），源码不在包内。
这个脚本补上同名占位，让场景能解析过去。

实测（godot-live）：补齐 149 个 SVG + 16 个 PNG 后，
`Resource file not found` 类错误全部消失。
"""
import base64, glob, os, sys

SVG = ('<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16">'
       '<rect width="16" height="16" fill="#888888"/></svg>')
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")


def main(root):
    made = {"svg": 0, "png": 0}
    for imp in glob.glob(os.path.join(root, "**", "*.svg.import"), recursive=True):
        base = imp[: -len(".import")]
        if not os.path.exists(base):
            open(base, "w").write(SVG); made["svg"] += 1
    for imp in glob.glob(os.path.join(root, "**", "*.png.import"), recursive=True):
        base = imp[: -len(".import")]
        if not os.path.exists(base):
            open(base, "wb").write(PNG); made["png"] += 1
    print("生成占位: %d 个 svg, %d 个 png" % (made["svg"], made["png"]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "."))
