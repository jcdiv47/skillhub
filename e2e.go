//go:build e2e

package main

// End-to-end builds let another process drive the app through the
// automation plugin: go build -tags e2e, then MYGO_AUTOMATION=<socket>.
import _ "github.com/egoist/mygo/plugins/automation"
