# Publishing Guide

This guide covers current publishing workflows for HAVI and HPPR.

## Choose a workflow

### 1) Fast path: publish one file from a running HAVI

Use `havi-cli publish` when HAVI is already running with DevTools enabled.

```bash
HAVI_DEVTOOLS=6000 ./havi/havi-cli publish //u/site/index.html ./index.html
```

What it does:

1. `hppr add --seal-by oldest` with inferred `Content-Type`
2. navigates HAVI to `hppr://u/site/index.html`

Use this for quick iteration on one file.

### 2) Script path: publish with `hppr add`

`add` is the normal publish command. You send headers/data; repo builds packets.

```bash
# authenticate request (example ring1 token)
export HPPR_SIGNER='!ring0/init'

# publish content
hppr add --seal-by oldest \
  -H 'Content-Type: text/html; charset=utf-8' \
  //u/site/index.html < ./index.html
```

Use this for CI scripts and repeatable deploy steps.

### 3) Route deployment pointer (routed apps)

For routed origins (`hppr://<group>/<app>/...`), set deployment metadata on the
upstream repo:

```bash
./havi/havi-cli deploy <group> <app> //<deploy-root> <deploy-signer>
```

This writes:

`//<group>/admin/deploy/<app>/|`

Headers:

- `Deploy-Root: //<...>`
- `Deploy-Signer: V.<...>.H3`

At runtime HAVI resolves routed GET/LIST through this deployment pointer.

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

### Via pylon (recommended)

```bash
# mount repo subtree with write enabled
pylon mount /mnt/hppr --root //u/site --rw --seal_with oldest

# copy directory contents into mounted tree
cp -a ./site/. /mnt/hppr/

# unmount when done
pylon unmount /mnt/hppr
```

This replaces old `dir-pac`/`pac-dir` workflows.

`havi-cli publish-dir` currently shells out to `dir-pac`. If your environment
does not provide `dir-pac`, use mount+copy.

### Direct `hppr-nfs`

```bash
hppr-nfs --home "$HPPR_HOME" --signer "$HPPR_SIGNER" \
  --root //u/site --rw --seal-with oldest --bind 127.0.0.1:3049
```

Then mount and copy using your OS NFS client.

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
- Tool syntax: `../reference/havi-cli.md`, `../../../hppr/rust/tools/cli/README.md`
