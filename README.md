Helix Calc is a simple Reverse Polish Notation calculator.

It's written in Rust, using [Ratatui](https://ratatui.rs) for the
cross-platform interface and [bigdecimal-rs](https://github.com/akubera/bigdecimal-rs)
for the internal operations.

The name is inspired by [Helix Editor](https://helix-editor.com/), and the
functionality by the venerable GNU dc.

This is a toy app that fulfills my personal needs, but I'm open to PR :)

## Operations

Operators manipulate the stack of values [S1, S2, ...]:

- `+`, `-`, `*`, `/` : perform the arithmetic operation on S2 and S1.
- `%` : compute the modulo of S2 divided by S1.
- `^` : raise S2 to the power of S1.
- `P` : pop S1 off the stack.
- `d` : duplicate S1.
- `v` : compute the square root of S1.
- `k` : pop S1 and use it to set the precision.
- `o` : pop S1 and use it to set the output base (2–36).
- `r` : swap S1 and S2.
- `u` : undo the last operation.
- `U` : redo the last undone operation.
- `s` : pop S1 and save it to a named register (prompts for a key).
- `l` : load a named register onto the stack (prompts for a key).
- `c` : clear the stack.
- `C` : clear the registers.
- `n` : reset precision and output base.
- `y` : rotate stack forward (S1→S2→S3→…→S1).
- `Y` : rotate stack backward (S1→…→S3→S2→S1).
- `'` : toggle decimal separator.
- `[Up]`: edit S1.

## Negative numbers

Two options to enter them:

- Type them as `_123`.
- Type them as `123-` (careful, no space).

## Limitations

By default, BigDecimal is compiled with a max precision of 100 digits: beyond
that size, decimal places will be dropped, even if the integer part can handle
much larger numbers.
