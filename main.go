//go:build !windows

package main

const HAS_SERVICE_MAN = false

// this app is not really meant for anything else but windows.
// this file is here to allow some kind of developement to occur
// into the other platforms
func main() {
	r := newRunner("wsw", "")

	r.Start()
	<-r.Quit
}
