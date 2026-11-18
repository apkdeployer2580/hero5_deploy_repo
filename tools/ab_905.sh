#!/usr/bin/env bash
# The project's acceptance test: offline, five scenes, patched vs --no-patch,
# byte-identical.
#
# The first four are the documented 905 frames (newgame 119 + ingame 543 +
# shop 134 + online 109). `npcmenu` is new and it is not optional: those four
# call `Battle::CallNpcMenu` zero times, so 19 KB of 제련/오브/조합/도감 drawing
# code sat outside the acceptance test while blitter ports were landing
# (ANALYSIS.md §31.6).
#
#   bash tools/ab_905.sh <tag>            # run both sides and compare
#   bash tools/ab_905.sh <tag> ref        # only the --no-patch side
#   bash tools/ab_905.sh <tag> new        # only the patched side
#   bash tools/ab_905.sh <tagA> cmp <tagB>-<side>   # compare two saved sides
#
# Traps this encodes, all of them paid for once already:
#   * offline (no --server): a live server makes the *baseline* wobble
#     (ANALYSIS.md 18.4), so the A/B would measure a race, not a port.
#   * `out/storage` untouched -- each side gets its own fresh storage dir, and
#     the four scenes run in the same order: a save left by a previous run
#     draws different screens (25.6); 534 of 905 frames "differed" once for
#     exactly that reason.
#   * `--film-every 10`, the default, is what makes the count 905.
set -uo pipefail
export PATH="$PATH:$HOME/.cargo/bin"
# A dedicated target dir: a concurrent `cargo run` elsewhere in the tree
# takes the lock on the shared one and kills a 40-minute A/B halfway.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-target/ab}"
tag="${1:?usage: ab_905.sh <tag> [ref|new|both|cmp <other>]}"
which="${2:-both}"
# SCENES=... narrows a run to one scene while iterating on a port.
scenes="${SCENES:-newgame ingame shop online npcmenu}"
# PROBE_FLAGS=... goes to both sides, so whatever it changes is not what the
# comparison measures. The use for it is asking "are the ports still
# byte-identical at the canvas the phone actually runs" -- the default here is
# the bare 400x240, and the shell ships 585x270 (`--screen 2340x1080`).
probe_flags="${PROBE_FLAGS:-}"

run_side() {          # $1 = side name, $2... = extra probe flags
  side="$1"; shift
  rm -rf "out/ab-$tag-$side" "out/ab-$tag-$side-storage"
  mkdir -p "out/ab-$tag-$side-storage"
  for s in $scenes; do
    if ! cargo run -q -p hl5_probe -- --boot \
      --storage "out/ab-$tag-$side-storage" \
      --script "tools_re/scripts/$s.txt" \
      --film "out/ab-$tag-$side/$s" $probe_flags "$@" >"out/ab-$tag-$side-$s.log" 2>&1
    then
      echo "!! $side/$s failed; see out/ab-$tag-$side-$s.log"
      tail -5 "out/ab-$tag-$side-$s.log"
      exit 1
    fi
    printf '  %-4s %-8s %4s frames\n' "$side" "$s" "$(ls "out/ab-$tag-$side/$s" | wc -l)"
  done
}

compare() {           # $1 = ref dir prefix, $2 = new dir prefix
  total=0
  for s in $scenes; do
    printf '== %-8s ' "$s"
    python tools_re/ab_compare.py "$1/$s" "$2/$s" 2>&1 | tail -2 | tr '\n' ' '
    echo
  done
}

case "$which" in
  ref)  run_side ref --no-patch ;;
  new)  run_side new ;;
  cmp)  compare "out/ab-$tag-ref" "out/ab-${3:?need <other>-<side>}" ;;
  both) run_side ref --no-patch
        run_side new
        compare "out/ab-$tag-ref" "out/ab-$tag-new" ;;
  *)    echo "unknown mode $which"; exit 2 ;;
esac
