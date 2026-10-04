# Contributing

Thanks for the interest — small fixes and ideas are welcome.

## Reporting a bug or asking for a feature

Open an issue on GitHub. Please include:

- Which COSMIC version you are on (`cosmic-session --version` or the
  package version).
- What you saw and what you expected.
- Steps to reproduce if it is a bug. A short clip of the behaviour helps
  a lot and can be captured with `gpu-screen-recorder` without stealing
  focus from the menu.

## Making a change

1. Fork the repository and work on a branch.
2. Keep the change focused — one problem per pull request.
3. Match the existing code style (dense "why" comments over "what"
   comments; small pure modules with their own tests).
4. Run the same checks CI runs before you push:

   ```sh
   just verify
   ```

   That is `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`
   and `cargo test`. It must end green.

5. Write a short commit message that says what changed and why.
6. Open a pull request. Note anything only a person pressing keys can
   confirm — screenshots or a short clip of the behaviour help the
   reviewer judge motion, colour and layout.

## Running the app from your tree

The menu is a resident process that owns a D-Bus name, so a locally built
copy will not take over the installed one on its own. `just install`
handles this by stopping the running menu first; if you want to run your
build without installing it, stop the installed one yourself:

```sh
pkill -f 'cosmic-start-menu-applet --(toggle|prewarm)'
./target/release/cosmic-start-menu-applet --prewarm &
./target/release/cosmic-start-menu-applet --toggle
```

Afterwards, restart the pre-warmed installed copy or run `just install`
again.

## License

By submitting a pull request you agree your contribution may be used
under the project's dual Apache 2.0 / MIT licence.
