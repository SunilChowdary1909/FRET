#!/bin/sh
export TOPLEVEL="remote/timedump"
[ -d "$TOPLEVEL/feedgeneration100" ] && mv $TOPLEVEL/feedgeneration100 $TOPLEVEL/evolutionary
[ -d "$TOPLEVEL/stg" ] && mv $TOPLEVEL/stg $TOPLEVEL/fret
[ -d "$TOPLEVEL/frafl" ] && mv $TOPLEVEL/frafl $TOPLEVEL/coverage
