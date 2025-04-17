package main

import (
	"bufio"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

const (
	STATUS_STOPPED = iota
	STATUS_STARTED
)

func newRunner(name string, workingDir string) *runner {
	r := &runner{
		Quit:       make(chan bool),
		name:       name,
		workingDir: workingDir,
	}
	l := getLogger(r.name)
	l.Printf("runner created: %s", name)
	return r
}

// The runner is a generic application that
// will be launched using a blocking command
// runner will watch for process health and restart the application
// if it fails
type runner struct {
	workingDir string
	execCmd    *exec.Cmd
	status     int
	name       string

	mu             sync.Mutex
	watcherStarted bool

	Quit chan bool
}

func (r *runner) watcher() {
	l := getLogger(r.name)

	for {
		select {
		case <-r.Quit:
			r.watcherStarted = false
			return
		case <-time.After(5 * time.Second):
			var status int
			r.mu.Lock()
			status = r.status
			r.mu.Unlock()

			if status == STATUS_STARTED {
				if !r.healty() {
					l.Println("restarting...")
					r.Start()
				}
			}

		}
	}
}

func (r *runner) runCmd(name string, exePath string, args []string, env []string) (*exec.Cmd, error) {
	l := getLogger(name)

	cmd := exec.Command(exePath, args...)
	if r.workingDir != "" {
		cmd.Dir = r.workingDir
	} else {
		// if no working dir is set, use the executable path
		// as working dir
		// this is the default behavior of windows services
		// and we should keep it
		// for consistency
		cmd.Dir = filepath.Dir(exePath)
	}

	stdout, err := cmd.StdoutPipe()
	if err != nil {
		l.Println(err)
		return nil, err
	}
	go r.printStream(name, stdout)

	stderr, err := cmd.StderrPipe()
	if err != nil {
		l.Println(err)
		return nil, err
	}
	go r.printStream(name, stderr)

	cmd.Env = env
	// This is NON blocking and do not wait for the command to end
	cerr := cmd.Start()
	if cerr != nil {
		l.Fatal(cerr)
	}

	return cmd, nil
}

func (r *runner) printStream(name string, stream io.ReadCloser) {
	buffer := bufio.NewReader(stream)
	for {
		line, err := buffer.ReadString('\n')
		if len(line) == 0 && err != nil {
			if err == io.EOF {
				return
			}
		}
		l := getLogger(name)
		l.Print(line)
	}
}

func (r *runner) Start() {
	r.mu.Lock()
	defer r.mu.Unlock()

	if !r.watcherStarted {
		r.watcherStarted = true
		go r.watcher()
	}

	l := getLogger(r.name)
	l.Printf("starting... '%s'", wrappedCmdFlag)

	parsed := strings.Fields(wrappedCmdFlag)

	if len(parsed) == 0 {
		l.Fatal("no command found")
	}

	cmd := parsed[0]
	args := parsed[1:]
	l.Printf("cmd: '%s', args: '%s'", cmd, args)
	r.execCmd, _ = r.runCmd(r.name, cmd, args, os.Environ())

	// without this call, the ProcessState is not fullfilled and we
	// don't have anything to check. Run in a goroutine to take status
	// in non blocking way
	go r.execCmd.Wait()
	r.status = STATUS_STARTED
}

func (r *runner) Stop() {
	r.mu.Lock()
	defer r.mu.Unlock()

	close(r.Quit)
	if r.execCmd != nil && r.execCmd.Process != nil {
		r.execCmd.Process.Kill()
		r.execCmd.Process.Release()
	}
	r.status = STATUS_STOPPED
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
