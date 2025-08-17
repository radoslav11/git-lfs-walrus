#!/bin/bash
touch wrapper_was_run
echo "Wrapper script called with args: $@" >&2
yes | "$@"
