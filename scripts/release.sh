#!/usr/bin/env bash
# Usage: scripts/release.sh 0.1.0 | v0.1.0 | 0.1.0-beta1
#
# Sets the workspace version, commits, tags and (after confirmation) pushes
# main and the tag, which triggers .github/workflows/release.yml.
# If the tag already exists it offers to redo the release: the GitHub release
# and the tag are deleted right before pushing the new tag.
set -euo pipefail
cd "$(dirname "$0")/.."

die() { echo "error: $*" >&2; exit 1; }

[[ $# -eq 1 ]] || die "usage: $0 <X.Y.Z[-suffix]>"
tag="v${1#v}"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || die "invalid version: $1"
version="${tag#v}"
version="${version%%-*}"

[[ "$(git branch --show-current)" == main ]] || die "not on main"
[[ -z "$(git status --porcelain)" ]] || die "working tree is not clean"
git fetch --quiet --tags origin main
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || die "main is not in sync with origin/main"

redo=false
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null \
    || git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null; then
    command -v gh >/dev/null || die "tag $tag already exists (install gh to redo the release)"
    read -r -p "Tag $tag already exists. Delete its release and tag and release again? [y/N] " answer
    [[ "$answer" =~ ^[Yy]$ ]] || die "tag $tag already exists"
    redo=true
    git tag -d "$tag" >/dev/null 2>&1 || true
fi

read -r -p "Tag message [$tag]: " message
message="${message:-$tag}"

workspace_version() {
    sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml
}

committed=false
current="$(workspace_version)"
if [[ "$current" != "$version" ]]; then
    sed -i.bak "/^\[workspace.package\]/,/^\[/s/^version = \".*\"/version = \"$version\"/" Cargo.toml
    rm Cargo.toml.bak
    [[ "$(workspace_version)" == "$version" ]] || die "could not update Cargo.toml"
    cargo metadata --format-version 1 --offline >/dev/null
    git add Cargo.toml Cargo.lock
    git commit --quiet -m "Release $tag"
    committed=true
    echo "Workspace version: $current -> $version"
fi

git tag -a "$tag" -m "$message"
echo "Created tag $tag on $(git rev-parse --short HEAD)"

read -r -p "Push main and $tag to origin? [y/N] " answer
if [[ "$answer" =~ ^[Yy]$ ]]; then
    if $redo; then
        if gh release view "$tag" >/dev/null 2>&1; then
            gh release delete "$tag" --yes
            echo "Deleted release $tag"
        fi
        if git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null; then
            git push --quiet origin ":refs/tags/$tag"
            echo "Deleted remote tag $tag"
        fi
    fi
    git push --atomic origin main "$tag"
else
    echo "Not pushed. To push: git push --atomic origin main $tag"
    undo="git tag -d $tag"
    $redo && undo+=" && git fetch origin tag $tag"
    $committed && undo+=" && git reset --hard HEAD~1"
    echo "To undo: $undo"
fi
