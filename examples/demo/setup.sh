#!/bin/sh
# Makes a fresh demo workspace in FOLDER: the project `demo`, with this
# folder's lifecycles and actionables, and tickets on them.
#
#   examples/demo/setup.sh FOLDER [PRIMUM]
#
# PRIMUM is the primum to use (`primum` on PATH by default). FOLDER is
# replaced only when it is a demo made here before.
set -eu

[ $# -ge 1 ] || { echo "usage: $0 FOLDER [PRIMUM]" >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
dir=$1
primum=${2:-primum}
case "$primum" in
  */*) primum=$(cd "$(dirname "$primum")" && pwd)/$(basename "$primum") ;;
esac

if [ -e "$dir" ]; then
  [ -f "$dir/.primum-demo" ] || {
    echo "$dir exists and is not a demo made here: not replacing it" >&2
    exit 1
  }
  rm -rf "$dir"
fi
mkdir -p "$dir"
cd "$dir"
touch .primum-demo
"$primum" workspace init > /dev/null
"$primum" init demo > /dev/null
cp -R "$here/lifecycles" "$here/actionables" demo/
cd demo
"$primum" lifecycle check

# a ticket on a lifecycle: add it, then attach it
on() {
  id=$("$primum" add TODO "$2")
  "$primum" lifecycle attach "$id" "$1" > /dev/null
  echo "#$id $1: $2"
}
on publish "Café menüsü"
on publish "Şehir rehberi"
on hello "Müşteri yanıtı"
"$primum" add TODO "Elle yürüyen iş" > /dev/null
