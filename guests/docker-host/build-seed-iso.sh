#!/bin/sh
# Builds the cloud-init seed ISO that the Docker host's first boot reads from its DVD drive.
#
# Usage: build-seed-iso.sh <public key file> <output ISO>
#
# The ISO holds user-data, with __PUBLIC_KEY__ replaced by the key the runner logs on to the host with, and a
# meta-data naming the instance. cloud-init's NoCloud source reads both from a volume labelled cidata. The ISO is
# written with xorriso in a container, so the runner needs Docker and nothing else.
set -eu

public_key=$1
output=$2
here=$(cd "$(dirname "$0")" && pwd)

sed "s|__PUBLIC_KEY__|$(cat "$public_key")|" "$here/user-data" |
    docker run --rm -i alpine:3.20 sh -c '
        apk add -q xorriso &&
        cd /tmp &&
        cat > user-data &&
        printf "instance-id: vmtest-kvm-1\nlocal-hostname: vmtest-kvm\n" > meta-data &&
        xorriso -as mkisofs -quiet -V cidata -J -r -o seed.iso user-data meta-data >&2 &&
        cat seed.iso' > "$output"
