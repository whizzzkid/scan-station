# Development

## Rust CLI

This repo includes a small Rust CLI scaffold with a Justfile for convenience.

- Prereqs: install Rust toolchain and `just`.

```sh
# macOS (Homebrew)
brew install just
rustup component add clippy rustfmt
```

- Common tasks (using just):

```sh
just build           # debug build
just build-release   # release build
just test            # run tests
just fmt             # format via rustfmt
just lint            # clippy with -D warnings
just run --          # run the CLI (optionally pass a URL)
```

- Example run:

```sh
just run -- https://httpbin.org/get
```

The CLI demonstrates a `kdam` progress bar and performs a blocking HTTP GET via `reqwest`.
