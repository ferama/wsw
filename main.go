//go:build !windows

package main

// this app is not really meant for anything else but windows.
// this file is here to allow some kind of developement to occur
// into the other platforms
func main() {
	r := newRunner()

	r.Start()
	<-r.Quit
}
