#!/usr/bin/env bash
set -euo pipefail

: "${GH_REPO:?GH_REPO is required}"
: "${GH_TOKEN:?GH_TOKEN is required}"
: "${GITHUB_SHA:?GITHUB_SHA is required}"
: "${RELEASE_VERSION:?RELEASE_VERSION is required}"
[[ "$RELEASE_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
[[ "$GITHUB_SHA" =~ ^[0-9a-f]{40}$ ]]
release_tag="v${RELEASE_VERSION}"
lookup_error="$(mktemp)"
trap 'rm -f "$lookup_error"' EXIT

# --target does not move an existing tag. Reject a tag owned by another commit.
if tag_sha="$(gh api "repos/$GH_REPO/git/ref/tags/$release_tag" --jq '.object.sha' 2> "$lookup_error")"; then
  if [[ "$tag_sha" != "$GITHUB_SHA" ]]; then
    echo "Tag $release_tag already points elsewhere; refusing to reuse it." >&2
    exit 1
  fi
elif ! grep -Fq '(HTTP 404)' "$lookup_error"; then
  cat "$lookup_error" >&2
  exit 1
fi

# A failed build or incomplete upload must never publish a release.
for asset in \
  "Octopus_${RELEASE_VERSION}_amd64.deb" \
  "Octopus-${RELEASE_VERSION}-1.x86_64.rpm" \
  "Octopus_${RELEASE_VERSION}_amd64.AppImage" \
  SHA256SUMS; do
  test -s "release-assets/$asset"
done
(cd release-assets && sha256sum --check SHA256SUMS)

cat > release-notes.md <<EOF
Octopus ${RELEASE_VERSION} for Linux x86_64 (Ubuntu 24.04 baseline).

- Debian/Ubuntu: download the .deb package.
- RPM-based distributions: download the .rpm package.
- Portable bundle: download the .AppImage and make it executable.
- Verify downloads with SHA256SUMS. System Git 2.43 or newer is required.

Source commit: ${GITHUB_SHA}
Known limitations and workflow acceptance: https://github.com/${GH_REPO}/blob/${GITHUB_SHA}/docs/development.md
EOF

if metadata="$(gh release view "$release_tag" --json isDraft,targetCommitish 2> "$lookup_error")"; then
  target="$(jq -r '.targetCommitish' <<< "$metadata")"
  if [[ "$target" != "$GITHUB_SHA" ]]; then
    echo "Release $release_tag belongs to a different commit; refusing to overwrite it." >&2
    exit 1
  fi
  if [[ "$(jq -r '.isDraft' <<< "$metadata")" != true ]]; then
    echo "Release $release_tag is already published; leaving it unchanged."
    exit 0
  fi
  # Resume an incomplete draft; published release assets are never overwritten.
  gh release upload "$release_tag" release-assets/* --clobber
else
  # gh release view uses this message for a missing release, without an HTTP code.
  if ! grep -Eq 'release not found|\(HTTP 404\)' "$lookup_error"; then
    cat "$lookup_error" >&2
    exit 1
  fi
  gh release create "$release_tag" release-assets/* \
    --draft --target "$GITHUB_SHA" \
    --title "Octopus ${RELEASE_VERSION}" --notes-file release-notes.md
fi

release_id="$(gh release view "$release_tag" --json databaseId --jq '.databaseId')"
[[ "$release_id" =~ ^[1-9][0-9]*$ ]]
# Semantic-version ordering also handles concurrent builds finishing out of order.
gh api --method PATCH "repos/$GH_REPO/releases/$release_id" \
  -F draft=false -F prerelease=false -f make_latest=legacy --silent
