#!/usr/bin/env fish

set MESSAGE "Build completed!"

argparse m= -- $argv
or begin
    echo "Usage: notify.fish -m \"message\""
    exit 1
end

if set -q _flag_m
    set MESSAGE $_flag_m
end

powershell.exe -NoProfile -Command "msg * \"$MESSAGE\""

