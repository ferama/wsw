# WSW - Windows Service Wrapper

> 💡 A tiny, practical tool that lets **any executable or script** run as a **real Windows service**, with zero boilerplate.


## 🚀 What is WSW?

Running background applications or daemons as Windows services should be easy — but in reality, **Windows makes it tricky**. 

If you've ever tried to:

- Wrap a custom binary as a service  
- Automatically restart a tool on failure  
- Avoid writing a Windows service in C++ or wrestling with the Windows API  
- Escape painful `sc.exe` syntax and quoting errors  

**Then WSW is for you.** It covers what [WinSW](https://github.com/winsw/winsw) does, as a single static executable that needs no .NET runtime and no XML file: one command line installs the service.


## ✅ Features

- 🧠 **Simple CLI interface**: one command installs and starts your service  
- ⚙️ **Wraps any executable** — started directly with its arguments, or through `cmd.exe`  
- 🛑 **Graceful stop**: the wrapped process gets a Ctrl+C (or your own stop command) and time to exit before being killed, on service stop and on machine shutdown  
- 💥 **Restart policies** with exponential backoff, plus the native SCM recovery actions  
- 🌱 **Environment variables**, service **dependencies**, **delayed start**, any **service account** (`NetworkService`, virtual accounts, gMSA...)  
- 📜 **Logs** of the wrapped process with time and size based rotation  
- 📝 Optional **TOML configuration file**, and `update` to change a service without reinstalling it  
- 🧼 **Clean install/uninstall** without needing `sc.exe`  
- 💼 Built with **pure Rust** 


## 🔧 Usage

### 🛠️ Install

```powershell
# using cargo
cargo install --git https://github.com/ferama/wsw
```

or download a prebuilt binary from github release page

### 🛠️ Install your executable as a Windows service:

```powershell
wsw.exe install --name myapp --exe C:\MyApp\app.exe -- --arg1 --arg2
```

This will:
- Install `wsw.exe` as a Windows service named `myapp`  
- Configure it to launch `app.exe` with the arguments `--arg1 --arg2`  
- Automatically start it  

Everything after `--` is passed to the executable as it is: no quoting rules
of `cmd.exe` to fight with. The older form, where the command line is run by
`cmd.exe /C`, is still supported and useful for shell features (pipes,
redirections, `.bat` files):

```powershell
wsw.exe install --name myapp --cmd "C:\MyApp\app.exe --arg1 --arg2"
```

Prefer `--exe` when possible: the wrapped process is then the direct child of
wsw, so the graceful stop (Ctrl+C) reaches it reliably and its real exit code
is reported.

### 🧹 Uninstall the service:

```powershell
wsw.exe uninstall --name myapp
```

Stops and removes the service cleanly.

### 🎛️ Manage services

```powershell
wsw.exe list                         # services managed by wsw
wsw.exe status  --name myapp         # state, PID, exit code
wsw.exe status  --name myapp -v      # ...and the whole effective configuration
wsw.exe start   --name myapp
wsw.exe stop    --name myapp
wsw.exe restart --name myapp
wsw.exe logs    --name myapp -f      # follow the output of the wrapped process
wsw.exe update  --name myapp --stop-timeout 30   # change options, then restart
```

### 🧪 Run manually (for testing):

You can also run it in the foreground without installing it as a service:

```powershell
wsw.exe run --exe C:\MyApp\app.exe -- --arg1 --arg2
```

The process is supervised exactly like the service does (restart policy,
logs...) until you press Ctrl+C — useful for debugging. `wsw run --name myapp`
runs an installed service with its own configuration.

## ⚙️ Options

All the options of `install` can be changed later with `update`. List options
(`--env`, `--depends-on`, `--grant-dir`) are repeatable; with `update` they
replace the whole list, and an empty value (`--depends-on ""`) clears it.

| Option | Description |
| --- | --- |
| `--name`, `-n` | Service name (default `wsw`) |
| `--display-name` | Display name (default `wsw-<name>`) |
| `--description` | Description shown by the Services console |
| `--exe <path> -- <args...>` | Executable started directly, with its arguments after `--` |
| `--cmd`, `-c` | Command line run through `cmd.exe /C` (alternative to `--exe`) |
| `--working-dir` | Working directory (default: directory of the executable) |
| `--env KEY=VALUE` | Environment variable; values can reference others: `PATH=C:\bin;%PATH%` |
| `--env-file <path>` | File of `KEY=VALUE` lines (`#` comments allowed), read at every start |
| `--depends-on <service>` | Service that must be running before this one starts |
| `--start-type` | `auto` (default), `delayed-auto`, `manual`, `disabled` |
| `--account-name` | Account to run as (default `LocalSystem`), see below |
| `--account-password` | Password, needed for regular user accounts only |
| `--grant-logon-right` | Grant "Log on as a service" to the account |
| `--grant-dir <path>` | Give the account Modify permissions on a directory |
| `--stop-timeout <secs>` | Time given to the process to exit after Ctrl+C / `--stop-cmd` (default 15) |
| `--stop-cmd <cmdline>` | Command run to stop the process, instead of sending Ctrl+C |
| `--restart` | `always` (default), `on-failure` (non-zero exit code), `never` |
| `--restart-delay <secs>` | Delay before a restart. Alone it is fixed; default: 1s doubling up to 60s |
| `--restart-max-delay <secs>` | Enables the exponential backoff, from `--restart-delay` up to this value |
| `--reset-after <secs>` | Stable run time after which delay and failure count start over (default 60) |
| `--max-restarts <n>` | Give up after n consecutive restarts, stopping the service with a failure (default unlimited) |
| `--scm-failure-actions` | Native SCM recovery actions, e.g. `restart:10,restart:60,none` |
| `--scm-failure-reset <secs>` | Time after which the SCM resets the failure count (default 1 day) |
| `--pre-start <cmdline>` | Command run before the process starts; if it fails the service does not start |
| `--post-stop <cmdline>` | Command run after the process stopped |
| `--hook-timeout <secs>` | Timeout of the hooks (default 30) |
| `--log-dir <path>` | Log directory (default `%ProgramData%\wsw\logs`) |
| `--log-rotation`, `-l` | `daily` (default), `hourly`, `minutely`, `never` |
| `--log-max-size <MB>` | Also rotate a log file when it grows over this size |
| `--max-log-files`, `-m` | Log files to keep (default 30) |
| `--log-split` | Write stderr to a separate `<name>.err.log` (`wsw logs --stderr`) |
| `--disable-logs`, `-d` | Do not capture the output of the process |
| `--config <file>` | Read the options from a TOML file |

Boolean options can be turned off with `update`, e.g. `--disable-logs=false`.

### 👤 Service accounts

`--account-name` accepts:

- built-in accounts, without password: `LocalSystem`, `LocalService`,
  `NetworkService` (also as `NT AUTHORITY\LocalService` and
  `NT AUTHORITY\NetworkService`);
- virtual accounts, without password: `NT SERVICE\<service name>`;
- group managed service accounts, without password: `DOMAIN\name$`;
- regular users, with `--account-password`: `.\user` for local users,
  `DOMAIN\user` for domain users. Add `--grant-logon-right` if the user does
  not have the "Log on as a service" right yet.

Accounts other than `LocalSystem` usually cannot write to the application
directory: `--grant-dir` gives them Modify permissions on a directory, with no
`icacls` needed. A custom `--log-dir` is created and made writable for the
account automatically.

### 🌱 Environment variables

Variables are applied in order (env file first, then `--env`), and `%VAR%`
references are expanded when the service starts, against the system
environment and the variables defined before: `PATH=C:\Ruby\bin;%PATH%`
extends the system `PATH`. They are stored unexpanded with the service
configuration (not in the SCM `Environment` registry value, which the SCM
does not expand). Only the variable names are written to the logs.

From `cmd.exe`, `%PATH%` on the command line is expanded by `cmd.exe` itself
before wsw sees it: escape it as `%^PATH%` outside quotes, or use PowerShell,
`--env-file` or `--config`, where `%` is not special.

### 🛑 Stop and shutdown

On `wsw stop`, `net stop`, a restart from the Services console or a machine
shutdown, wsw:

1. asks the process to exit: it sends Ctrl+C to the console of the process,
   or runs `--stop-cmd`;
2. waits for it up to `--stop-timeout`, telling the SCM the stop is
   progressing;
3. kills whatever is left of the process tree (the whole tree runs in a Job
   Object);
4. runs the `--post-stop` hook.

The preshutdown timeout of the service is set so that this also completes
during a machine shutdown.

Through `--cmd`, the Ctrl+C reaches `cmd.exe` and the program it runs. For
batch files `cmd.exe` may ask "Terminate batch job (Y/N)?": wsw does not
answer and kills the tree once the timeout expires. Use `--exe` (or
`--stop-cmd`) for a reliable graceful stop.

### 💥 Restart policies

The wrapped process is restarted by wsw according to `--restart`. When it is
not restarted (`never`, `on-failure` after a clean exit, or `--max-restarts`
exceeded) the service stops: with exit code 0 the SCM sees a clean stop,
otherwise a service specific error carrying the exit code of the process.
That failure triggers the SCM recovery actions configured with
`--scm-failure-actions`, e.g. to let Windows restart the whole service:

```powershell
wsw.exe install --name myapp --exe C:\MyApp\app.exe --restart never --scm-failure-actions restart:10,restart:60,none
```

### 📝 Configuration file

Every option can be written in a TOML file, with keys named like the flags.
Command line flags take precedence over the file:

```toml
# redmine.toml
name = "Redmine"
description = "Redmine project management"
exe = 'C:\Redmine\ruby\bin\ruby.exe'
args = ['C:\Redmine\ruby\bin\bundle', "exec", "puma", "-e", "production", "-b", "tcp://0.0.0.0:3000"]
working-dir = 'C:\Redmine\redmine'
env = ["RAILS_ENV=production", 'PATH=C:\Redmine\ruby\bin;%PATH%']
depends-on = ["RedminePostgreSQL"]
start-type = "delayed-auto"
account-name = 'NT AUTHORITY\NetworkService'
grant-dir = ['C:\Redmine\redmine']
restart = "on-failure"
restart-delay = 10
stop-timeout = 20
log-dir = 'C:\Redmine\logs'
log-max-size = 10
max-log-files = 8
```

```powershell
wsw.exe install --config redmine.toml
wsw.exe update  --config redmine.toml   # after editing it
```

## 🔁 WinSW → wsw

| WinSW XML | wsw |
| --- | --- |
| `<id>` | `--name` |
| `<name>` | `--display-name` |
| `<description>` | `--description` |
| `<executable>` | `--exe` (or `--cmd`) |
| `<arguments>` | arguments after `--` |
| `<workingdirectory>` | `--working-dir` |
| `<env name="K" value="V"/>` | `--env K=V`, `--env-file` |
| `<depend>` | `--depends-on` |
| `<startmode>` | `--start-type auto\|manual\|disabled` |
| `<delayedAutoStart>` | `--start-type delayed-auto` |
| `<serviceaccount>` `<username>` / `<password>` | `--account-name` / `--account-password` |
| `<serviceaccount>` `<allowservicelogon>` | `--grant-logon-right` |
| — | `--grant-dir` |
| `<stoptimeout>` | `--stop-timeout` |
| `<stopexecutable>` + `<stoparguments>` | `--stop-cmd` |
| `<onfailure action="restart" delay="10 sec"/>` | `--restart on-failure --restart-delay 10` (by wsw), or `--scm-failure-actions restart:10` (by the SCM) |
| `<onfailure action="none"/>` | `--restart never` |
| `<resetfailure>` | `--reset-after` (wsw), `--scm-failure-reset` (SCM) |
| `<prestart>` | `--pre-start` |
| `<poststop>` | `--post-stop` |
| `<logpath>` | `--log-dir` |
| `<log mode="roll-by-time">` `<pattern>` | `--log-rotation daily\|hourly\|minutely` |
| `<log mode="roll-by-size">` `<sizeThreshold>` `<keepFiles>` | `--log-max-size` + `--max-log-files` |
| `<log mode="roll-by-size-time">` | `--log-rotation` + `--log-max-size` |
| `<log mode="none">` | `--disable-logs` |
| `<log>` separate `.out.log` / `.err.log` | `--log-split` |
| the XML file | `--config service.toml` |
| `winsw install` / `uninstall` / `start` / `stop` / `restart` / `status` | same `wsw` commands |
| `winsw refresh` | `wsw update` |
| `winsw test` | `wsw run` |

Not supported: `<poststart>`, `<prestop>`, `<download>`, `<priority>`,
`<securityDescriptor>`, `<sharedDirectoryMapping>`, `<beeponshutdown>`,
`<interactive>`, extensions.

## 🔍 How it works

WSW installs itself as a service and monitors a child process (your actual app),
running in a Job Object together with all its children.
If the child process exits or crashes, WSW logs the event and restarts it according to the restart policy.

The service ImagePath only contains `wsw.exe run --name <name>`: the rest of
the configuration is stored as TOML under
`HKLM\SYSTEM\CurrentControlSet\Services\<name>\Parameters\Config`, and is removed
together with the service. Services installed by older wsw versions, whose
ImagePath carries the whole configuration (`run --cmd ...`), keep working
unchanged; `wsw update` migrates them.

This makes your app:
- Service-friendly  
- Resilient to crashes  
- Easy to deploy  

## 📦 Use case examples

- Running a Go, Rust, Net, any other runnable app as a service  
- Auto-starting a CLI tool with logging on boot  
- Running off-the-shelf tools like Python or Powershell scripts in the background  
- Easy service wrapping in CI setups or cloud VMs  

### 💎 Example: Redmine (Ruby + Puma)

Redmine running as `NetworkService`, started after its PostgreSQL service
(run from an elevated PowerShell, where `%PATH%` is not expanded):

```powershell
wsw install --name Redmine --exe C:\Redmine\ruby\bin\ruby.exe --working-dir C:\Redmine\redmine `
  --env RAILS_ENV=production --env "PATH=C:\Redmine\ruby\bin;%PATH%" `
  --depends-on RedminePostgreSQL --start-type delayed-auto `
  --account-name "NT AUTHORITY\NetworkService" --grant-dir C:\Redmine\redmine `
  --restart on-failure --restart-delay 10 --stop-timeout 20 `
  --log-dir C:\Redmine\logs --log-max-size 10 --max-log-files 8 `
  -- C:\Redmine\ruby\bin\bundle exec puma -e production -b tcp://0.0.0.0:3000
```

## 🧪 Testing

Unit tests of the platform independent parts (parsing, quoting, backoff, log
buffering and rotation...) run on any OS; the whole suite runs on Windows:

```powershell
cargo clippy --all-targets -- -D warnings
cargo test
```

The service behaviour needs a manual check, from an elevated PowerShell.

**1. Graceful stop.** Save this script as `C:\wsw-test\graceful.ps1`:

```powershell
$log = 'C:\wsw-test\graceful.log'
# A grandchild with a console of its own: Ctrl+C does not reach it, only
# the Job Object can terminate it
Start-Process ping.exe -ArgumentList '-t', '127.0.0.1' -WindowStyle Hidden
if ($env:IGNORE_CTRL_C) { [Console]::TreatControlCAsInput = $true }
try {
    while ($true) { Start-Sleep -Seconds 1 }
} finally {
    # Runs on Ctrl+C
    "$(Get-Date -Format o) graceful shutdown" | Out-File -Append $log
}
```

```powershell
wsw install --name wsw-test --stop-timeout 10 --exe powershell.exe -- -NoProfile -ExecutionPolicy Bypass -File C:\wsw-test\graceful.ps1
Measure-Command { wsw stop --name wsw-test }   # a few seconds at most
Get-Content C:\wsw-test\graceful.log           # ends with "graceful shutdown"
Get-Process ping -ErrorAction SilentlyContinue # nothing: the tree is gone
wsw logs --name wsw-test --full                # "Child process exited after ..."

# Now a process that ignores Ctrl+C: killed after --stop-timeout
wsw update --name wsw-test --env IGNORE_CTRL_C=1
wsw start --name wsw-test
Measure-Command { wsw stop --name wsw-test }   # about 10 seconds
Get-Process ping -ErrorAction SilentlyContinue # nothing: killed with the tree
wsw logs --name wsw-test --full                # "did not exit within 10s, killing the process tree"
wsw uninstall --name wsw-test
```

**2. Redmine.** Install it with the command of the example above, then check:

```powershell
sc.exe qc Redmine        # START_TYPE: AUTO_START (DELAYED), DEPENDENCIES: RedminePostgreSQL,
                         # SERVICE_START_NAME: NT AUTHORITY\NetworkService
sc.exe qfailure Redmine  # recovery actions, if --scm-failure-actions is used
wsw status --name Redmine -v
# The variables are in the environment of the process tree, e.g. with
# Process Explorer (Properties > Environment) on ruby.exe, or by adding
# a pre-start hook: --pre-start "set > C:\Redmine\logs\env.txt"
```

Reboot the machine: Redmine must start after PostgreSQL, a couple of minutes
after boot (delayed start), and `wsw logs --name Redmine --full` must show the
orderly stop of the previous shutdown ("Child process exited after ...").

## 📺 Prevent Windows Defender complaints

Windows Defender or other antivirus software might incorrectly flag `wsw` as a virus. This is because it uses low-level Windows APIs 
to install itself as a service, which can be interpreted as malicious behavior by some security tools.

To prevent this you can exlude the directory where `wsw` is installed from Windows Defender
using a command like this:

```powershell
Add-MpPreference -ExclusionPath "C:\Path\To\wsw"
```
Replace C:\Path\To\wsw with the actual installation path.

## 📄 License

MIT

## ❤️ Contribute

Got an idea or edge case? Open an issue or pull request — it's a tiny project, but it loves real-world use!
