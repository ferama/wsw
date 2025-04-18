package main

import (
	"flag"
	"fmt"
	"log"
	"os"
	"os/signal"
	"sync"
	"syscall"
	"time"

	"github.com/judwhite/go-svc"
	wsvc "golang.org/x/sys/windows/svc"
	"golang.org/x/sys/windows/svc/mgr"
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

func installService(name string, args string, workingDir string) error {
	m, err := mgr.Connect()
	if err != nil {
		return err
	}
	defer m.Disconnect()

	exepath, err := os.Executable()
	if err != nil {
		return err
	}

	s, err := m.CreateService(name, exepath, mgr.Config{
		DisplayName: name,
		StartType:   mgr.StartAutomatic,
	},
		fmt.Sprintf("-%s=\"%s\"", SERVICE_NAME_FLAG, name),
		fmt.Sprintf("-%s=\"%s\"", CMD_FLAG, args),
		fmt.Sprintf("-%s=\"%s\"", WORKING_DIR_FLAG, workingDir),
	)
	if err != nil {
		return err
	}
	defer s.Close()

	err = s.Start()
	if err != nil {
		return fmt.Errorf("service installed but failed to start: %w", err)
	}

	log.Printf("Service %q installed and started.\n", name)

	return nil
}

func uninstallService(name string) error {
	m, err := mgr.Connect()
	if err != nil {
		return err
	}
	defer m.Disconnect()

	s, err := m.OpenService(name)
	if err != nil {
		return err
	}
	defer s.Close()

	// Try stopping the service if running
	status, err := s.Query()
	if err == nil && status.State == wsvc.Running {
		log.Printf("Stopping service %q...\n", name)
		_, err = s.Control(wsvc.Stop)
		if err != nil {
			log.Printf("Warning: could not stop service %q: %v\n", name, err)
		} else {
			// Poll until it's stopped or timeout
			deadline := time.Now().Add(stopTimeout)
			for time.Now().Before(deadline) {
				status, err = s.Query()
				if err != nil {
					break
				}
				if status.State != wsvc.Stopped {
					time.Sleep(500 * time.Millisecond)
				} else {
					break
				}
			}
		}
	}

	err = s.Delete()
	if err != nil {
		return err
	}

	log.Printf("Service %q uninstalled.\n", name)
	return nil
}

func main() {
	isWindowsService, err := wsvc.IsWindowsService()
	if err != nil {
		panic(err)
	}

	serviceName := "wsw"
	if serviceNameFlag != "" {
		serviceName = fmt.Sprintf("%s-%s", serviceName, unwrapQuotes(serviceNameFlag))
	}
	if isWindowsService {
		serviceName = unwrapQuotes(serviceNameFlag)
		workingDirFlag = unwrapQuotes(workingDirFlag)
	}

	if installServiceFlag && wrappedCmdFlag != "" {
		err := installService(serviceName, wrappedCmdFlag, workingDirFlag)
		if err != nil {
			log.Fatalf("Install failed: %v", err)
		}
		return
	}
	if uninstallServiceFlag {
		err := uninstallService(serviceName)
		if err != nil {
			log.Fatalf("Uninstall failed: %v", err)
		}
		return
	}

	if wrappedCmdFlag == "" {
		flag.PrintDefaults()
		return
	}

	runner := newRunner(serviceName, workingDirFlag)
	if isWindowsService {
		wrappedCmdFlag = unwrapQuotes(wrappedCmdFlag)
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
