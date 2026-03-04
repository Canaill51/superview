//go:build !windows

package common

import "syscall"

func getFreeDiskGB(path string) (float64, error) {
	stat := syscall.Statfs_t{}
	if err := syscall.Statfs(path, &stat); err != nil {
		return 0, err
	}

	freeBytes := stat.Bavail * uint64(stat.Bsize)
	freeGB := float64(freeBytes) / (1024 * 1024 * 1024)
	return freeGB, nil
}
