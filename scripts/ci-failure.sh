#!/usr/bin/env bash
# Print only the lines of a CI run that say why it failed.
#
#   scripts/ci-failure.sh [run-id]
#
# `gh run view --log-failed | grep error` does not work on this repository's
# logs, twice over: the dependency list is full of crates named for errors
# (`Downloaded thiserror`, `Compiling error-code`), and cargo colours its
# output, so the bytes between "error" and ":" are an ANSI escape and the
# tighter pattern matches nothing but the runner's own "##[error]Process
# completed". This strips the prefix and the colour first, then matches.
#
# With no run id it takes the newest run on main that finished as a failure.
# CI cancels a run its successor supersedes, so the newest run is often
# cancelled or still going and has no failure to show.
#
# Environment:
#   CI_FAILURE_CONTEXT   lines kept after the line that opens a diagnostic (6)
set -euo pipefail

die() {
    echo "ci-failure: $*" >&2
    exit 1
}

command -v gh >/dev/null 2>&1 || die "gh is not on PATH"
command -v awk >/dev/null 2>&1 || die "awk is not on PATH"

context=${CI_FAILURE_CONTEXT:-6}
case "$context" in
    '' | *[!0-9]*) die "CI_FAILURE_CONTEXT must be a whole number, got '$context'" ;;
esac

run_id=${1:-}
if [ -z "$run_id" ]; then
    run_id=$(gh run list --workflow CI --branch main --status failure --limit 1 \
        --json databaseId --jq '.[0].databaseId // empty')
    [ -n "$run_id" ] || die "no failed CI run on main"
fi

# Says which run this is, because the default is "the last failure", which
# is not the same as "main is red now".
gh run view "$run_id" --json databaseId,conclusion,headSha,createdAt,displayTitle \
    --jq '"run \(.databaseId)  \(.conclusion // "unfinished")  \(.headSha[0:7])  \(.createdAt)  \(.displayTitle)"'

# Each log line is "<job>\t<step>\t<timestamp> <text>". The match wants
# "error" at a word start so `thiserror:` in a path or message stays out.
#
# Colour arrives in two spellings: a real ESC, and the two characters "^["
# that gh substitutes for it when its output is not a terminal. Stripping
# only the first is how a pipe ends up matching nothing.
#
# Only a line that opens a diagnostic carries context, and only as far as the
# blank line that ends it; a test's "FAILED" line would otherwise drag in the
# passing tests printed after it.
gh run view "$run_id" --log-failed | awk -v context="$context" -v run="$run_id" '
    BEGIN {
        esc = sprintf("%c", 27)
        colour = "(" esc "|\\^\\[)\\[[0-9;]*[A-Za-z]"
        opens = "(^|[^[:alnum:]_-])error(\\[|:)|panicked at"
        alone = "FAILED|left:|right:|test result: F"
    }
    {
        split($0, field, "\t")
        block = field[1] " / " field[2]
        text = $0
        sub(/^[^\t]*\t[^\t]*\t/, "", text)
        gsub(colour, "", text)
        sub(/^\357\273\277/, "", text)
        sub(/^[0-9-]+T[0-9:.]+Z ?/, "", text)

        if (block != current) {
            current = block
            announced = 0
            remaining = 0
        }

        if (text ~ /##\[error\]/) {
            # The last word on a step; what follows is job cleanup.
            remaining = 1
        } else if (text ~ opens) {
            remaining = context + 1
        } else if (text ~ /^[[:space:]]*$/) {
            remaining = 0
        } else if (text ~ alone && remaining == 0) {
            remaining = 1
        }
        if (remaining == 0) {
            gap = 1
            next
        }

        if (!announced) {
            printf "\n== %s ==\n", block
            announced = 1
        } else if (gap) {
            print "--"
        }
        gap = 0
        remaining--
        print text
        found = 1
    }
    END {
        if (!found) {
            print "ci-failure: no failure lines matched; read the log with: gh run view " run " --log-failed" > "/dev/stderr"
            exit 1
        }
    }
'
