//go:build !service_man && windows

package main

const HAS_SERVICE_MAN = false

func installService(name string, args string, workingDir string) error {
	return nil
}
func uninstallService(name string) error {
	return nil
}
