# WSW - Windows Service Wrapper

> 💡 A tiny, practical tool that lets **any executable or script** run as a **real Windows service**, with zero boilerplate.


## 🚀 What is WSW?

Running background applications or daemons as Windows services should be easy — but in reality, **Windows makes it tricky**. 

If you've ever tried to:

- Wrap a custom binary as a service  
- Automatically restart a tool on failure  
- Avoid writing a Windows service in C++ or wrestling with the Windows API  
- Escape painful `sc.exe` syntax and quoting errors  

**Then WSW is for you.**


## ✅ Features

- 🧠 **Simple CLI interface**: one command installs and starts your service  
- ⚙️ **Wraps any executable** — even with arguments  
- 💥 **Automatic restart** if the wrapped process crashes  
- 🧼 **Clean install/uninstall** without needing `sc.exe`  
- 📜 Logs every restart attempt and failure  
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
wsw.exe install --name myapp --cmd "C:\MyApp\app.exe --arg1 --arg2"
```

This will:
- Install `wsw.exe` as a Windows service named `myapp`  
- Configure it to launch `app.exe --arg1 --arg2`  
- Automatically start it  


### 🧹 Uninstall the service:

```powershell
wsw.exe uninstall --name myapp
```

Stops and removes the service cleanly.

### 🧪 Run manually (for testing):

You can also run it directly without installing as a service:

```powershell
wsw.exe run --cmd "C:\MyApp\app.exe --arg1 --arg2"
```

This is how the Windows Service Manager internally starts it — useful for debugging.

## 🔍 How it works

WSW installs itself as a service and monitors a child process (your actual app).  
If the child process exits or crashes, WSW logs the event and restarts it after a short delay.

This makes your app:
- Service-friendly  
- Resilient to crashes  
- Easy to deploy  

## 📦 Use case examples

- Running a Go, Rust, Net, any other runnable app as a service  
- Auto-starting a CLI tool with logging on boot  
- Running off-the-shelf tools like Python or Powershell scripts in the background  
- Easy service wrapping in CI setups or cloud VMs  

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


