#!/usr/bin/env bash
# 开工前自检：把"会被浪费掉的整轮工作"挡在门口。见 docs/90。
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
fail=0; note() { printf '  %s\n' "$1"; }
echo "== 1. 全量测试 =="
export CARGO_HOME="${CARGO_HOME:-$PWD/../.cargo-home}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/../.cargo-home/target}"
res=$(timeout 600 cargo test --offline 2>&1 | grep -E "test result" | awk '{s+=$4; f+=$6} END {print s" "f}')
ps=${res%% *}; fs=${res##* }
note "通过 $ps 失败 $fs"
[ "${fs:-1}" != "0" ] && { echo "  ✗ 测试不绿，禁止开始新分析"; fail=1; }

echo "== 2. 硬编码步长扫描 =="
bad=$(grep -rnE "shot_points\([^)]*, 0\.5[,)]|calculate\([^)]*, 0\.5[,)]|step: 0\.5" examples tests src 2>/dev/null | grep -v SERVER_STEP || true)
if [ -n "$bad" ]; then note "✗ 发现硬编码 0.5："; printf '%s\n' "$bad" | sed 's/^/      /'; fail=1; else note "✓ 无硬编码 0.5"; fi

echo "== 3. SERVER_STEP 与权威表一致 =="
if grep -q "pub const SERVER_STEP: f64 = 0.01;" src/lib.rs && grep -q "0.01\` on the server" ../AGENTS.md 2>/dev/null; then
  note "✓ SERVER_STEP=0.01，与 AGENTS.md Hard facts 一致"
else
  note "✗ SERVER_STEP 与 AGENTS.md 不一致（先查权威表，别再'发现'一次）"; fail=1
fi

echo "== 4. 已知早停 canary（必须报告，不静默） =="
if [ -x examples/rep.rs ] || [ -f examples/rep.rs ]; then
  out=$(timeout 300 cargo run --release --offline --example rep 2>/dev/null | grep "^step")
  printf '%s\n' "$out" | sed 's/^/      /'
  # 修复后的正确行为：step=0.5 → 401 点，step=0.01 → 20001 点
  if printf '%s' "$out" | grep -q "npts=20001"; then
    note "✓ 早停已修复（npts=20001，与真二进制一致）"
  else
    note "⚠ 早停 canary 异常：期望 npts=20001（见 docs/91）。物理结论不可信，先查这个"
  fi
fi

echo "== 5. 工作区卫生 =="
dirty=$(git status --porcelain | wc -l)
note "未提交改动 ${dirty} 个"
[ "$dirty" != "0" ] && note "⚠ 有未提交改动：确认不是实验残留（docs/90 失败模式 #4）"

echo "== 6. 功能台账 =="
scn=$(find rsc -maxdepth 2 -name "*.scn" 2>/dev/null | wc -l)
gdc=$(find rsc -name "*.gdc" 2>/dev/null | wc -l)
note "解包资源：${scn} 个 .scn、${gdc} 个 .gdc（台账见 docs/88 §10-11）"

echo
[ "$fail" = "0" ] && echo "PREFLIGHT: PASS" || echo "PREFLIGHT: FAIL"
exit "$fail"
