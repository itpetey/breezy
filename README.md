# Breezy

GitHub Action for continuous draft release generation.

## What it does

- Creates or updates a single draft release per branch.
- Uses merged PR titles as release notes.
- Resolves version numbers from language archetypes (e.g. Rust `Cargo.toml`).
- Optional `breezy.yml` config for grouping, templating, and tag/name formats.

## Inputs

- `language` (optional): Language archetype(s) for version detection. If omitted, `breezy.yml` is used.
- `github-token` (required): GitHub token used to create/update releases.
- `tag-prefix` (optional): Prefix for tags when no `tag-template` is set. Default `v`.
- `config-file` (optional): Path to a `breezy.yml` config.

## Config file (`breezy.yml`)

By default, Breezy looks for `.github/breezy.yml` in the repo, or `$HOME/.github/breezy.yml` inside the container. You can also pass `config-file` explicitly.

Example:

```yml
language: rust
tag-template: v$VERSION
name-template: Release $VERSION
categories:
  - title: Features
    labels:
      - feature
      - enhancement
  - title: Bug Fixes
    labels:
      - fix
      - bugfix
      - bug
  - title: Maintenance
    label: chore
exclude-labels:
  - skip-log
change-template: "* $TITLE @$AUTHOR ($NUMBER)"
template: |
  ## Changes

  $CHANGES
```

Template variables:

- `$VERSION`: Resolved version.
- `$TITLE`: PR title.
- `$AUTHOR`: PR author login.
- `$NUMBER`: PR URL.
- `$CHANGES`: Rendered change list (only for the top-level `template`).

## Example workflow

```yml
name: Breezy
on:
  push:
    branches: [main]
  workflow_dispatch:

jobs:
  draft:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: read
    steps:
      - uses: actions/checkout@v4
      - uses: ./
        with:
          language: rust
```

## Prior art

This action is heavily inspired by [release-drafter](https://github.com/release-drafter/release-drafter). There are a few key differences:
- `breezy` does not attempt to increment the version number - it reads directly from the appropriate manifest
- `breezy` creates a single release draft per branch
