package main

import "flag"

var (
	wrappedCmdFlag       string
	serviceNameFlag      string
	installServiceFlag   bool
	uninstallServiceFlag bool
)

const (
	CMD_FLAG          = "cmd"
	SERVICE_NAME_FLAG = "service-name"
)

func init() {
	flag.StringVar(&wrappedCmdFlag, CMD_FLAG, "", "Path to the executable with arguments to run")
	flag.StringVar(&serviceNameFlag, SERVICE_NAME_FLAG, "", "service name suffix")
	flag.BoolVar(&installServiceFlag, "install-service", false, "Install the service")
	flag.BoolVar(&uninstallServiceFlag, "uninstall-service", false, "Uninstall the service")

	flag.Parse()

}
