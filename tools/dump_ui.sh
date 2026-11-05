#!/usr/bin/env bash
# Dump the Graphwar II client UI from the live harness (one command, reproducible).
#
#   tools/dump_ui.sh                 # every route, output into .rea-work/ui/
#   tools/dump_ui.sh boot play       # selected routes
#   GW2_UI_OUT=dir tools/dump_ui.sh  # custom output dir
#
# How: godot-live/probe.gd (our autoload) is swapped for tools/dump_ui.gd for the
# duration of the run and restored afterwards, then one headless Godot process per
# route is executed. Each route is a scripted press sequence ending in tree dumps.
#
# Requires the game assets in godot-live/ (see docs/22) and the Godot 4.7.2 binary
# in ../tools/ (workspace root).
set -u

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REBUILD="$(cd "$HERE/.." && pwd)"
WS="$(cd "$REBUILD/.." && pwd)"
LIVE="$WS/godot-live"
GODOT="$WS/tools/Godot_v4.7.2-stable_linux.x86_64"
OUT="${GW2_UI_OUT:-$WS/.rea-work/ui}"
ALL_ROUTES="boot campaign player replay lobby setup addbot setup_bots joinip setup_cycle play play_diffeq play_extra play_chat play_exit connect_fail connect_room connect_ip_port connect_refused endgame endgame6"

if [ ! -x "$GODOT" ]; then echo "missing godot: $GODOT" >&2; exit 1; fi
if [ ! -d "$LIVE" ]; then echo "missing harness: $LIVE" >&2; exit 1; fi
mkdir -p "$OUT"

ROUTES="${*:-$ALL_ROUTES}"

# project.binary (the shipped, compiled project settings) overrides project.godot,
# which would drop our autoload. docs/22 §1 case 3.
rm -f "$LIVE/project.binary"
# Back up the harness probe ONCE, and never back up our own dumper (a run that was
# killed before its trap fired would otherwise poison the backup).
MARK="Runtime UI inventory dumper"
if ! grep -q "$MARK" "$LIVE/probe.gd" 2>/dev/null; then
  cp "$LIVE/probe.gd" "$OUT/probe.gd.bak"
fi
cp "$HERE/dump_ui.gd" "$LIVE/probe.gd"
restore() {
  if [ -f "$OUT/probe.gd.bak" ] && ! grep -q "$MARK" "$OUT/probe.gd.bak"; then
    cp "$OUT/probe.gd.bak" "$LIVE/probe.gd"
  fi
}
trap restore EXIT INT TERM

export XDG_DATA_HOME="$WS/.godot-data"
export XDG_CACHE_HOME="$WS/.godot-cache"
export XDG_CONFIG_HOME="$WS/.godot-config"

for r in $ROUTES; do
  log="$OUT/$r.log"
  echo "== route $r -> $log"
  GW2_UI_ROUTE="$r" timeout 900 "$GODOT" --headless --path "$LIVE" >"$log" 2>&1
  rc=$?
  echo "   rc=$rc lines=$(wc -l <"$log")"
done
echo "done; logs in $OUT"
