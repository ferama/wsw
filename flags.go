package main

import "flag"

var (
	wrappedCmdFlag       string
	serviceNameFlag      string
	workingDirFlag       string
	installServiceFlag   bool
	uninstallServiceFlag bool
)

const (
	CMD_FLAG          = "cmd"
	SERVICE_NAME_FLAG = "service-name"
	WORKING_DIR_FLAG  = "working-dir"
)

func init() {
	flag.StringVar(&wrappedCmdFlag, CMD_FLAG, "", "Path to the executable with arguments to run")
	flag.StringVar(&serviceNameFlag, SERVICE_NAME_FLAG, "", "Service name suffix")
	flag.StringVar(&workingDirFlag, WORKING_DIR_FLAG, "", "Service workging directory (default to service executable directory)")
	flag.BoolVar(&installServiceFlag, "install-service", false, "Install the service")
	flag.BoolVar(&uninstallServiceFlag, "uninstall-service", false, "Uninstall the service")

	flag.Parse()

}
