# WebDAV

An Ice Commander plugin that adds WebDAV servers as a connection kind. It claims
no file extension: a WebDAV filesystem is reached by connecting to a server and
no other way.

## What it registers

One connection kind, `webdav`, described by `src/webdav-fs/documents/webdav.json`.
The host draws that form; the plugin never touches a widget. The form collects a
connection name, the URL, a user and password, and a remote path to open at. A
hidden `port` field defaults to 443 and is written on commit. The connections
list and the drives list draw `src/webdav-fs/assets/webdav.svg`, which the
plugin registers at init as an asset owned by `ic-webdav-fs`.

Summary lines show the URL, falling back to `host`.

## What it does

One handle is one open connection. Calls may arrive on a worker thread, so
nothing in the filesystem calls back into the host. The wire side is blocking
HTTP through `reqwest` with rustls, with Basic authentication when a user is
set:

| the panel asks | the server is asked |
|---|---|
| list a folder | `PROPFIND`, `Depth: 1` |
| read a file | `GET` |
| write a file | `PUT` |
| new folder | `MKCOL` |
| delete | `DELETE`, where a 404 counts as done |
| rename | `MOVE` with `Destination` and `Overwrite: T` |

The mount reports itself writable. A failed call keeps its reason on the handle,
and `last_error` returns it.

The `PROPFIND` answer is read without an XML parser: the `D:` and `d:` prefixes
are stripped, then each `response` is searched for `href`, `getlastmodified`,
`getcontentlength` and `collection`. `href` is percent-decoded, and the folder
that was asked about is dropped from its own listing. `getlastmodified` is
parsed as RFC 2822, which covers the HTTP date the specification requires; any
other format reads as zero.

There is no shell, and the filesystem cannot be opened inside a file of another
one.

## Known limitations

- The mount reads only `url` (or `host` when `url` is missing), `user` and
  `pass`. `port` is stored but not used; the port comes from the URL.
  `remote_path` is applied by the host, not by the mount.
- No timeout is set, so `reqwest`'s blocking default of 30 seconds applies to
  each request including its body: a file that takes longer to download or
  upload fails.
- Files are read and written whole, in memory, with no progress reported.
- Every call builds a new HTTP client, so no connection is reused between calls.
- TLS trusts only the bundled webpki root certificates, not the system store,
  and there is no option to accept a self-signed certificate.
- Paths go into the URL as they are: names containing `#`, `?` or `%` are not
  escaped, and the `MOVE` `Destination` is not percent-encoded.
- Elements under a namespace prefix other than `D:`/`d:` are not recognised.
  Such a server's entries lose their size, date or folder flag, and the listing
  breaks (empty, or at most one entry) if `href` or `response` carry the prefix.
- The tests check opening, the operation table and host refusal; none of them
  talks to a server or exercises the `PROPFIND` parser.

## Building

```sh
./build.sh
./test.sh
./deploy-local.sh
```

`build.sh` builds in release for the host platform and copies the library into
`bin/`: `libic_webdav_fs.dylib` on macOS, `libic_webdav_fs.so` on Linux,
`ic_webdav_fs.dll` on Windows. `test.sh` runs `cargo test --workspace`.
`deploy-local.sh` copies the library into the folder the application loads
plugins from (`IC_PLUGIN_DIR` overrides it), then switch it on in
**Settings → Plugins** and restart.

The version the plugin reports comes from `package.json`. `npm run gen-version`
writes it into `version.rs`.

## Licence

The code is MIT or Apache-2.0, at your option. The icon in
`src/webdav-fs/assets/` is Icons8 artwork and is not covered by either licence;
see [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md). Contributions are taken
under the [DCO](DCO); sign off with `git commit -s`.
