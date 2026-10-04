#!/bin/sh
# Build the public-header C example on the host platform.
set -eu
cd "$(dirname "$0")"
case "$(uname -s)" in
  Darwin) pic=-fPIC; output=libfastmath.dylib; shared=-dynamiclib; threads= ;;
  MINGW*|MSYS*|CYGWIN*) pic=; output=fastmath.dll; shared=-shared; threads= ;;
  *) pic=-fPIC; output=libfastmath.so; shared=-shared; threads=-lpthread ;;
esac
${CC:-cc} -std=c11 -O2 -Wall -Wextra -Werror $shared $pic -I../../../include fastmath.c -lm $threads -o "$output"
