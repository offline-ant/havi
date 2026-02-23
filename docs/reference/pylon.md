# pylon Reference

Service manager for the HPPR ecosystem. Manages hpprd and satellite services
(lokid, unlokid, hppr-nfs, hppr-fuse) as child processes. TCP JSON lines
control protocol on localhost.

Pylon operates in two modes:

- **Local mode** (default): owns and manages hpprd as a child process.
  PID file lives in the repo directory, enforcing one pylon per repo.
- **Remote mode** (`--home <via>`): connects to an external hpprd. Does not
  manage hpprd lifecycle. Satellites run against the external endpoint.

## Synopsis

```bash
pylon                              # start daemon, local mode (repo: ./repo)
pylon --path <path>                # start daemon for specific repo
pylon --home <via>                 # start daemon, remote mode
pylon [COMMAND]                    # send command to running daemon
```

## Daemon Mode

```bash
pylon
pylon --path <path>
pylon --home <via>
pylon --bind [host:]port
pylon --path <path> --home <via> --bind [host:]port
```

Starts the pylon daemon. Scans for a free TCP control port starting at 4850
up to 4900, falling back to an OS-assigned random port if the range is
exhausted. Writes `<pid> <port>` to `<repo>/pylon.pid`. Refuses to start if
another pylon is already running for the same repo.

Prints `PYLON_BIND=127.0.0.1:<port>` on startup.

Auto-shuts down after 30 seconds with zero connected clients.

### Local Mode

Default. Pylon auto-starts hpprd on its default port (4777) and manages its
full lifecycle: start, stop, listen, unlisten.

### Remote Mode

Activated by `--home <via>`. Pylon connects satellites to the external hpprd
at `<via>`. The hpprd service is not managed — `pylon hpprd start`,
`pylon hpprd stop`, `pylon hpprd listen`, and `pylon hpprd unlisten` are
unavailable. Status reports the external endpoint instead of a managed process.

### Repo Path Resolution

1. `--path <path>` CLI argument
2. Default: `./repo`

## Global Commands

```bash
pylon status                       # service status + mounts + user
pylon mounts                       # list active mounts
pylon shutdown                     # stop all services and exit
```

The `status` response includes:

- `"user"`: OS user running pylon
- `"mode"`: `"local"` or `"remote"`
- `"mounts"`: active mounts, each with `"device"`, `"mountpoint"`, `"fstype"`
  (`"nfs"` or `"fuse"`)

In local mode, `status.hpprd.listeners` lists active listener IDs
(e.g. `tcp:127.0.0.1:4777`, `ws:127.0.0.1:4778`, `unix:/abs/path/hppr.sock`).

In remote mode, `status.hpprd` reports the external endpoint.

## Service Commands

```bash
pylon hpprd start [--k v]          # start hpprd (local mode only)
pylon hpprd stop                   # stop hpprd (local mode only)
pylon hpprd listen --bind <spec>   # add hpprd listener (local mode only)
pylon hpprd unlisten --bind <id|spec>   # remove hpprd listener (local mode only)
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

## Mount Commands

```bash
pylon mount [<mountpoint>] [--k v]   # mount filesystem (auto-selects backend)
pylon unmount [<mountpoint>]         # unmount filesystem
```

Pylon selects the backend automatically: FUSE on Linux, NFS on macOS/Windows.

Default mountpoint: `/mnt/hppr`.

### Mount Options

- `--home <addr>`: remote repository address
- `--root <coordinate>`: root coordinate (default: `//`)
- `--signer <signer>`: authentication identity (default: `anyone`)
- `--rw`: enable write support
- `--seal_with <mode>`: seal mode for writes

## NFS Commands

```bash
pylon nfs start [--k v]            # start hppr-nfs server
pylon nfs stop                     # stop hppr-nfs server
```

### hppr-nfs Options

- `--home <addr>`: remote repository address
- `--root <coordinate>`: root coordinate (default: `//`)
- `--bind [host:]port`: listen address (default: `127.0.0.1:2049`)
- `--signer <signer>`: authentication identity
- `--rw`: enable write support

## Control Protocol

TCP JSON lines on localhost. Default port 4850.

Request:

```json
{"id": 1, "cmd": "status"}
{"id": 2, "cmd": "start", "service": "hpprd", "args": {"bind": "127.0.0.1:4777"}}
{"id": 3, "cmd": "listen", "args": {"bind": "ws+127.0.0.1:4778"}}
{"id": 4, "cmd": "unlisten", "args": {"bind": "ws:127.0.0.1:4778"}}
{"id": 5, "cmd": "mount", "args": {"mountpoint": "/mnt/hppr"}}
{"id": 6, "cmd": "unmount", "args": {"mountpoint": "/mnt/hppr"}}
```

Response:

```json
{"id": 1, "ok": true, "data": {"user": "alice", "mode": "local", "hpprd": {"state": "running", ...}, ...}}
{"id": 2, "ok": true}
{"id": 3, "ok": true, "data": {"listeners": ["ws:127.0.0.1:4778"]}}
{"id": 4, "ok": true, "data": {"listeners": ["ws:127.0.0.1:4778"]}}
{"id": 5, "ok": true, "data": {"mountpoint": "/mnt/hppr"}}
{"id": 6, "ok": true}
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

`listen`/`unlisten` are strict ACKed operations (local mode only): pylon
writes a JSON control line to hpprd stdin and waits for hpprd stdout markers:

- success: `HPPRD_LISTEN=<listener-id>` or `HPPRD_UNLISTEN=<listener-id>`
- failure: `HPPRD_ERROR=<message>`

If hpprd does not ACK within the control timeout, pylon returns an error.

## PID File

`<repo>/pylon.pid`

Contains `<pid> <port>` on one line. Written on startup, removed on clean
shutdown. Used by clients to discover the pylon port for a given repo.

If the PID file exists and the process is alive, pylon refuses to start.

## HAVI Integration

HAVI always connects through pylon. Startup sequence:

1. Find or spawn pylon for `<config-dir>/repo/`
2. If `HAVI_HOME` is set, pylon starts in remote mode (`--home <via>`)
3. Subscribe to event stream → update toolbar status on service changes
4. Background reader thread holds the TCP connection open

## Examples

```bash
# Start daemon, local mode (auto-starts hpprd)
pylon

# Start daemon, remote mode
pylon --home tcp+10.0.0.5:4777

# Start daemon for specific repo
pylon --path /data/myrepo

# Check status
pylon status

# Mount filesystem (auto-selects FUSE on Linux, NFS elsewhere)
pylon mount /mnt/hppr --root //u/

# Unmount
pylon unmount /mnt/hppr

# Start hpprd on custom port (local mode)
pylon hpprd start --bind 127.0.0.1:4777

# Add/remove listeners at runtime (local mode)
pylon hpprd listen --bind ws+127.0.0.1:4778
pylon hpprd unlisten --bind ws:127.0.0.1:4778

# Stop everything
pylon shutdown
```
