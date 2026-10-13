# Prepare Linux live testing

[Clausewitz MCP](https://github.com/smerss/Clausewitz-MCP), by smerss, reads a running native Linux Hearts of Iron IV process. The pinned upstream revision is `97c85437081904d5ad07bae1623ec58d1c0a5b9e`. It supports the native **x86-64 Linux** HOI4 **1.19.2** build only, with the matching Linux executable layout. A different version, patch, distribution build, or executable can invalidate its offsets. A successful doctor command does not establish build compatibility.

Linux game installation and server preparation are extra manual steps. You may leave **I confirm a compatible Linux game and Clausewitz MCP server are ready** unchecked in setup. The app records a to-do and does not register this server until you confirm readiness in a reviewed setup or repair plan.

## Linux: use the native game and server

Use the native x86-64 Linux HOI4 1.19.2 build. No WSL, virtual machine, SSH host, or extra remote environment is needed on Linux. Prepare the pinned server below at `~/tools/Clausewitz-MCP` and verify its connection to the compatible game as your ordinary user. In **Live testing**, leave Clausewitz MCP selected and confirm the game and server are ready. Setup registers the virtual-environment Python directly with an absolute server path.

## Windows: prepare WSL

1. Follow Microsoft's [WSL instructions](https://learn.microsoft.com/windows/wsl/install). In an administrator PowerShell where those instructions require it, run `wsl --install -d Ubuntu`. Restart when prompted. Run `wsl --update`, then `wsl --list --verbose` to check the installed distribution and WSL 2 version. Complete the distribution's first-run user setup yourself.
2. Open that distribution. Prepare Linux graphical application support according to Microsoft's [WSLg guidance](https://learn.microsoft.com/windows/wsl/tutorials/gui-apps). Install Steam **inside Linux** using the official [Install Steam page](https://store.steampowered.com/about/) or your distribution's software manager. GPU and game support depend on your computer; WSL installation alone does not prove HOI4 can run.
3. Sign in to Steam yourself. In **Library**, open **Hearts of Iron IV ? Properties ? Compatibility** and leave **Force the use of a specific Steam Play compatibility tool** off. Install the Linux game. In **Properties ? Betas**, choose a compatible 1.19.2 branch if your account offers it, wait for the download, then start the game and check its version. If that build is unavailable, leave this workflow deferred.
4. Make the mod accessible to the Linux game, configure its Linux launcher descriptor, and confirm it loads. The Windows game process is not compatible with this server.
5. Prepare the server inside Linux using the commands below. Run the game and server as the same ordinary Linux user.
6. In **Live testing**, choose Clausewitz MCP. Optionally enter the exact safe WSL distribution name; leave it empty to use the default distribution. Confirm readiness only after the compatible Linux game and server work.

## macOS: prepare Linux and SSH

1. Prepare a Linux machine or virtual machine that can run the native Linux game. Supply suitable graphics support and a Linux host/VM capable of running the x86-64 game binary. An Arm Linux VM on Apple silicon alone does not provide x86-64 HOI4 compatibility. The macOS game executable is not compatible with Clausewitz MCP.
2. Install Steam on Linux, sign in yourself, and install the compatible native HOI4 1.19.2 build. Confirm your mod loads there.
3. Prepare the server on that Linux machine using the fixed path below, under the same ordinary Linux user as the game.
4. Configure SSH access outside the app. Create a host alias in your own SSH configuration and arrange your own authentication. For example, add the following to `~/.ssh/config`, replacing the example host and Linux user with your own values. The app asks only for the alias; never enter a password, private key, or credential into setup.

   ```sshconfig
   Host linux-game
       HostName linux-pc.local
       User linux-user
   ```
5. From macOS, verify `ssh -o BatchMode=yes -T linux-game true`, replacing `linux-game` with your alias. Accept and verify the host key yourself before using batch mode. Resolve authentication or connectivity failures outside the app.
6. Enter that safe alias in **SSH host alias** and confirm readiness after verifying the compatible game and server. A ready macOS route requires an alias.

## Install the pinned server on Linux

Install Git, Python 3 with virtual environment support, and your distribution's prerequisites. In your Linux terminal:

```sh
mkdir -p ~/tools
git clone https://github.com/smerss/Clausewitz-MCP.git ~/tools/Clausewitz-MCP
cd ~/tools/Clausewitz-MCP
git checkout --detach 97c85437081904d5ad07bae1623ec58d1c0a5b9e
python3 -m venv .venv
.venv/bin/python -m pip install -r requirements.txt
```

The fixed installation location and `.venv` are used by the app's route. If the folder already exists, inspect it and preserve any local changes before checking out the pinned revision. Upstream declares NumPy 1.20 or newer in its requirements.

Linux process inspection requires ptrace permission. Check your Linux security policy and upstream guidance. Restrictive Yama settings can prevent a server from attaching to an independently launched game even for the same user. Decide with the Linux administrator how to permit the required access; setup does not automatically change sysctl. Run the server as your ordinary user, never as root.

With the compatible game running, test the server from Linux:

```sh
~/tools/Clausewitz-MCP/.venv/bin/python ~/tools/Clausewitz-MCP/clausewitz.py serve
```

This is an MCP stdio server, so it may wait for client input. Starting without an immediate error is not proof that live game reads work. Verify the client connection and a bounded read against the compatible game before treating gameplay testing as available.

After readiness is confirmed, the app registers a reviewed native Linux, WSL or SSH route. Restart the selected coding client after applying the plan. **Configured** describes the registered route; it does not claim live gameplay was verified. The alternative HOI4 Live Tools provider is experimental, slower, and not fully tested. Its current public package verifies Windows only, and its macOS scaffold remains incomplete.
