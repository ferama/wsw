package main

import (
	"flag"
	"log"
	"os"
	"os/signal"
	"sync"
	"syscall"
	"time"

	"github.com/judwhite/go-svc"
	wsvc "golang.org/x/sys/windows/svc"
)

const stopTimeout = 10 * time.Second

type app struct {
	runner *runner

	wg   sync.WaitGroup
	quit chan bool
}

func (a *app) Init(env svc.Environment) error {
	return nil
}

func (a *app) Start() error {
	// The Start method must not block, or Windows may assume your service failed
	// to start. Launch a Goroutine here to do something interesting/blocking.

	a.wg.Add(1)
	go func() {
		go a.runner.Start()

		<-a.quit
		a.runner.Stop()
		a.wg.Done()
	}()

	return nil
}

// Stop shutdown the windows service
func (a *app) Stop() error {
	// The Stop method is invoked by stopping the Windows service, or by pressing Ctrl+C on the console.
	// This method may block, but it's a good idea to finish quickly or your process may be killed by
	// Windows during a shutdown/reboot. As a general rule you shouldn't rely on graceful shutdown.
	close(a.quit)
	a.wg.Wait()
	return nil
}

func main() {
	isWindowsService, err := wsvc.IsWindowsService()
	if err != nil {
		panic(err)
	}
	// Service managment
	switch {
	case installServiceFlag && wrappedCmdFlag != "":
		err := installService(serviceNameFlag, wrappedCmdFlag, workingDirFlag)
		if err != nil {
			log.Fatalf("Install failed: %v", err)
		}
		return
	case uninstallServiceFlag:
		err := uninstallService(serviceNameFlag)
		if err != nil {
			log.Fatalf("Uninstall failed: %v", err)
		}
		return
	case listServicesFlag:
		err := listServices()
		if err != nil {
			log.Fatalf("Failed to list services: %v", err)
		}
		return
	}

	if wrappedCmdFlag == "" {
		flag.Usage()
		return
	}

	runner := newRunner(serviceNameFlag, workingDirFlag)
	if isWindowsService {
		prg := &app{
			runner: runner,
			quit:   make(chan bool),
		}

		if err := svc.Run(prg); err != nil {
			panic(err)
		}
	} else {
		// run as usual if we are not running as windows service
		runner.Start()

		c := make(chan os.Signal, 1)
		signal.Notify(c, os.Interrupt, syscall.SIGTERM)
		<-c
	}
}
