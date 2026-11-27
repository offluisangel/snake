# Omarchy Snake

Snake for Omarchy. Fill the 16 × 16 board without running into yourself. The game follows your Omarchy theme as it changes and saves a local high score for each difficulty.

![Omarchy Snake ready to play, with the board, high score, and keyboard shortcuts visible](./assets/omarchy-snake.png)

## Install and play

**Omarchy or Arch Linux with Wayland (x86_64).** Run this command from any directory. No Rust toolchain or source checkout required.

```bash
( pkg=$(mktemp --suffix=.pkg.tar.zst) || exit; trap 'rm -f -- "$pkg"' EXIT; curl -fL -o "$pkg" https://github.com/offluisangel/snake/releases/download/v0.3.0/omarchy-snake-0.3.0-1-x86_64.pkg.tar.zst && printf '%s  %s\n' 63ed847e4a54e08858ea73b211c2ad9e26fe838c983856d41fefd311e677e39a "$pkg" | sha256sum -c - && sudo pacman -U "$pkg" )
```

The command downloads and verifies the [v0.3.0 package](https://github.com/offluisangel/snake/releases/tag/v0.3.0). Confirm the installation when `pacman` prompts you, then open **Omarchy Snake** from **Apps** or run `omarchy-snake`.

| Key | Action |
| --- | --- |
| Arrow keys or WASD | Turn |
| 1 / 2 / 3 | Select Easy / Normal / Hard before a game or from the result screen |
| Space | Start, pause, resume, or play again |
| M | Toggle sound effects |
| Q | Quit |

You can also play with a controller. The hints show the controls relevant to the current screen. All keyboard shortcuts still work when a controller is connected.

| Controller | Action |
| --- | --- |
| D-pad or left stick | Turn during play |
| D-pad left / right | Change difficulty before a game or on the result screen |
| Right face button (Xbox B / PlayStation Circle) | Start, pause, resume, or play again |
| Left face button (Xbox X / PlayStation Square) | Toggle sound effects |
| Select (also labeled Back, View, or Share on some controllers) | Quit |

To uninstall, run `sudo pacman -R omarchy-snake`.

## Run and test from source

With Rust, Cargo, `pkg-config`, and `libudev` installed, run these commands from the repository root:

```bash
cargo run --locked --release  # Launch the game
cargo test --locked           # Run the tests
```
