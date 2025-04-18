package main

import (
	"flag"
	"fmt"

	wsvc "golang.org/x/sys/windows/svc"
)

var (
	wrappedCmdFlag  string
	serviceNameFlag string
	workingDirFlag  string

	installServiceFlag   bool
	uninstallServiceFlag bool
	listServicesFlag     bool
)

const (
	CMD_FLAG          = "cmd"
	SERVICE_NAME_FLAG = "name"
	WORKING_DIR_FLAG  = "wd"

	SERVICE_NAME_PREFIX = "wsw"
)

func init() {
	isWindowsService, err := wsvc.IsWindowsService()
	if err != nil {
		panic(err)
	}

	// Service flags
	flag.StringVar(&wrappedCmdFlag, CMD_FLAG, "", "path to the executable with arguments to run")
	flag.StringVar(&serviceNameFlag, SERVICE_NAME_FLAG, "", "service name suffix (final name will be wsw-<name>)")
	flag.StringVar(&workingDirFlag, WORKING_DIR_FLAG, "", "service working directory (default to service executable directory)")

	// Service management flags
	flag.BoolVar(&installServiceFlag, "i", false, "install the service.\nEx:\n  wsw.exe -i -name test\n  will install as wsw-test")
	flag.BoolVar(&uninstallServiceFlag, "u", false, "uninstall the service.\nEx:\n  wsw.exe -u -name test\n  will uninstall wsw-test")
	flag.BoolVar(&listServicesFlag, "l", false, "list all installed services")

	flag.Parse()

	if !isWindowsService {
		if serviceNameFlag != "" {
			serviceNameFlag = fmt.Sprintf("%s-%s", SERVICE_NAME_PREFIX, unwrapQuotes(serviceNameFlag))
		} else {
			serviceNameFlag = SERVICE_NAME_PREFIX
		}
	}

	if isWindowsService {
		serviceNameFlag = unwrapQuotes(serviceNameFlag)
		workingDirFlag = unwrapQuotes(workingDirFlag)
		wrappedCmdFlag = unwrapQuotes(wrappedCmdFlag)
	}
}
