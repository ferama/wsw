package main

import (
	"fmt"
	"os"
	"time"
)

func main() {
	// Check for log prefix argument
	if len(os.Args) < 2 {
		fmt.Println("Usage: go run main.go <log-prefix>")
		return
	}
	logFilePath := os.Args[1]

	file, err := os.Create(logFilePath)
	if err != nil {
		fmt.Println("Error creating file:", err)
		return
	}
	defer file.Close()

	for {
		now := time.Now().Format(time.RFC3339)
		_, err := file.WriteString(now + "\n")
		if err != nil {
			fmt.Println("Error writing to file:", err)
			return
		}

		file.Sync()

		time.Sleep(1 * time.Second)
	}
}
