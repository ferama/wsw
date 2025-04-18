//go:build service_man && windows

package main

import (
	"fmt"
	"log"
	"os"
	"time"

	wsvc "golang.org/x/sys/windows/svc"
	"golang.org/x/sys/windows/svc/mgr"
)

const HAS_SERVICE_MAN = true

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
		_, err = s.Control(wsvc.Stop)
		if err != nil {
			log.Printf("Warning: could not stop service %q: %v\n", name, err)
		} else {
			log.Printf("Stopping service %q...\n", name)
			time.Sleep(2 * time.Second) // Give it time to shut down
		}
	}

	err = s.Delete()
	if err != nil {
		return err
	}

	log.Printf("Service %q uninstalled.\n", name)
	return nil
}
