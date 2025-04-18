package main

import (
	"fmt"
	"log"
	"os"
	"strings"
	"time"

	wsvc "golang.org/x/sys/windows/svc"
	"golang.org/x/sys/windows/svc/mgr"
)

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

func listServices() error {
	m, err := mgr.Connect()
	if err != nil {
		return err
	}
	defer m.Disconnect()

	services, err := m.ListServices()
	if err != nil {
		return err
	}

	fmt.Printf("%-30s %-15s\n", "Service Name", "Status")
	fmt.Println(strings.Repeat("-", 45))

	for _, service := range services {
		if strings.HasPrefix(service, SERVICE_NAME_PREFIX) {
			s, err := m.OpenService(service)
			if err != nil {
				fmt.Printf("%-30s %-15s\n", service, "Error")
				continue
			}

			status, err := s.Query()
			s.Close()
			if err != nil {
				fmt.Printf("%-30s %-15s\n", service, "Error")
				continue
			}

			var state string
			switch status.State {
			case wsvc.Stopped:
				state = "Stopped"
			case wsvc.StartPending:
				state = "Start Pending"
			case wsvc.StopPending:
				state = "Stop Pending"
			case wsvc.Running:
				state = "Running"
			case wsvc.ContinuePending:
				state = "Continue Pending"
			case wsvc.PausePending:
				state = "Pause Pending"
			case wsvc.Paused:
				state = "Paused"
			default:
				state = "Unknown"
			}

			fmt.Printf("%-30s %-15s\n", service, state)
		}
	}

	return nil
}
