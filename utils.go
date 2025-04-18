package main

import (
	"fmt"
	"io"
	"log"
	"os"
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

	fileName := filepath.Join(getRunnerDir(), fmt.Sprintf("%s.log", name))
	jack := &lumberjack.Logger{
		Filename:   fileName,
		MaxSize:    25, // megabytes
		MaxBackups: 2,
		MaxAge:     7,
	}

	multi := io.MultiWriter(jack, os.Stdout)
	logger.SetOutput(multi)

	loggers[name] = logger
	return logger
}

func getRunnerDir() string {
	ex, err := os.Executable()
	if err != nil {
		panic(err)
	}
	exPath := filepath.Dir(ex)
	return exPath
}

func unwrapQuotes(s string) string {
	if len(s) >= 2 && s[0] == '"' && s[len(s)-1] == '"' {
		return s[1 : len(s)-1]
	}
	return s
}
