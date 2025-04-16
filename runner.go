package main

import (
	"flag"
	"os"
	"os/exec"
	"strings"
	"sync"
	"time"
)

const (
	STATUS_STOPPED = iota
	STATUS_STARTED
)

func newRunner() *runner {
	return &runner{
		Quit: make(chan bool),
		Name: "wsw",
	}
}

// The runner is a generic application that
// will be launched using a blocking command
// runner will watch for process health and restart the application
// if it fails
type runner struct {
	execCmd *exec.Cmd
	status  int

	mu             sync.Mutex
	watcherStarted bool

	Quit chan bool
	Name string
}

func (c *runner) watcher() {
	l := getLogger(c.Name)

	for {
		select {
		case <-c.Quit:
			c.watcherStarted = false
			return
		case <-time.After(5 * time.Second):
			var status int
			c.mu.Lock()
			status = c.status
			c.mu.Unlock()

			if status == STATUS_STARTED {
				if !c.healty() {
					l.Println("restarting...")
					c.Start()
				}
			}

		}
	}
}

func (c *runner) Start() {
	c.mu.Lock()
	defer c.mu.Unlock()

	flag.Parse()

	if !c.watcherStarted {
		c.watcherStarted = true
		go c.watcher()
	}

	l := getLogger(c.Name)
	l.Println("starting...", os.Args[1:])
	osArgs := os.Args[1:]

	parsed := splitWindowsArgs(strings.Join(osArgs, " "))

	if len(parsed) == 0 {
		l.Fatal("no command found")
	}

	cmd := parsed[0]
	args := parsed[1:]
	l.Printf("cmd: %s, args: %s", cmd, args)
	c.execCmd, _ = runCmd(c.Name, cmd, args, os.Environ())

	// without this call, the ProcessState is not fullfilled and we
	// don't have anything to check. Run in a goroutine to take status
	// in non blocking way
	go c.execCmd.Wait()
	c.status = STATUS_STARTED
}

func (c *runner) Stop() {
	c.mu.Lock()
	defer c.mu.Unlock()

	close(c.Quit)
	if c.execCmd != nil && c.execCmd.Process != nil {
		c.execCmd.Process.Kill()
		c.execCmd.Process.Release()
	}
	c.status = STATUS_STOPPED
}

// reports any health issues
func (c *runner) healty() bool {
	if c.execCmd != nil {
		if c.execCmd.ProcessState != nil && c.execCmd.ProcessState.Exited() {
			return false
		}
		return true
	}
	// no command, no health issues
	return true
}
