# Spar libraries

This repository holds the Spar standard library, six beta packages, and the native Rust SDK used by `spar-tcp`.

- `stdlib/` is the source tree installed at `$SPA_HOME/stdlib` or `$XDG_DATA_HOME/spar/stdlib`. Spar embeds its standard library for execution; these files give installed tools matching source paths for navigation and diagnostics.
- `packages/` contains `spar-args`, `spar-log`, `spar-web`, `spar-tcp`, `spar-http-server`, and `tester`. Each directory has its own `spar.package.spar` manifest. Local dependencies point to sibling directories.
- `sdk/` contains `spar-native-sys`, `spar-native-macros`, and `spar-native`. They are needed to rebuild the TCP native module and to build Spar from source.

The beta source installer in `oraclevs/sparsh` clones this repository. It installs the standard library sources and the packages under the Spar data directory. Project dependencies remain in Spar's package manager: declare them with `spar add` and resolve them with `spar install`.

For GitHub package installation, each package is also available as a branch of this repository. For example:

```sh
spar add args github:oraclevs/spar-libraries#spar-args
spar add tcp "path:$HOME/.local/share/spar/libraries/spar-tcp"
```

The package branches contain the package at the repository root. Their manifests refer to other package branches when needed. The `main` branch holds the complete library tree. After committing changes on `main`, run `scripts/sync-package-branches.sh --push` to update the six package branches.

The source installer builds the TCP native module for the host before installing the packages. A direct GitHub checkout of the `spar-tcp` branch needs `build-package.sh` before native imports can run. Its TLS integration test generates a localhost certificate at test time with OpenSSL.
