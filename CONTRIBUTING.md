# Contributing to Claude Monitor

**English** · [Português](docs/pt-BR/CONTRIBUTING.md) · [Español](docs/es/CONTRIBUTING.md)

Thanks for helping! Bug reports, ideas, translations and code are all welcome.

## Reporting bugs and ideas

Open an [issue](https://github.com/klsbecker/claude-monitor/issues/new/choose) with the templates. For bugs,
include your Windows version, the app version (menu → *About*), what you expected and what happened, and the
relevant lines of `%LOCALAPPDATA%\ClaudeMonitor\claude-monitor.log`. **Never paste the contents of
`.credentials.json`** or any token.

Security problems go through [SECURITY.md](SECURITY.md), not public issues.

## Development setup

- Windows 10 or 11, [rustup](https://rustup.rs). The toolchain version (and rustfmt/clippy) comes from
  `rust-toolchain.toml`.
- With Visual Studio Build Tools (C++ workload) the default MSVC toolchain builds with plain `cargo build`.
- Without them, `scripts\build-gnu.ps1` builds with the GNU toolchain (it needs `dlltool`/`as`, e.g. from
  Strawberry Perl).

```powershell
cargo build                 # debug build
cargo run -- --demo         # made-up data, no sign-in needed
cargo test
cargo fmt
cargo clippy --all-targets -- -D warnings
```

Running a build replaces any running instance, so you can iterate without closing the app by hand.

## Branch model (git flow)

| Branch | Purpose |
| --- | --- |
| `main` | Released code only. Every commit on `main` is a tagged release `vX.Y.Z`. |
| `develop` | Integration branch for the next release. Pull requests target `develop`. |
| `feature/<short-name>` | New features and non-urgent fixes, branched from `develop`. |
| `release/X.Y.Z` | Release preparation (version bump, changelog), branched from `develop`, merged into `main` and back into `develop`. |
| `hotfix/X.Y.Z` | Urgent fixes to a release, branched from `main`, merged into `main` and `develop`. |

Maintainers follow [docs/releasing.md](docs/releasing.md) to cut releases.

## Versioning

The project follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.

- **PATCH** — bug fixes, translations, no visible behavior change.
- **MINOR** — new features or options, backwards compatible (settings keep working).
- **MAJOR** — breaking changes, such as settings or files that no longer carry over.

The version lives only in `Cargo.toml`; the app shows it in the menu. Every release has an entry in
[CHANGELOG.md](CHANGELOG.md) ([Keep a Changelog](https://keepachangelog.com) format). Add your change under
**Unreleased** in your pull request.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org): `type: description`, imperative, lowercase,
no trailing period. Types: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`, `ci`, `chore`, `style`.

```
feat: notify when the weekly limit resets
fix: keep the strip left of the notification area after explorer restarts
```

Explain the *why* in the body when it is not obvious. Keep every commit buildable.

## Code standards

- **Everything in the code is English**: identifiers, comments, log messages. Text the user sees goes in
  [`src/i18n.rs`](src/i18n.rs), never inline.
- Format with `cargo fmt` (settings in `rustfmt.toml`) and keep `cargo clippy --all-targets -- -D warnings`
  clean. CI enforces both.
- One concern per module (see [docs/architecture.md](docs/architecture.md)); keep `unsafe` blocks small and
  around the Win32 calls.
- Doc comments (`///`) on public items say what something is or guarantees. Regular comments explain *why*
  (a Windows quirk, a constraint), not *what* the code already says.
- Add unit tests for pure logic (parsing, math, formatting). UI changes: attach a screenshot to the PR.
- No new dependencies without a good reason: the app is a single small `.exe`.

## Translations

1. Add a variant to `Lang` in `src/i18n.rs` and give it a code, a native name and a Windows language id in
   `Lang::system`.
2. Copy the `EN` table, translate every string (keep the `{placeholders}`), and return it from `tr()`.
3. Optionally translate the README into `docs/<lang>/README.md`.

## Pull requests

1. Fork, branch from `develop` (`feature/...`), commit, push.
2. Open the PR against `develop` and fill in the template.
3. CI must pass (build, tests, fmt, clippy). A maintainer reviews and squash-merges or merges.

By contributing you agree your work is licensed under the [MIT License](LICENSE).
