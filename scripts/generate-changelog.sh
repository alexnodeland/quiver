#!/bin/bash
# Generate CHANGELOG.md from git history
# Usage: ./scripts/generate-changelog.sh [--output FILE]

set -e

OUTPUT_FILE=".github/CHANGELOG.md"
REPO_URL="https://github.com/alexnodeland/quiver"

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --output|-o)
            OUTPUT_FILE="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [--output FILE]"
            echo "Generate CHANGELOG.md from git history"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

echo "Generating changelog..."

# Get all version tags (assuming format v0.0.0 or 0.0.0)
TAGS=$(git tag -l --sort=-v:refname 2>/dev/null | grep -E '^v?[0-9]+\.[0-9]+\.[0-9]+' || true)

# Start building the changelog
cat > "$OUTPUT_FILE" << 'HEADER'
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This changelog is auto-generated from git history. Run `make changelog` to update.

HEADER

# Add unreleased section
LATEST_TAG=$(echo "$TAGS" | head -n1)
if [ -n "$LATEST_TAG" ]; then
    UNRELEASED_COMMITS=$(git log "$LATEST_TAG"..HEAD --oneline 2>/dev/null | wc -l)
else
    UNRELEASED_COMMITS=$(git log --oneline 2>/dev/null | wc -l)
fi

if [ "$UNRELEASED_COMMITS" -gt 0 ]; then
    echo "## [Unreleased]" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"

    if [ -n "$LATEST_TAG" ]; then
        echo "[Compare with $LATEST_TAG]($REPO_URL/compare/$LATEST_TAG...HEAD)" >> "$OUTPUT_FILE"
    fi
    echo "" >> "$OUTPUT_FILE"

    # Categorize unreleased commits
    declare -A CATEGORIES
    CATEGORIES["feat"]="### Added"
    CATEGORIES["fix"]="### Fixed"
    CATEGORIES["docs"]="### Documentation"
    CATEGORIES["refactor"]="### Changed"
    CATEGORIES["perf"]="### Performance"
    CATEGORIES["test"]="### Testing"
    CATEGORIES["chore"]="### Maintenance"

    for prefix in feat fix docs refactor perf test chore; do
        if [ -n "$LATEST_TAG" ]; then
            COMMITS=$(git log "$LATEST_TAG"..HEAD --oneline --grep="^$prefix" 2>/dev/null || true)
        else
            COMMITS=$(git log --oneline --grep="^$prefix" 2>/dev/null || true)
        fi

        if [ -n "$COMMITS" ]; then
            echo "" >> "$OUTPUT_FILE"
            echo "${CATEGORIES[$prefix]}" >> "$OUTPUT_FILE"
            echo "" >> "$OUTPUT_FILE"
            echo "$COMMITS" | while read -r line; do
                HASH=$(echo "$line" | cut -d' ' -f1)
                MSG=$(echo "$line" | cut -d' ' -f2-)
                # Extract PR number if present
                PR_NUM=$(echo "$MSG" | grep -oE '#[0-9]+' | head -1 || true)
                if [ -n "$PR_NUM" ]; then
                    echo "- $MSG ([${HASH:0:7}]($REPO_URL/commit/$HASH))" >> "$OUTPUT_FILE"
                else
                    echo "- $MSG ([${HASH:0:7}]($REPO_URL/commit/$HASH))" >> "$OUTPUT_FILE"
                fi
            done
        fi
    done

    # Also include uncategorized commits
    if [ -n "$LATEST_TAG" ]; then
        OTHER_COMMITS=$(git log "$LATEST_TAG"..HEAD --oneline 2>/dev/null | grep -vE "^[a-f0-9]+ (feat|fix|docs|refactor|perf|test|chore)" || true)
    else
        OTHER_COMMITS=$(git log --oneline 2>/dev/null | head -20 | grep -vE "^[a-f0-9]+ (feat|fix|docs|refactor|perf|test|chore)" || true)
    fi

    if [ -n "$OTHER_COMMITS" ]; then
        echo "" >> "$OUTPUT_FILE"
        echo "### Other" >> "$OUTPUT_FILE"
        echo "" >> "$OUTPUT_FILE"
        echo "$OTHER_COMMITS" | while read -r line; do
            HASH=$(echo "$line" | cut -d' ' -f1)
            MSG=$(echo "$line" | cut -d' ' -f2-)
            echo "- $MSG ([${HASH:0:7}]($REPO_URL/commit/$HASH))" >> "$OUTPUT_FILE"
        done
    fi

    echo "" >> "$OUTPUT_FILE"
fi

# Process each version tag
PREV_TAG=""
for TAG in $TAGS; do
    echo "" >> "$OUTPUT_FILE"

    # Get tag date
    TAG_DATE=$(git log -1 --format=%ai "$TAG" 2>/dev/null | cut -d' ' -f1)

    echo "## [$TAG] - $TAG_DATE" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"

    if [ -n "$PREV_TAG" ]; then
        echo "[Compare with previous]($REPO_URL/compare/$TAG...$PREV_TAG)" >> "$OUTPUT_FILE"
    fi
    echo "" >> "$OUTPUT_FILE"

    # Get commits between tags
    if [ -n "$PREV_TAG" ]; then
        RANGE="$TAG...$PREV_TAG"
    else
        # For the first tag, get all commits up to it
        RANGE="$TAG"
    fi

    # Get commit messages (from merge commits, which are typically PRs)
    git log "$RANGE" --oneline --first-parent 2>/dev/null | head -50 | while read -r line; do
        HASH=$(echo "$line" | cut -d' ' -f1)
        MSG=$(echo "$line" | cut -d' ' -f2-)
        echo "- $MSG ([${HASH:0:7}]($REPO_URL/commit/$HASH))" >> "$OUTPUT_FILE"
    done

    PREV_TAG="$TAG"
done

# If no tags exist, show recent commits
if [ -z "$TAGS" ]; then
    echo "## Recent Changes" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
    echo "*No version tags found. Showing recent commits.*" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"

    git log --oneline -30 2>/dev/null | while read -r line; do
        HASH=$(echo "$line" | cut -d' ' -f1)
        MSG=$(echo "$line" | cut -d' ' -f2-)
        echo "- $MSG ([${HASH:0:7}]($REPO_URL/commit/$HASH))" >> "$OUTPUT_FILE"
    done
fi

echo "" >> "$OUTPUT_FILE"
echo "---" >> "$OUTPUT_FILE"
echo "" >> "$OUTPUT_FILE"
echo "*Generated on $(date -u +%Y-%m-%d) by \`make changelog\`*" >> "$OUTPUT_FILE"

echo "Changelog generated: $OUTPUT_FILE"
