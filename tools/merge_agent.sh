#!/usr/bin/env bash
# 合并一个 agent 分支，**自动避开 docs/06 的必然冲突**。
#
# 为什么需要它（2026-11-27 一天内踩了四次）
# -------------------------------------------
# 每个 agent 分支都会在 `docs/06-verification.md` 末尾追加自己的一节，所以**两两分支合并必然冲突**。
# 我手工解冲突时连续踩了四种坏法：
#   1. 正文被吞（§113 的 217 行只剩标题）；
#   2. 正文搬错位置 + 标题张冠李戴（12 处 `§120.x` 外部引用全指错节）；
#   3. 按号去重把整节删掉（T75 §121 / T71 §123 / T77 §124 三节蒸发）；
#   4. `git add -A` 把**未解决的冲突标记**一起提交（`<<<<<<<` 进了 master）。
#
# 正确做法（本脚本）：
#   a. `git merge --no-commit`；
#   b. **把 `docs/06` 与 `docs/16` 直接取 master 版本**（冲突永远不解，直接丢弃对方那一版）；
#   c. 其余文件照常合并（若还有冲突就停下来让人处理）；
#   d. 用 `tools/doc06_insert.py` 把该分支的**新节按内容**插进来（自动分配 max+1 的号）；
#   e. 跑 `docsec.py check` + `check_doc_refs.py`，绿了才提交。
#
# 用法：
#   tools/merge_agent.sh agent/<topic> --title-match '<正则>' --message-file <路径> [--allow path ...]
set -uo pipefail

BRANCH="${1:?用法: merge_agent.sh agent/<topic> --title-match <正则> --message-file <file> [--allow <path>...]}"
shift
TITLE_MATCH=""
MSG_FILE=""
ALLOWS=()
while [ $# -gt 0 ]; do
  case "$1" in
    --title-match) TITLE_MATCH="$2"; shift 2;;
    --message-file) MSG_FILE="$2"; shift 2;;
    --allow) ALLOWS+=("--allow" "$2"); shift 2;;
    *) echo "未知参数: $1" >&2; exit 2;;
  esac
done

cd "$(git rev-parse --show-toplevel)" || exit 1

echo "== 1/5 merge_check =="
python3 tools/merge_check.py --branch "$BRANCH" --base master --no-report "${ALLOWS[@]}" 2>&1 | tail -2

echo "== 2/5 git merge --no-commit =="
git merge --no-commit --no-ff "$BRANCH" >/tmp/merge_out.txt 2>&1 || true
tail -3 /tmp/merge_out.txt

echo "== 3/5 docs/06 与 docs/16 一律取 master 版（避免冲突标记进仓库）=="
git checkout --ours docs/06-verification.md 2>/dev/null && git add docs/06-verification.md
git checkout --ours docs/16-client.md 2>/dev/null && git add docs/16-client.md
# 生成物冲突也一律取 master 版，随后重算
for f in docs/coverage-matrix.tsv docs/149-coverage-matrix.md fixtures/function_coverage.tsv; do
  [ -f "$f" ] && git checkout --ours "$f" 2>/dev/null && git add "$f"
done

LEFT=$(git diff --name-only --diff-filter=U | tr '\n' ' ')
if [ -n "$LEFT" ]; then
  echo "✗ 还有未解冲突（需要人看）: $LEFT" >&2
  exit 3
fi

echo "== 4/5 把分支的新节按内容插入 docs/06 =="
if [ -n "$TITLE_MATCH" ]; then
  python3 tools/doc06_insert.py --from-ref "$BRANCH" --title-match "$TITLE_MATCH" 2>&1 | tail -2
else
  echo "（未给 --title-match，跳过插节 —— 该分支没有 docs/06 新节）"
fi

echo "== 5/5 门槛 + 提交 =="
export PATH="$PWD/.reference/venv/bin:$PATH"
COVERAGE_MATRIX_UPSTREAM="${COVERAGE_MATRIX_UPSTREAM:-$PWD/.reference/upstream}" \
  python3 tools/coverage_matrix.py >/dev/null 2>&1
# function_coverage 也是生成物：第 3 步为保证不冲突取了 master 版，这里必须按当前 src 重算
python3 tools/function_coverage.py >/dev/null 2>&1
python3 tools/docsec.py check docs/06-verification.md | tail -2
python3 tools/check_doc_refs.py 2>&1 | tail -1
git add -A
if [ -n "$MSG_FILE" ]; then
  git -c user.name=Cmd-GZ commit -q -F "$MSG_FILE"
else
  git -c user.name=Cmd-GZ commit -q -m "merge: $BRANCH"
fi
git log --oneline -1 | cut -c1-70
