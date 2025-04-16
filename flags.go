package main

import "flag"

var (
	wrappedPathFlag      string
	serviceNameFlag      string
	installServiceFlag   bool
	uninstallServiceFlag bool
)

func init() {
	flag.StringVar(&wrappedPathFlag, "path", "", "Path to the executable with arguments to run")
	flag.StringVar(&serviceNameFlag, "service-name", "", "service name suffix")
	flag.BoolVar(&installServiceFlag, "install-service", false, "Install the service")
	flag.BoolVar(&uninstallServiceFlag, "uninstall-service", false, "Uninstall the service")

	flag.Parse()

}
