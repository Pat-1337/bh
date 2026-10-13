# bh

An animated ASCII black hole for your terminal, with an accretion disk, gravitational lensing and a twinkling starfield.

## Install

```sh
cargo install --git https://github.com/Pat-1337/bh
```

Or build from source:

```sh
cargo build --release
./target/release/bh
```

## Usage

```
bh [ZOOM] [--clean] [--black] [--once]
```

| Option    | Description                          |
| --------- | ------------------------------------ |
| `ZOOM`    | zoom level, 5-80 (default 20)        |
| `--clean` | hide background stars                |
| `--black` | force a black background             |
| `--once`  | render a single frame and exit       |

Press `q`, `Esc` or `Ctrl+C` to quit.

Needs a terminal with truecolor support.
