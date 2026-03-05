#!/bin/bash

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

    fyne-cross ${GOOS} -silent -arch ${GOARCH} -icon Icon.png -ldflags="-s -w -H=windowsgui" -output ${output_name} "superview-gui.go"
    output_name="fyne-cross/dist/${GOOS}-${GOARCH}/${output_name}.zip"

    echo "Built: ${output_name}"
    files+=($output_name)
done

git tag v${VERSION}
git push origin --tags
if command -v hub &> /dev/null; then
    hub release create -do $(for f in "${files[@]}"; do echo "-a "$f; done) -m "Release v${VERSION}" v${VERSION}
fi