#!/bin/sh
set -eu
cd "$(dirname "$0")"
case "$(uname -s)" in
  Darwin) pic=-fPIC; output=libtextkit.dylib; shared=-dynamiclib ;;
  MINGW*|MSYS*|CYGWIN*) pic=; output=textkit.dll; shared=-shared ;;
  *) pic=-fPIC; output=libtextkit.so; shared=-shared ;;
esac
${CXX:-c++} -std=c++17 -O2 -Wall -Wextra -Werror $shared $pic ${SPAR_TEST_CFLAGS:-} -I../../../include textkit.cpp -o "$output"
