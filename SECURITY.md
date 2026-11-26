# Security

Claude Monitor handles an OAuth token with access to your Claude account, so security reports are taken
seriously.

## What the app does with your credentials

- It reads `.credentials.json` from the Claude Code folder (`~/.claude`, `$CLAUDE_CONFIG_DIR` or the folder
  chosen in the menu), written by Claude Code.
- It sends the access token only to `api.anthropic.com`, and the refresh token only to `platform.claude.com`
  when the access token has expired. Both over HTTPS.
- After a refresh it writes the new tokens back to the same file, as Claude Code does.
- It never logs tokens and has no telemetry or other network traffic.

## Reporting a vulnerability

Please **do not open a public issue**. Use GitHub's
[private vulnerability reporting](https://github.com/klsbecker/claude-monitor/security/advisories/new)
with a description, steps to reproduce and the affected version. You will get an answer within a week.

Only the latest release receives security fixes.
