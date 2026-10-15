#!/bin/sh
# Checks a ticket before it is published: it needs a title.
job=$(cat)
case "$job" in
  *'"title":""'*) echo '{"ok": false, "why": "no title"}' ;;
  *) echo '{"ok": true}' ;;
esac
