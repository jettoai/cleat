import AppKit

// One binary, two jobs. A bare launch (from Finder, `open`, or the login item) is the daemon; a
// first argument that is not a flag is a CLI subcommand, which reads the files the daemon wrote
// and exits without ever starting NSApplication.
//
// The "not a flag" test is what keeps the two apart: LaunchServices starts an app with flag-shaped
// arguments only (`-NSTreatUnknownArgumentsAsOpen`, `-psn_...`), so a bare word is always a
// person at a terminal - including a misspelt one, which CLI answers with usage rather than
// silently launching a second daemon. The three conventional flags are the exception: nobody
// typing `cleat --help` wants a daemon, and LaunchServices never passes them.
let cleatArguments = Array(CommandLine.arguments.dropFirst())
let cliFlags: Set<String> = ["--help", "-h", "--version"]
if let first = cleatArguments.first, !first.hasPrefix("-") || cliFlags.contains(first) {
    exit(CLI.run(cleatArguments))
}

NSApplication.shared.delegate = AppDelegate.shared
NSApplication.shared.run()
