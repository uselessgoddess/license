#!/bin/sh
# Setup git hooks for local development
# Run this once after cloning the repository

git config core.hooksPath .githooks
echo "Git hooks configured. Pre-commit hook will run fmt, clippy, and test."
