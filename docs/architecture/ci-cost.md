# CI cost policy

## Goal

Keep pull request feedback fast. Use GitHub runners only for Linux and Windows checks.

## Trigger matrix

| Event | Core CI | Windows CI | macOS |
| --- | --- | --- | --- |
| Pull request without `ci:full` | Yes | No | Local only |
| Pull request with `ci:full` | Yes | Yes | Local only |
| Push to `main` | Yes | Yes | Local only |
| Manual dispatch | Yes | Yes | Local only |

Path filters still apply. A matching event can skip the Windows workflow when its changed files do not match the workflow paths.

## Windows pull request labels

- Add `ci:full` to a pull request when the change needs Windows validation before merge.
- Keep `ci:full` on the pull request when later commits also need platform validation.
- Remove `ci:full` to stop Windows builds for later commits.

Create the `ci:full` repository label before you use this control.

## macOS verification

GitHub Actions does not run macOS builds.

Run the complete macOS verification on a local Mac:

```bash
bash apps/macos/verify.sh
```

The script checks formatting, runs core tests, runs Clippy, runs the macOS smoke tests, and builds the release application.

## Cost controls

- Pull requests normally use one Ubuntu runner.
- Windows CI keeps the cached release build.
- Routine CI does not upload Windows artifacts.
- Failed Windows E2E logs stay for one day.
- Concurrent runs for the same pull request or branch are cancelled.
- Tags do not start the full CI workflow.
- No GitHub-hosted macOS runner is used.

## Estimate

The recent 100 workflow runs contained 59 full CI runs. Most pull request iterations started both Windows and macOS runners.

The new policy removes GitHub macOS completions entirely. The remaining expected weighted runner minutes are below 5 percent of the previous value. Check the next billing period for the actual result.
