# WSW - Windows Service Wrapper

> 💡 A tiny, practical tool that lets **any executable or script** run as a **real Windows service**, with zero boilerplate.

---

## 🚀 What is WSW?

Running background applications or daemons as Windows services should be easy — but in reality, **Windows makes it tricky**. 

If you've ever tried to:

- Wrap a custom binary as a service  
- Automatically restart a tool on failure  
- Avoid writing a Windows service in C++ or wrestling with the Windows API  
- Escape painful `sc.exe` syntax and quoting errors  

**Then WSW is for you.**

---

## ✅ Features

- 🧠 **Simple CLI interface**: one command installs and starts your service  
- ⚙️ **Wraps any executable** — even with arguments  
- 💥 **Automatic restart** if the wrapped process crashes  
- 🧼 **Clean install/uninstall** without needing `sc.exe`  
- 📜 Logs every restart attempt and failure  
- 💼 Built with **pure Go**, no unsafe code  

---

## 🔧 Usage

### 🛠️ Install

```powershell
go install github.com/ferama/wsw@latest
```

### 🛠️ Install your executable as a Windows service:

```powershell
wsw.exe -install-service -service-name myapp -cmd "C:\MyApp\app.exe --arg1 --arg2"
```

This will:
- Install `wsw.exe` as a Windows service named `wsw-myapp`  
- Configure it to launch `app.exe --arg1 --arg2`  
- Automatically start it  

---

### 🧹 Uninstall the service:

```powershell
wsw.exe -uninstall-service -service-name myapp
```

Stops and removes the service cleanly.

---

### 🧪 Run manually (for testing):

You can also run it directly without installing as a service:

```powershell
wsw.exe -cmd "C:\MyApp\app.exe --arg1 --arg2"
```

This is how the Windows Service Manager internally starts it — useful for debugging.

---

## 🔍 How it works

WSW installs itself as a service and monitors a child process (your actual app).  
If the child process exits or crashes, WSW logs the event and restarts it after a short delay.

This makes your app:
- Service-friendly  
- Resilient to crashes  
- Easy to deploy  

---

## 🛠️ Build it yourself

Requires Go 1.18+

```bash
go build -o wsw.exe .
```

---

## 📦 Use case examples

- Running a Go, Rust, Net, any other runnable app as a service  
- Auto-starting a CLI tool with logging on boot  
- Running off-the-shelf tools like Python or Powershell scripts in the background  
- Easy service wrapping in CI setups or cloud VMs  

---

## 📄 License

MIT

---

## ❤️ Contribute

Got an idea or edge case? Open an issue or pull request — it's a tiny project, but it loves real-world use!


