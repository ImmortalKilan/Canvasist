# Canvasist

> Never miss an assignment deadline again.

Canvasist is an open-source desktop app that gathers every assignment from **Canvas** and **Gradescope** into one minimal list, shows each deadline with its submission status, and reminds you before anything is due.

**Status:** early development — not usable yet.

## Why

Gradescope assignments often don't appear in Canvas's assignment list, so checking Canvas alone makes it easy to miss work. Canvasist pulls from both, merges duplicates, and keeps everything in one place.

## Planned features

- Works with most Canvas instances — you supply your school's Canvas URL and a personal access token
- Gradescope access through your Canvas LTI integration (no separate Gradescope login)
- Current-term assignments, quizzes and graded discussions
- Detailed status: not submitted, submitted, late, graded, missing, dismissed
- Duplicate Canvas/Gradescope entries merged into one
- Customizable desktop notifications before deadlines
- Runs quietly in the system tray with a small footprint; optional start on login
- English and Chinese interface

## Privacy

Canvasist runs entirely on your computer. Your Canvas token is stored in the operating system's credential store and is only ever sent to your own Canvas server. There is no Canvasist backend, telemetry or analytics.

## Platform support

Windows first. macOS and Linux may follow.

## License

[MIT](LICENSE)
