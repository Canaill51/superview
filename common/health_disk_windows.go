//go:build windows

package common

import "golang.org/x/sys/windows"

func getFreeDiskGB(path string) (float64, error) {
	pathPtr, err := windows.UTF16PtrFromString(path)
	if err != nil {
		return 0, err
	}

	var freeBytesAvailable uint64
	var totalBytes uint64
	var freeBytes uint64
	if err := windows.GetDiskFreeSpaceEx(pathPtr, &freeBytesAvailable, &totalBytes, &freeBytes); err != nil {
		return 0, err
	}

	freeGB := float64(freeBytes) / (1024 * 1024 * 1024)
	return freeGB, nil
}
