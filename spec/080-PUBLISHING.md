# Publishing Content

Publishing means creating signed Seal packets so consumers can verify source
identity.

## Model

Signatures are verified independently of transport endpoint.

## Key management

### Generate a keypair

```bash
hppr key generate > my-signing-key.txt
```

Output includes:

- secret key: `&.<...>.H3`
- verification key: `V.<...>.H3`

Keep secret keys private.

### Store key for HAVI workflows

Store signing keys in a Ring1 keys packet so `🖧ADD` can sign.

Ring1 keys path: `//repo/admin/ring1/<name>/keys/|/seal/<vkey>`
(HPPR `050-RING1.md`).

Example API flow from a privileged page:

```javascript
await window.ring0.add({
  headers: [
    'Group: repo',
    'App: admin',
    'Location: ring1/my-publisher/keys',
    'Seal-By: oldest',
    'Secret-Key: &.your~secret~key...H3'
  ],
  data: ''
});
```

## ADD vs STORE

- `ADD`: send headers/data; repo constructs packet structure
- `STORE`: send full packet bytes unchanged

Use `ADD` for normal publishing.
Use `STORE` for prebuilt packet pipelines.

## Create signed content

### CLI example

```bash
hppr add --group mygroup --app myapp --location docs/readme \
  --seal-by oldest < readme.md
```

### JS example

```javascript
await window.ring0.add({
  headers: [
    'Group: mygroup',
    'App: myapp',
    'Location: docs/readme',
    'Seal-By: oldest'
  ],
  data: '# My README\n\nContent here...'
});
```

`Seal-By` values:

- `oldest`
- `latest`
- explicit verification key

## Update content

Publish a new version at the same coordinate with fresh `TAI`.
Top-coordinate fetch returns the latest version. Older versions stay
addressable.

Optional linkage:

- `+Link: supersedes <old-hash>`

## Multi-author patterns

### Shared trust

Site-trust can include multiple publisher keys with repeated `Member` headers.

### Editorial flow

1. contributor signs draft in a staging location
2. editor reviews
3. editor republishes to final location with the production key

## Key rotation

1. generate a new keypair
2. add the new key to keys config
3. overlap trust during transition
4. sign new content with the new key
5. remove the old key after migration

Publish a rotation notice so consumers update trust config.

## Distribution

### Direct repo

Run `hpprd` and share route endpoint details.

### Mirrors

Mirrors can serve copied packets unchanged.
Consumers verify signer keys, not mirror identity.

### Export and import

```bash
hppr get //mygroup/myapp/docs/ --recursive > docs.hppr
cat docs.hppr | hppr store
```

Signatures remain valid after transfer.
