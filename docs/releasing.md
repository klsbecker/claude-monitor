# Releasing

Releases follow git flow and [SemVer](https://semver.org). Pushing a `vX.Y.Z` tag builds the `.exe` on
GitHub Actions and publishes the GitHub release with the notes from `CHANGELOG.md`.

## Cut a release

```powershell
git switch develop && git pull
git switch -c release/1.2.0

# 1. Bump the version in Cargo.toml (and Cargo.lock: cargo build updates it).
# 2. In CHANGELOG.md, rename "Unreleased" to "[1.2.0] - YYYY-MM-DD" and add an empty "Unreleased" above it;
#    update the compare links at the bottom.
git commit -am "chore: bump version to v1.2.0"

git switch main && git merge --no-ff release/1.2.0 -m "chore: release v1.2.0"
git tag -a v1.2.0 -m "v1.2.0"
git switch develop && git merge --no-ff release/1.2.0 -m "chore: merge release v1.2.0 into develop"
git branch -d release/1.2.0

git push origin main develop v1.2.0
```

The **Release** workflow checks that the tag matches `Cargo.toml`, builds, tests, and attaches
`ClaudeMonitor.exe` plus `SHA256SUMS.txt` to the release. The asset name never changes, so winget installs
it as `ClaudeMonitor.exe` and the command `ClaudeMonitor`.

Hotfixes are the same from `main`: `hotfix/1.2.1` → bump → merge into `main` (tag) and `develop`.

## winget

The package id is `KlsBecker.ClaudeMonitor`, installer type `portable` (winget copies the `.exe` and adds
the `ClaudeMonitor` command).

### First submission (once)

1. Publish the release on GitHub first.
2. Install the manifest tool: `winget install Microsoft.WingetCreate`.
3. Create the manifest from the release asset and submit it (it forks `microsoft/winget-pkgs` and opens the
   PR with your GitHub account):

   ```powershell
   wingetcreate new https://github.com/klsbecker/claude-monitor/releases/download/v1.0.0/ClaudeMonitor.exe
   ```

   Answer the prompts: `PackageIdentifier` `KlsBecker.ClaudeMonitor`, `Publisher` `Klaus Becker`,
   `PackageName` `Claude Monitor`, `License` `MIT`, `ShortDescription` "Claude plan usage on the Windows
   taskbar"; leave `PortableCommandAlias` empty (the command is the file name, `ClaudeMonitor`). Then choose to submit.
4. Microsoft's bots validate the PR (download, hash, malware scan, install test) and a moderator merges it,
   usually within a few days. After that, `winget install KlsBecker.ClaudeMonitor` works.

### Updates

After each release:

```powershell
wingetcreate update KlsBecker.ClaudeMonitor --version 1.2.0 `
  --urls https://github.com/klsbecker/claude-monitor/releases/download/v1.2.0/ClaudeMonitor.exe `
  --submit
```

This can be automated with the `winget` job in `.github/workflows/release.yml`: create a classic GitHub token
with the `public_repo` scope, save it as the repository secret `WINGET_TOKEN`, and the job submits the update
on every release (it is skipped while the secret is missing, so the first submission stays manual).
