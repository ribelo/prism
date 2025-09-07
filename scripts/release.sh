#!/usr/bin/env bash

set -e

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ] || [ ! -d ".git" ]; then
    echo "ERROR: Run this from the project root directory"
    exit 1
fi

# Check if working directory is clean
if ! git diff-index --quiet HEAD --; then
    echo "ERROR: Working directory is not clean. Commit your changes first."
    git status --porcelain
    exit 1
fi

echo "Working directory is clean"

# Run regular tests (integration tests require running server)
echo "Running tests..."
cargo test

# Get commit count for version
COMMIT_COUNT=$(git rev-list --count HEAD)
VERSION="v0.0.${COMMIT_COUNT}"

echo "Creating version: ${VERSION}"

# Check if tag already exists
if git tag -l | grep -q "^${VERSION}$"; then
    echo "ERROR: Tag ${VERSION} already exists"
    exit 1
fi

# Create tag
git tag -a "${VERSION}" -m "Release ${VERSION}"

# Push everything
echo "Pushing to origin..."
git push origin $(git branch --show-current)
git push origin "${VERSION}"

echo "Done! CI will build release at: https://github.com/ribelo/prism/releases/tag/${VERSION}"