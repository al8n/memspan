# Releasing memspan

The version in `Cargo.toml` is the source of truth. A release tag must be
`v<version>` for that exact version.

## Trusted publishing setup

Before the first OIDC release:

1. Create a GitHub environment named `crates-io` and require `al8n` approval.
2. Configure the `memspan` crate's GitHub Actions trusted publisher on
   crates.io with owner `al8n`, repository `memspan`, workflow filename
   `crates.yml`, and environment `crates-io`.

The workflow requests a short-lived crates.io token only in the publish job.
Do not add a long-lived crates.io token to GitHub Actions. Leave crates.io's
`trustpub_only` setting disabled until one release succeeds through OIDC; it
can be enabled separately afterward if token-based publication should be
prohibited.

## Release process

1. Update the version in `Cargo.toml` and add the release notes to
   `CHANGELOG.md`.
2. Merge the release preparation only after CI and the Crates.io verification
   job pass.
3. Create and push the matching `v<version>` tag from the release commit.
4. Review the completed verification job, then approve the `crates-io`
   environment deployment.
5. Confirm the new version appears on crates.io, then build that exact version
   in a fresh consumer project.

Pull requests and manual workflow dispatches run verification only. Publication
is limited to pushed `v*` tags whose tag name exactly matches the manifest
version. Version `0.1.2` is already published and must not be used to test the
new workflow.
