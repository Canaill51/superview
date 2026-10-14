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

	// freeBytesAvailable, not freeBytes: the first is what the calling user may
	// write ("If per-user quotas are being used, this value may be less than the
	// total number of free bytes on a disk"), the second the volume's total free
	// space. It is also what the Unix side reads, Bavail rather than Bfree.
	freeGB := float64(freeBytesAvailable) / (1024 * 1024 * 1024)
	return freeGB, nil
}
