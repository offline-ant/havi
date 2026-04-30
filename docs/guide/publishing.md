# Publishing Guide

This guide covers current publishing workflows for HAVI and HPPR.

## Choose a workflow

### 1) Fast path: publish one file from a running HAVI

Use `havi-cli publish` when HAVI is already running with DevTools enabled.

```bash
HAVI_DEVTOOLS=6000 ./havi/havi-cli publish //u/site/index.html ./index.html
```

What it does:

1. `hppr add --seal-by ring0` with inferred `Content-Type`
2. navigates HAVI to `hppr://u/site/index.html`

`havi-cli deploy` uses the same `hppr add --seal-by ring0` path for the
`//<group>/admin/deploy/<app>/|` content-pointer packet.

Use this for quick iteration on one file.

### 2) Script path: publish with `hppr add`

`add` is the normal publish command. You send headers/data; repo builds packets.

```bash
# authenticate request (example ring1 token)
export HPPR_SIGNER='ring1:ring0|init'

# publish content
hppr add --seal-by ring0 \
  -H 'Content-Type: text/html; charset=utf-8' \
  //u/site/index.html < ./index.html
```

Use this for CI scripts and repeatable deploy steps.

### 3) Route app content pointer (routed apps)

For routed origins (`hppr://<group>/<app>/...`), set app content pointer
metadata on the upstream repo:

```bash
./havi/havi-cli deploy <group> <app> //<content-root> <content-authority>
```

This writes:

`//<group>/admin/deploy/<app>/|`

Headers:

- `Content-Root: //<...>`
- `Content-Authority: V.<...>.H3`

At runtime HAVI resolves routed GET/LIST through this app content pointer.

### 4) Exact-bytes path: `mkpac` + `store`

Use this when you must control packet bytes exactly (offline build pipelines,
reproducible artifacts, prebuilt packet bundles).

```bash
hppr mkpac seal -k "$SIGNING_KEY" //u/site/index.html < ./index.html | hppr store
```

- `mkpac` builds the packet locally.
- `store` sends packet bytes unchanged.

### 5) Large file path: `hppr chunk`

For content above blob limits, use chunk manifests.

```bash
hppr chunk ./video.mp4 //u/media/video.mp4 --seal-by "$SIGNING_KEY"
```

Clients that support chunk manifests read it transparently.

## `add` vs `store`

- `add`: repo constructs packet layers from your headers + data.
- `store`: repo stores full packet bytes you already built.

Default to `add`. Use `store` for prebuilt packets.

## Directory publishing (current approach)

For whole-site import/export, use filesystem mount + copy.

### Linux FUSE

```bash
hppr-fuse --home "$HPPR_HOME" --signer "$HPPR_SIGNER" \
  --root //u/site --mount /mnt/hppr --rw --seal-with ring0 &
cp -a ./site/. /mnt/hppr/
fusermount3 -u /mnt/hppr
```

### Portable NFS

```bash
hppr-nfs --home "$HPPR_HOME" --signer "$HPPR_SIGNER" \
  --root //u/site --rw --seal-with ring0 --bind 127.0.0.1:3049
```

Then mount and copy using your OS NFS client:

```bash
mount -t nfs -o port=3049,mountport=3049,nfsvers=3,tcp,nolock 127.0.0.1:/ /mnt/hppr
cp -a ./site/. /mnt/hppr/
umount /mnt/hppr
```

These workflows target the repo named by `HPPR_HOME`; they do not write into
HAVI's browser-local `havi-packets.sqlite` store. `havi-cli publish-dir` is
retired and exits with an explicit error.

## Keys and identities

Two independent choices exist for each write:

1. **Request identity** (`HPPR_SIGNER` / `--id`): who is allowed to write.
2. **Content seal identity** (`--seal-by`): which key signs stored content.

Common key commands:

```bash
hppr key generate site-admin
hppr key list
hppr key show site-admin
hppr key pubkey site-admin
```

## Verify after publish

```bash
hppr headers //u/site/index.html
hppr data //u/site/index.html > /tmp/index.out
hppr tips //u/site/index.html
```

For live updates:

```bash
hppr watch //u/site/
```

## Next

- Access strategy: [Access Patterns](access-patterns.md)
- Tool syntax: `../reference/havi-cli.md`,
  `../../../hppr/rust/tools/cli/README.md`
