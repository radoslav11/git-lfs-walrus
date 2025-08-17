#!/bin/bash
echo "Wrapper script called with args: $@" >&2
yes | "$@"
