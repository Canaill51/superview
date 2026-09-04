#!/bin/bash
#
# DEPRECATED — do not use for releases.
#
# This script predates the Linux port (commit 76a8341) and only builds Windows
# binaries. It also relies on fyne-cross v2, whose documented install command
# (`go get`) no longer installs binaries since Go 1.18.
#
# Releases are produced by .github/workflows/release.yml, which builds Windows
# and Linux artifacts on native runners, publishes checksums and creates a draft
# release. Tag the repository (`git tag vX.Y.Z && git push origin --tags`) and
# let CI do the rest.
#
# Kept only for historical reference. It will refuse to run unless
# SUPERVIEW_ALLOW_LEGACY_BUILD=1 is set.

if [ "${SUPERVIEW_ALLOW_LEGACY_BUILD:-0}" != "1" ]; then
    echo "build.sh is deprecated; releases are built by .github/workflows/release.yml." >&2
    echo "Set SUPERVIEW_ALLOW_LEGACY_BUILD=1 to run it anyway." >&2
    exit 1
fi

if [ $# -ne 1 ]; then
    echo "Usage: ./build.sh <version number>"
    echo "Suggested version: "$(git describe --tags | tr -d v | awk '{printf "%.1f", $1 + .1}')
    exit
fi

if ! command -v fyne-cross &> /dev/null; then
    echo "This build script requires fyne-cross v2 to be installed:"
    echo "go get github.com/lucor/fyne-cross/v2/cmd/fyne-cross"
    exit
fi

VERSION=$1

echo "Build GUI Windows packages with version number ${VERSION}"

platforms=("windows/amd64" "windows/386")
files=()

for platform in "${platforms[@]}"; do
    platform_split=(${platform//\// })
    GOOS=${platform_split[0]}
    GOARCH=${platform_split[1]}
    output_name="superview-gui-${GOOS}-${GOARCH}-v${VERSION}.exe"

    fyne-cross ${GOOS} -silent -arch ${GOARCH} -icon Icon.png -ldflags="-s -w -H=windowsgui" -output ${output_name} .
    output_name="fyne-cross/dist/${GOOS}-${GOARCH}/${output_name}.zip"

    echo "Built: ${output_name}"
    files+=($output_name)
done

git tag v${VERSION}
git push origin --tags
if command -v hub &> /dev/null; then
    hub release create -do $(for f in "${files[@]}"; do echo "-a "$f; done) -m "Release v${VERSION}" v${VERSION}
fi