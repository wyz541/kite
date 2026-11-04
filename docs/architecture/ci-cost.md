# CI cost policy

## Goal

Keep pull request feedback fast. Run expensive platform builds only when the change needs them.

## Trigger matrix

| Event | Core CI | Windows CI | macOS CI |
| --- | --- | --- | --- |
| Pull request without `ci:full` | Yes | No | No |
| Pull request with `ci:full` | Yes | Yes | Yes |
| Push to `main` | Yes | Yes | macOS UI changes only |
| Weekly schedule | No | No | Yes |
| Manual dispatch | Yes | Yes | Yes |

Path filters still apply. A matching event can skip a platform workflow when its changed files do not match the workflow paths.

## Pull request labels

- Add `ci:full` to a pull request when the change needs Windows or macOS validation before merge.
- Keep `ci:full` on the pull request when later commits also need platform validation.
- Remove `ci:full` to stop platform builds for later commits.

Create the `ci:full` repository label before you use this control.

## Cost controls

- Pull requests normally use one Ubuntu runner.
- Windows and macOS CI use debug builds. They do not build release binaries.
- Routine CI does not upload Windows or macOS artifacts.
- Failed Windows E2E logs stay for one day.
- Concurrent runs for the same pull request or branch are cancelled.
- Tags do not start the full CI workflow.
- macOS CI runs weekly, when macOS UI files change on `main`, or on demand.

## Estimate

The recent 100 workflow runs contained 59 full CI runs. Most pull request iterations started both Windows and macOS runners.

The new policy reduces the expected weighted runner minutes to about 5 percent of the previous value. Check the next billing period for the actual result.
