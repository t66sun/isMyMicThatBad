# Issue tracker: GitHub

Issues and specs live in GitHub Issues for `t66sun/isMyMicThatBad`.
Use the `gh` CLI from this repository; it resolves the repo from `origin`.

## Conventions

- Create: `gh issue create --title "..." --body-file <file>`.
- Read: `gh issue view <number> --comments`.
- List: `gh issue list --state open --json number,title,body,labels,comments`.
- Comment: `gh issue comment <number> --body-file <file>`.
- Apply or remove labels: `gh issue edit <number> --add-label "..."` or `--remove-label "..."`.
- Close: `gh issue close <number> --comment "..."`.

Use a UTF-8 file for multiline bodies.
When a skill says "publish to the issue tracker", create a GitHub issue.
When it says "fetch the relevant ticket", read the issue and its comments.

## Pull requests as a triage surface

**PRs as a request surface: no.**

## Wayfinding operations

- Map: one issue labelled `wayfinder:map`, containing Notes, Decisions-so-far, and Fog.
- Child ticket: link it as a GitHub sub-issue; if unavailable, add `Part of #<map>` to the child and a task-list entry to the map.
- Ticket type: use `wayfinder:research`, `wayfinder:prototype`, `wayfinder:grilling`, or `wayfinder:task`.
- Blocking: use native GitHub issue dependencies; if unavailable, record `Blocked by: #<number>` in the child body.
- Frontier: choose the first open child in map order with no open blockers and no assignee.
- Claim: `gh issue edit <number> --add-assignee "@me"`.
- Resolve: comment with the answer, close the child, and append a summary and link to the map's Decisions-so-far.
