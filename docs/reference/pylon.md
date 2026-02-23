# pylon Reference

Service manager for the HPPR ecosystem. Manages hpprd and satellite services
(lokid, unlokid, hppr-fs) as child processes. TCP JSON lines control protocol
on localhost.

## Synopsis

```bash
pylon                              # start daemon
pylon [COMMAND]                    # send command to running daemon
```

## Daemon Mode

```bash
pylon
pylon --port <port>
```

Starts the pylon daemon. Binds a TCP control port (default: 4850) and
auto-starts hpprd. Writes port to `~/.config/pylon/pylon.port`.

Prints `PYLON_BIND=127.0.0.1:<port>` on startup.

Auto-shuts down after 30 seconds with zero connected clients.

## Global Commands

```bash
pylon status                       # service status + NFS mounts
pylon mounts                       # list active NFS mounts
pylon shutdown                     # stop all services and exit
```

## Service Commands

```bash
pylon hpprd start [--k v]          # start hpprd
pylon hpprd stop                   # stop hpprd
pylon lokid start [--k v]          # start lokid
pylon lokid stop                   # stop lokid
pylon unlokid start [--k v]        # start unlokid
pylon unlokid stop                 # stop unlokid
```

### hpprd Options

- `--repo_path <path>`: repository directory
- `--bind <host:port>`: bind address
- `--port <port>`: shorthand for `--bind 127.0.0.1:<port>`
- `--phc <params>`: Argon2id PHC string (set via `HPPR_PHC` env)

### lokid Options

- `--key <signing-key>`: HSB3 signing key
- `--port <port>`: listen port

### unlokid Options

- `--port <port>`: listen port
- `--shim <bool>`: enable shim mode

## NFS Commands

```bash
pylon nfs start [--k v]            # start hppr-fs server
pylon nfs stop                     # stop hppr-fs server
pylon nfs mount [<mountpoint>]     # start hppr-fs + OS mount
pylon nfs unmount [<mountpoint>]   # OS unmount
```

Default mountpoint: `/mnt/hppr`.

### hppr-fs / mount Options

- `--repo <addr>`: remote repository address
- `--root <coordinate>`: root coordinate (default: `//'`)
- `--port <port>`: NFS listen port
- `--bind <addr>`: bind address (default: `127.0.0.1`)
- `--signer <signer>`: authentication identity
- `--rw`: enable write support

`pylon nfs mount` starts hppr-fs if not already running, waits for the NFS
port, then executes the platform mount command.

## Control Protocol

TCP JSON lines on localhost. Default port 4850.

Request:

```json
{"id": 1, "cmd": "status"}
{"id": 2, "cmd": "start", "service": "hpprd", "args": {"bind": "127.0.0.1:4777"}}
{"id": 3, "cmd": "mount", "args": {"mountpoint": "/mnt/hppr"}}
```

Response:

```json
{"id": 1, "ok": true, "data": {...}}
{"id": 2, "ok": true}
{"id": 3, "ok": false, "error": "mount failed: ..."}
```

Event broadcast (unsolicited):

```json
{"event": "service_started", "service": "hpprd", "pid": 1234, "port": 4777}
{"event": "service_stopped", "service": "hpprd", "pid": 1234}
```

## Port File

`~/.config/pylon/pylon.port` (or `$XDG_CONFIG_HOME/pylon/pylon.port`)

Contains the TCP port number as plain text. Removed on clean shutdown.

## HAVI Integration

HAVI connects to pylon on startup (spawning it if needed). The TCP connection
is held open for the app lifetime to prevent idle shutdown. Startup sequence:

1. Check `HAVI_REPO` env → use external endpoint directly
2. Otherwise → connect to or spawn pylon → start hpprd → use hpprd port

## Examples

```bash
# Start daemon (auto-starts hpprd)
pylon

# Check status
pylon status

# Mount repository as NFS filesystem
pylon nfs mount /mnt/hppr --root //u/

# Unmount
pylon nfs unmount /mnt/hppr

# Start hpprd on custom port
pylon hpprd start --bind 127.0.0.1:4777

# Stop everything
pylon shutdown
```
