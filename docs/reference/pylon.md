# pylon Reference

Service manager for the HPPR ecosystem. Manages hpprd and satellite services
(lokid, unlokid, hppr-nfs) as child processes. TCP JSON lines control protocol
on localhost.

Each pylon instance is bound to one hpprd repository directory. The PID file
lives in the repo directory, enforcing one pylon per repo.

## Synopsis

```bash
pylon                              # start daemon (repo: ./repo)
pylon --path <path>                # start daemon for specific repo
pylon [COMMAND]                    # send command to running daemon
```

## Daemon Mode

```bash
pylon
pylon --path <path>
pylon --bind [host:]port
pylon --path <path> --bind [host:]port
```

Starts the pylon daemon. Scans for a free TCP control port starting at 4850
up to 4900, falling back to an OS-assigned random port if the range is
exhausted. Auto-starts hpprd on its default port (4777). Writes
`<pid> <port>` to `<repo>/pylon.pid`. Refuses to start if another pylon is
already running for the same repo.

Prints `PYLON_BIND=127.0.0.1:<port>` on startup.

Auto-shuts down after 30 seconds with zero connected clients.

### Repo Path Resolution

1. `--path <path>` CLI argument
2. Default: `./repo`

## Global Commands

```bash
pylon status                       # service status + NFS mounts + user
pylon mounts                       # list active NFS mounts
pylon shutdown                     # stop all services and exit
```

The `status` response includes a `"user"` field with the OS user running pylon.
When hpprd is running, `status.hpprd.listeners` lists active listener IDs
(e.g. `tcp:127.0.0.1:4777`, `ws:127.0.0.1:4778`, `unix:/abs/path/hppr.sock`).

## Service Commands

```bash
pylon hpprd start [--k v]          # start hpprd
pylon hpprd stop                   # stop hpprd
pylon hpprd listen --bind <spec>   # add hpprd listener at runtime
pylon hpprd unlisten --bind <id|spec>   # remove hpprd listener at runtime
pylon lokid start [--k v]          # start lokid
pylon lokid stop                   # stop lokid
pylon unlokid start [--k v]        # start unlokid
pylon unlokid stop                 # stop unlokid
```

### hpprd Options

- `--repo_path <path>`: repository directory (injected by pylon automatically)
- `--bind <spec>`: bind spec (`host:port`, `ws+host:port`, `quib+host:port`, `udp+host:port`, `unix+/path`, `all+host:port`)
- `--port <port>`: shorthand for `--bind 127.0.0.1:<port>`
- default (no bind/port): `127.0.0.1:4777`
- `--phc <params>`: Argon2id PHC string (set via `HPPR_PHC` env)

### lokid Options

- `--key <signing-key>`: HSB3 signing key

### unlokid Options

- `--shim <bool>`: enable shim mode

## NFS Commands

```bash
pylon nfs start [--k v]            # start hppr-nfs server
pylon nfs stop                     # stop hppr-nfs server
pylon nfs mount [<mountpoint>]     # start hppr-nfs + OS mount
pylon nfs unmount [<mountpoint>]   # OS unmount
```

Default mountpoint: `/mnt/hppr`.

### hppr-nfs / mount Options

- `--home <addr>`: remote repository address
- `--root <coordinate>`: root coordinate (default: `//'`)
- `--bind [host:]port`: listen address (default: `127.0.0.1:2049`)
- `--signer <signer>`: authentication identity
- `--rw`: enable write support

`pylon nfs mount` starts hppr-nfs if not already running, waits for the NFS
port, then executes the platform mount command.

## Control Protocol

TCP JSON lines on localhost. Default port 4850.

Request:

```json
{"id": 1, "cmd": "status"}
{"id": 2, "cmd": "start", "service": "hpprd", "args": {"bind": "127.0.0.1:4777"}}
{"id": 3, "cmd": "listen", "args": {"bind": "ws+127.0.0.1:4778"}}
{"id": 4, "cmd": "unlisten", "args": {"bind": "ws:127.0.0.1:4778"}}
{"id": 5, "cmd": "mount", "args": {"mountpoint": "/mnt/hppr"}}
```

Response:

```json
{"id": 1, "ok": true, "data": {"user": "alice", "hpprd": {"state": "running", ...}, ...}}
{"id": 2, "ok": true}
{"id": 3, "ok": true, "data": {"listeners": ["ws:127.0.0.1:4778"]}}
{"id": 4, "ok": true, "data": {"listeners": ["ws:127.0.0.1:4778"]}}
{"id": 5, "ok": false, "error": "mount failed: ..."}
```

Event broadcast (unsolicited):

```json
{"event": "service_started", "service": "hpprd", "pid": 1234, "port": 4777}
{"event": "service_stopped", "service": "hpprd", "pid": 1234}
{"event": "command", "cmd": "start", "service": "hpprd"}
```

Events can arrive before the response to a request on the same connection.
Clients must ignore event lines and wait for the matching response `id`.

Command events are emitted for start, stop, mount, unmount, listen, unlisten,
and shutdown.

`listen`/`unlisten` are strict ACKed operations: pylon writes a JSON control
line to hpprd stdin and waits for hpprd stdout markers:

- success: `HPPRD_LISTEN=<listener-id>` or `HPPRD_UNLISTEN=<listener-id>`
- failure: `HPPRD_ERROR=<message>`

If hpprd does not ACK within the control timeout, pylon returns an error.

## PID File

`<repo>/pylon.pid`

Contains `<pid> <port>` on one line. Written on startup, removed on clean
shutdown. Used by clients to discover the pylon port for a given repo.

If the PID file exists and the process is alive, pylon refuses to start.

## HAVI Integration

HAVI connects to pylon on startup (spawning it if needed). A background reader
thread holds the TCP connection open and receives service events. Startup
sequence:

1. Check `HAVI_HOME` env → use external endpoint directly
2. Otherwise → find or spawn pylon for `~/.config/HAVI/repo/`
3. Subscribe to event stream → update toolbar status on service changes

## Examples

```bash
# Start daemon for default repo (auto-starts hpprd)
pylon

# Start daemon for specific repo
pylon --path /data/myrepo

# Check status
pylon status

# Mount repository as NFS filesystem
pylon nfs mount /mnt/hppr --root //u/

# Unmount
pylon nfs unmount /mnt/hppr

# Start hpprd on custom port
pylon hpprd start --bind 127.0.0.1:4777

# Add/remove listeners at runtime
pylon hpprd listen --bind ws+127.0.0.1:4778
pylon hpprd unlisten --bind ws:127.0.0.1:4778

# Stop everything
pylon shutdown
```
