Output docs/screenshots/board.gif

Set Shell "bash"
Set FontSize 15
Set Width 1400
Set Height 860
Set Padding 20
Set TypingSpeed 80ms

# IMPORTANT: run this tape with NO_COLOR unset in the parent shell, e.g.:
#   env -u NO_COLOR vhs docs/demo.tape
# ttyd/bash inherit the parent process environment. If NO_COLOR is set
# there, crossterm (https://no-color.org/) disables ALL color output and
# every screenshot renders monochrome even though the app emits truecolor
# escape codes correctly.
#
# Every Screenshot also needs a trailing Sleep: VHS schedules the capture
# asynchronously, and an immediately-following Type/keypress can race it,
# silently dropping the frame. A Hide/Show block around a burst of
# Screenshots was tried to keep the hero GIF short but reliably dropped
# every capture except the last one -- so the full theme cycle just stays
# visible in the GIF instead.

Type "bdw"
Enter
Sleep 2s
Screenshot docs/screenshots/board-status.png
Sleep 400ms

Type "e"
Sleep 600ms
Type "t"
Sleep 600ms
Screenshot docs/screenshots/board-epic.png
Sleep 400ms

Type "s"
Sleep 400ms
Down 3
Sleep 300ms
Type "t"
Sleep 600ms
Enter
Sleep 1s
Screenshot docs/screenshots/detail.png
Sleep 1500ms
Escape
Sleep 500ms

Type "t"
Sleep 500ms
Screenshot docs/screenshots/theme-catppuccin-mocha.png
Sleep 400ms

Type "t"
Sleep 500ms
Screenshot docs/screenshots/theme-tokyo-night.png
Sleep 400ms

Type "t"
Sleep 500ms
Screenshot docs/screenshots/theme-rose-pine.png
Sleep 400ms

Type "t"
Sleep 500ms
Screenshot docs/screenshots/theme-everforest.png
Sleep 400ms

Type "t"
Sleep 500ms
Screenshot docs/screenshots/theme-solarized-light.png
Sleep 400ms

Type "t"
Sleep 500ms
Screenshot docs/screenshots/theme-rose-pine-dawn.png
Sleep 400ms

Type "q"
Sleep 1s
