Helix Calc is a simple Reverse Polish Notation calculator.

It's written in Rust, using [Ratatui](https://ratatui.rs) for the
cross-platform interface and [bigdecimal-rs](https://github.com/akubera/bigdecimal-rs)
for the internal operations.

The name is inspired by [Helix Editor](https://helix-editor.com/), and the
functionality by the venerable GNU dc.

This is a toy app that fulfills my personal needs, but I'm open to PR :)

## Operations

- `+`, `-`, `*`, `/` : perform the arithmetic operation on the top two values.
- `%` : compute the modulo of the second value divided by the first.
- `^` : raise the second value to the power of the first.
- `P` : pop the top value off the stack.
- `d` : duplicate the top value.
- `v` : compute the square root of the top value.
- `k` : pop the top value and use it to set the precision.
- `r` : swap the first two values.

## Negative numbers

Two options to enter them:

- Type them as `_123`.
- Type them as `123-` (careful, no space).
