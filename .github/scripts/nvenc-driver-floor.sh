#!/bin/sh
# Read the NVENC driver floor compiled into an FFmpeg binary, and assert it.
#
# Why this exists
# ---------------
# FFmpeg decides at *compile* time which NVENC API version it will ask the
# driver for, from the nv-codec-headers it is built against. At runtime it
# refuses to encode if the driver is older:
#
#   Driver does not support the required nvenc API version. Required: 13.1 Found: 13.0
#   The minimum required Nvidia driver for nvenc is 610.00 or newer
#
# The version it prints comes from a table of string literals in
# libavcodec/nvenc.c, exactly one branch of which survives compilation. So the
# floor is readable straight out of any build, and two binaries both calling
# themselves "FFmpeg 8.1.2" can carry different ones -- which is how a user's
# RTX A1000 lost hardware encoding after a routine upgrade (ANALYSE.md U-03).
#
# Superview ships a build whose floor it has measured. This turns that
# measurement into something a release cannot get wrong: bump the pin to a
# build compiled against newer headers and the release fails here, instead of
# shipping and taking NVENC away from every machine below the new floor.
#
# tr rather than strings(1): the Windows runner's bash is Git Bash, which has
# no strings.

# Every floor libavcodec/nvenc.c can print, so an unexpected one is reported as
# what it is rather than as "not found".
NVENC_KNOWN_FLOORS='610\.00|570\.0|551\.76|550\.54\.14|531\.61|530\.41\.03|522\.25|520\.56\.06|471\.41|470\.57\.02|456\.71|455\.28|450\.51|445\.87|436\.15|435\.21|418\.81|418\.30|397\.93|396\.24|390\.25|378\.66|378\.13'

# driver_floor_of BINARY -- prints the floor found, or nothing.
driver_floor_of() {
    tr '\0' '\n' < "$1" \
        | grep -oE "^(${NVENC_KNOWN_FLOORS})$" \
        | sort -u \
        | tr '\n' ' ' \
        | sed 's/ *$//'
}

# assert_driver_floor EXPECTED BINARY... -- fails the job on any mismatch.
assert_driver_floor() {
    expected="$1"
    shift
    status=0

    for binary in "$@"; do
        found="$(driver_floor_of "$binary")"
        if [ "$found" = "$expected" ]; then
            echo "$binary: NVENC driver floor $found"
            continue
        fi
        if [ -z "$found" ]; then
            echo "::error::$binary carries no known NVENC driver floor. It was probably built without nvenc, so this release would ship an FFmpeg that cannot use NVIDIA hardware at all."
        else
            echo "::error::$binary demands driver '$found', not the '$expected' this release is pinned to. A build compiled against newer nv-codec-headers takes NVENC away from every machine below the new floor -- see ANALYSE.md U-03 before changing the pin."
        fi
        status=1
    done

    return $status
}
