package main

import (
	"bufio"
	"fmt"
	"io"
	"log"
	"os"
	"os/exec"
	"path/filepath"
	"sync"

	"gopkg.in/natefinch/lumberjack.v2"
)

var (
	loggers map[string]*log.Logger
	logMU   sync.Mutex
)

func init() {
	loggers = make(map[string]*log.Logger)
}

func getLogger(name string) *log.Logger {
	logMU.Lock()
	defer logMU.Unlock()

	if l, ok := loggers[name]; ok {
		return l
	}

	logger := log.New(os.Stdout, "", log.LstdFlags)

	fileName := filepath.Join(GetLauncherDir(), fmt.Sprintf("%s.log", name))
	jack := &lumberjack.Logger{
		Filename:   fileName,
		MaxSize:    1,
		MaxBackups: 2,
		MaxAge:     7,
	}

	multi := io.MultiWriter(jack, os.Stdout)
	logger.SetOutput(multi)

	loggers[name] = logger
	return logger
}

func GetLauncherDir() string {
	ex, err := os.Executable()
	if err != nil {
		panic(err)
	}
	exPath := filepath.Dir(ex)
	return exPath
}

func RunCmd(name string, command string, args []string, env []string) (*exec.Cmd, error) {
	l := getLogger("launcher")
	cmd := exec.Command(command, args...)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		l.Println(err)
		return nil, err
	}
	go printStream(name, stdout)

	stderr, err := cmd.StderrPipe()
	if err != nil {
		l.Println(err)
		return nil, err
	}
	go printStream(name, stderr)

	cmd.Env = env
	// This is NON blocking and do not wait for the command to end
	cerr := cmd.Start()
	if cerr != nil {
		l.Fatal(cerr)
	}

	return cmd, nil
}

func printStream(name string, stream io.ReadCloser) {
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
