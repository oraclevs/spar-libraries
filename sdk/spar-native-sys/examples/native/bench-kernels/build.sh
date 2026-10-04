#!/bin/sh
set -eu
cd "$(dirname "$0")"
case "$(uname -s)" in
  Darwin) pic=-fPIC; output=libbenchkit.dylib; shared=-dynamiclib ;;
  MINGW*|MSYS*|CYGWIN*) pic=; output=benchkit.dll; shared=-shared ;;
  *) pic=-fPIC; output=libbenchkit.so; shared=-shared ;;
esac
${CC:-cc} -std=c11 -O2 -Wall -Wextra -Werror $shared $pic ${SPAR_TEST_CFLAGS:-} -I../../../include benchkit.c -lm -o "$output"
