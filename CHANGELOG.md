# Changelog

Changes in this fork (tangomando/nai-ma-speak) since norswap/nai-ma-speak v0.3.0, gathered for a pull request back
to it.

## Unreleased

### Windows: the strip can now be read

The Windows companion found the strip but never read a frame ("its frames fail their check"). The cause was the game's
HDR output (`HDROutput 1`): a capture of the game comes back brightened, and the strip's four color levels 0, 85, 170,
255 read as 0, 149, 255, 255. The top two ran together, so about a third of the blocks were misread.

- **HDR:** with the game's HDR output on, the addon draws the strip, frame and palette, at half brightness, and paints
  it again when the option changes. The companion now tells the sync colors by each channel against the brightest one,
  not a fixed level of 128, so a dimmed strip is found whether or not the capture brightens it. Without HDR the strip is
  drawn exactly as before.
- **Blocks read from their inside:** each block is the per-channel median of its inner pixels, leaving out a 1 px
  border, instead of one pixel at its center. Edge blending (scaling, sharpening) and single noisy pixels no longer
  change a block.
- **No more false finds on scenery:** at 4K, colorful scenery passed for the sync blocks as runs of 1 px every few
  seconds, and each false find held the lock for 3 s. Runs under 2 px are no longer taken for the strip.

### Diagnosing an unreadable strip

- `--decode` prints a diagnosis: how each channel's four levels came back, the two palette colors closest together,
  how many data blocks are ambiguous, and the length and checksum read against the one computed. It also writes it to
  `<picture>.decode.txt`, since the Windows release is a windowed program and its printing never reaches a terminal.
- When the strip stays unreadable, the companion logs that diagnosis (colors and counts, no chat) and saves the strip
  as `strip-unreadable.png` beside `translate.log`, once per run. The picture is encoded chat and stays on the player's
  computer.
- When colors run together, the Status line says so and points to the game's HDR option, Windows HDR and the game's
  brightness, contrast and gamma, instead of only "its frames fail their check".

### Overlay

- **Chat to translate:** a picker on the Overlay tab, whether or not the overlay is anchored, shows which chat window
  the game sends to the overlay, or all chat. The companion cannot reach the game, so picking another copies
  `/tr /window <name>` for the player to paste into chat.
- **Background opacity on a chat tab:** an anchored overlay has its own background opacity (`anchored_opacity` in
  `config.json`), none by default as before.

### Addon

- New command `/tr /window <name>` (or `/tr /window all`): the same setting as "Lines sent to the overlay" in the
  options, matched without regard to case.
- New strip line `W\t<name>`, sent after each `L`: the chat window the addon copies, empty for all chat. Older
  companions ignore it, and an older addon simply never sends it (the picker then says "Not reported yet").

### Smaller

- The focused-window check reads the window title before the program name. Asking a program run as administrator
  (Task Manager, for one) for its name fails, and xcap logged an error four times a second while one had focus.
- macOS-only window listing code is marked so the Windows build has no dead-code warnings.
- README: Windows marked as tested, the HDR note, the new overlay settings, `/tr /window`, and the new troubleshooting.

### Tested

- Windows 11, 4096x2160 at 100%, WoW: Forever with HDR output on, Claude Haiku backend: the strip is read and chat lines
  are translated.
- `cargo test --locked` (69 tests, 6 new) and `cargo clippy --all-targets` clean on Windows. Addon tests pass under
  Lua 5.4 with `unpack = table.unpack`; this machine has no Lua 5.1.
- **Not yet checked:** the macOS build with these changes, switching windows with `/tr /window` in the game, and retail.
