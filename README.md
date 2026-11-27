# Omarchy Snake

Snake for Omarchy. Fill the 16 × 16 board without running into yourself. The game follows your Omarchy theme as it changes and saves a local high score for each difficulty.

![Omarchy Snake ready to play, with the board, high score, and keyboard shortcuts visible](./assets/omarchy-snake.png)

## Install and play

**Omarchy or Arch Linux with Wayland (x86_64).** Run this command from any directory. No Rust toolchain or source checkout required.

```bash
( pkg=$(mktemp --suffix=.pkg.tar.zst) || exit; trap 'rm -f -- "$pkg"' EXIT; curl -fL -o "$pkg" https://github.com/offluisangel/snake/releases/download/v0.2.0/omarchy-snake-0.2.0-1-x86_64.pkg.tar.zst && printf '%s  %s\n' 2d90fd69f03ecb761cd1d4288e16df58d3832b32dc4aeb0a7aa48d48158c722a "$pkg" | sha256sum -c - && sudo pacman -U "$pkg" )
```

The command downloads and verifies the [v0.2.0 package](https://github.com/offluisangel/snake/releases/tag/v0.2.0). Confirm the installation when `pacman` prompts you, then open **Omarchy Snake** from **Apps** or run `omarchy-snake`.

The controls below describe the current source version. The v0.1.0 package does not have difficulty selection.

| Key | Action |
| --- | --- |
| Arrow keys or WASD | Turn |
| 1 / 2 / 3 | Select Easy / Normal / Hard before a game or from the result screen |
| Space | Start, pause, resume, or play again |
| M | Toggle sound effects |
| Q | Quit |

You can also play with a controller. The on-screen shortcuts switch when a controller connects or disconnects, and the keyboard still works with a controller connected.

| Controller | Action |
| --- | --- |
| D-pad | Turn during play |
| D-pad left / right | Change difficulty before a game or on the result screen |
| Right face button (Xbox B / PlayStation Circle) | Start, pause, resume, or play again |
| Select (also labeled Back, View, or Share on some controllers) | Quit |

To uninstall, run `sudo pacman -R omarchy-snake`.

## Run and test from source

With Rust, Cargo, `pkg-config`, and `libudev` installed, run these commands from the repository root:

```bash
cargo run --locked --release  # Launch the game
cargo test --locked           # Run the tests
```
