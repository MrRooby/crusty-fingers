# WPM model

This document describes how the score is computed. The code is in `src/stats.rs`, and the
rules for which keys count are in `src/keys.rs`.

## What counts as typing

A word is five characters. A "character" is one press of a typing key, so the first
question is which key events count:

```mermaid
flowchart TD
    event["input event"]
    iskey{"key event?"}
    ismod{"Ctrl, left Alt<br/>or Super?"}
    track["remember whether<br/>it is held"]
    ispress{"a press?<br/>not a release<br/>or auto-repeat"}
    held{"is a shortcut<br/>modifier held?"}
    which{"which key?"}
    char["Key::Char<br/>plus one character"]
    back["Key::Backspace<br/>minus one character"]
    ignore["ignored"]

    event --> iskey
    iskey -->|"no"| ignore
    iskey -->|"yes"| ismod
    ismod -->|"yes"| track
    ismod -->|"no"| ispress
    ispress -->|"no"| ignore
    ispress -->|"yes"| held
    held -->|"yes"| ignore
    held -->|"no"| which
    which -->|"letter, digit, punctuation,<br/>space or enter"| char
    which -->|"backspace"| back
    which -->|"anything else"| ignore
```

In more detail:

- **Characters** are the digit row, the three letter rows with their punctuation keys,
  space, enter and the extra key next to left Shift on ISO keyboards.
- **Backspace** takes one character away. This makes the score a net speed: text you type
  and then delete does not count.
- **Shortcuts are not typing.** While Ctrl, left Alt or Super is held, nothing counts, not
  even backspace, because Ctrl+Backspace removes a whole word and there is no telling how
  long it was.
- **Shift and right Alt (AltGr) are not shortcut modifiers.** They produce capitals and
  characters such as `ą`, so keys pressed with them count.
- **Auto-repeat is ignored.** Holding a key down is one press.
- **Tab, Escape, arrows, function keys and the keypad are ignored.**

Modifier state is tracked per keyboard.

## The figures

| Constant | Value | Meaning |
|---|---|---|
| `WINDOW` | 10 s | how far back the live figure looks |
| `MIN_SPAN` | 3 s | the shortest time any rate is measured over |
| `PEAK_MIN_SPAN` | 8 s | how much time the live figure must cover to count as a peak |
| `IDLE_GAP` | 5 s | the longest pause that still counts as typing time |
| `CHARS_PER_WORD` | 5 | |

Every rate uses the same formula, and is rounded to a whole number:

```
rate(characters, span) = characters / 5 / (max(span, 3 s) in minutes)
```

### Live: `wpm`

The daemon keeps every counted key press of the last 10 seconds, each as +1 or -1.

```
wpm = rate(sum of the window, but at least 0; age of the oldest key in the window)
```

Dividing by the age of the oldest key, rather than always by 10 seconds, makes the figure
correct from about three seconds into a burst instead of ramping up over ten. The 3 second
minimum keeps the first few keys from reading as an absurd speed.

This is what the live figure does when you type steadily at 60 WPM (five keys per second)
for 15 seconds and then stop:

```mermaid
xychart-beta
    title "Live WPM during and after 15 seconds of typing at 60 WPM"
    x-axis "seconds since the first key" 0 --> 26
    y-axis "live WPM" 0 --> 70
    line [0, 20, 40, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 60, 54, 48, 42, 36, 30, 24, 18, 12, 6, 0, 0]
```

| Seconds | What happens |
|---|---|
| 0 to 3 | the rate is measured over the 3 second minimum, so it climbs: 20, 40, 60 |
| 3 to 15 | the figure is exact |
| 15 to 25 | typing has stopped; keys leave the window one second's worth at a time |
| 25 | the window is empty, the figure is 0 and the daemon goes back to sleep |

### Session: `chars`, `active_secs`, `average`

`chars` is the number of characters since the daemon started, net of backspaces. It never
goes below zero.

`active_secs` is the time spent typing. The gap between two consecutive counted keys is
added to it when the gap is 5 seconds or less. Longer pauses are thinking, reading or being
away, and are left out.

```
average = rate(chars, active time)
```

Because active time is made of the gaps between keys, a burst of N keys has N - 1 gaps.
In the example above, 75 keys cover 14.8 seconds, so the average reads 61 while the live
figure reads 60.

### Session: `peak`

`peak` is the highest live figure seen, with one condition: it is only updated when the
oldest key in the window is at least 8 seconds old. The live figure then covers at least
8 seconds, so a two second sprint cannot set a record. In the example, the peak becomes
60 at second 8.

Nothing is written to disk. All figures start again from zero when the daemon restarts.

## What the model cannot see

The daemon sees key presses, not the text that results from them. That keeps it simple
and private, and it has consequences:

- **Layouts do not matter.** A press of a typing key is one character whatever the layout
  maps it to.
- **Dead keys and compose sequences** count once per key pressed, not once per resulting
  character.
- **Completion and pasting** are invisible. Text your editor inserts for you is not counted,
  and neither is pasted text.
- **Deleting a selection** counts as one backspace, however much was selected.
- **Accuracy is not measured.** There is no reference text, so a typo you leave in counts
  the same as a correct character. Only what you delete is subtracted.
