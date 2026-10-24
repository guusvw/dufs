# Dufs

[![CI](https://github.com/sigoden/dufs/actions/workflows/ci.yaml/badge.svg)](https://github.com/sigoden/dufs/actions/workflows/ci.yaml)
[![Crates](https://img.shields.io/crates/v/dufs.svg)](https://crates.io/crates/dufs)
[![Docker Pulls](https://img.shields.io/docker/pulls/sigoden/dufs)](https://hub.docker.com/r/sigoden/dufs)

Dufs is a distinctive utility file server that supports static serving, uploading, searching, accessing control, webdav...

![demo](https://user-images.githubusercontent.com/4012553/220513063-ff0f186b-ac54-4682-9af4-47a9781dee0d.png)

## Features

- Serve static files
- Download folder as zip file
- Upload files and folders (Drag & Drop)
- Create/Edit/Search files
- Resumable/partial uploads/downloads
- Access control
- Support https
- Support webdav
- Easy to use with curl

## Install

### With cargo

```
cargo install dufs
```

### With docker

```
docker run -v `pwd`:/data -p 5000:5000 --rm sigoden/dufs /data -A
```

### With [Homebrew](https://brew.sh)

```
brew install dufs
```

### Binaries on macOS, Linux, Windows

Download from [Github Releases](https://github.com/sigoden/dufs/releases), unzip and add dufs to your $PATH.

## CLI

```
Dufs is a distinctive utility file server - https://github.com/sigoden/dufs

Usage: dufs [OPTIONS] [serve-path]

Arguments:
  [serve-path]  Specific path to serve [default: .]

Options:
  -c, --config <file>        Specify configuration file
  -b, --bind <addrs>         Specify bind address or unix socket
  -p, --port <port>          Specify port to listen on [default: 5000]
      --path-prefix <path>   Specify a path prefix
      --hidden <value>       Hide paths from directory listings, e.g. tmp,*.log,*.lock
  -a, --auth <rules>         Add auth roles, e.g. user:pass@/dir1:rw,/dir2
  -A, --allow-all            Allow all operations
      --allow-upload         Allow upload files/folders
      --allow-delete         Allow delete files/folders
      --allow-search         Allow search files/folders
      --search-limit <count> Set the maximum number of search results [default: 1000]
      --allow-symlink        Allow symlink to files/folders outside root directory
      --allow-archive        Allow download folders as archive file
      --allow-hash           Allow ?hash query to get file sha256 hash
      --enable-cors          Enable CORS, sets `Access-Control-Allow-Origin: *`
      --render-index         Serve index.html when requesting a directory, returns 404 if not found index.html
      --render-try-index     Serve index.html when requesting a directory, returns directory listing if not found index.html
      --render-spa           Serve SPA(Single Page Application)
      --assets <path>        Set the path to the assets directory for overriding the built-in assets
      --log-format <format>  Customize http log format
      --log-file <file>      Specify the file to save logs to, other than stdout/stderr
      --compress <level>     Set zip compress level [default: low] [possible values: none, low, medium, high]
      --shutdown-timeout <seconds>
                             Seconds to wait for in-flight requests on SIGINT/SIGTERM before aborting them [default: 10]
      --token-secret-file <path>  Path to the secret that signs share tokens, created if missing
      --upload-part-ttl <duration>  Remove unfinished upload files after this long, e.g. 90m, 24h, 7d [default: 24h]
      --log-json             Log one JSON object per line instead of text
      --log-skip-health      Do not log health checks that succeed
      --metrics-bind <addr:port>  Serve Prometheus metrics on this extra address, e.g. 127.0.0.1:9101
      --healthcheck [URL]    Probe a running server and exit 0 if it answers 200 [default: http://127.0.0.1:$DUFS_PORT/__dufs__/health]
      --completions <shell>  Print shell completion script for <shell> [possible values: bash, elvish, fish, powershell, zsh]
      --tls-cert <path>      Path to an SSL/TLS certificate to serve with HTTPS
      --tls-key <path>       Path to the SSL/TLS certificate's private key
  -h, --help                 Print help
  -V, --version              Print version
```

## Examples

Serve current working directory in read-only mode

```
dufs
```

Allow all operations like upload/delete/search/create/edit...

```
dufs -A
```

Only allow upload operation

```
dufs --allow-upload
```

Serve a specific directory

```
dufs Downloads
```

Serve a single file

```
dufs linux-distro.iso
```

Serve a single-page application like react/vue

```
dufs --render-spa
```

Serve a static website with index.html

```
dufs --render-index
```

> **Security note:** every other file is served with `Content-Security-Policy: sandbox` and `X-Content-Type-Options: nosniff`, so uploaded HTML/SVG cannot script the dufs origin. `--render-index`, `--render-try-index` and `--render-spa` deliberately serve `index.html` *without* the sandbox header so the site can run its scripts. That page runs on the same origin as the dufs UI and API, so only use these flags when you trust whoever can write `index.html`.

Require username/password

```
dufs -a admin:123@/:rw
```

Listen on specific host:ip 

```
dufs -b 127.0.0.1 -p 80
```

Listen on unix socket
```
dufs -b /tmp/dufs.socket
```

Use https

```
dufs --tls-cert my.crt --tls-key my.key
```

## API

Upload a file

```sh
curl -T path-to-file http://127.0.0.1:5000/new-path/path-to-file
```

Download a file
```sh
curl http://127.0.0.1:5000/path-to-file           # download the file
curl http://127.0.0.1:5000/path-to-file?hash      # retrieve the sha256 hash of the file
```

Download a folder as zip file

```sh
curl -o path-to-folder.zip http://127.0.0.1:5000/path-to-folder?zip
```

Delete a file/folder

```sh
curl -X DELETE http://127.0.0.1:5000/path-to-file-or-folder
```

Create a directory

```sh
curl -X MKCOL http://127.0.0.1:5000/path-to-folder
```

Move the file/folder to the new path

```sh
curl -X MOVE http://127.0.0.1:5000/path -H "Destination: http://127.0.0.1:5000/new-path"
```

List/search directory contents

```sh
curl http://127.0.0.1:5000?q=Dockerfile           # search for files, similar to `find -name Dockerfile`
curl http://127.0.0.1:5000?simple                 # output names only, similar to `ls -1`
curl http://127.0.0.1:5000?json                   # output paths in json format
```

Search returns at most `--search-limit` results (default 1000); the json output then carries `"truncated": true`. Searches and archive downloads share a limited number of concurrent directory walks (one per CPU); when too many requests are waiting, the server answers `503` with a `Retry-After` header.

With authorization (Both basic or digest auth works)

```sh
curl http://127.0.0.1:5000/file --user user:pass                 # basic auth
curl http://127.0.0.1:5000/file --user user:pass --digest        # digest auth
```

Resumable downloads

```sh
curl -C- -o file http://127.0.0.1:5000/file
```

Uploads with `PUT` are atomic: the body is written to a hidden `.<name>.dufs-<id>.part` file next to the target and renamed over it once complete, so an interrupted upload leaves the existing file untouched (and nothing behind for a new one). Leftover `.part` files that were not written to for longer than `--upload-part-ttl` (24 hours by default) are removed at startup and then every hour (or every half TTL if that is shorter), except those of uploads still in progress.

A replaced file is a new inode, so hard links to the old file are not preserved. The old owner, group and permissions are kept, except the setuid and setgid bits, which are dropped. An existing file that is not writable (for example mode `0444`) is not overwritten. If the file is writable but its directory is not, no temp file can be created, so that upload is written in place and is not atomic.

Resumable uploads. Pick a random 32-character hex id and upload with `PUT /file?upload=<id>`. If the transfer is cut off, the bytes received so far stay in the hidden part file instead of being removed. `HEAD /file?upload=<id>` returns them in the `x-dufs-upload-offset` header (`404` once the upload is complete or gone), and `PATCH /file?upload=<id>` with `X-Update-Range: bytes=<offset>-` continues from there. The offset must equal the received length, otherwise the answer is `409` (`x-dufs-error-code: upload-offset-mismatch`). The file is replaced only when the last `PATCH` completes. A failed `PUT` response also carries the id in `x-dufs-upload-id`. Where no part file can be created (see above), the upload is written in place and cannot be resumed.

```sh
id=$(openssl rand -hex 16)
curl -T file "http://127.0.0.1:5000/file?upload=$id"       # interrupted
offset=$(curl -sI "http://127.0.0.1:5000/file?upload=$id" | tr -d '\r' | sed -n 's/^x-dufs-upload-offset: //pI')
tail -c +$((offset + 1)) file | \
  curl -X PATCH -H "X-Update-Range: bytes=$offset-" --data-binary @- "http://127.0.0.1:5000/file?upload=$id"
```

Without an upload id, `PATCH` modifies a file in place (`X-Update-Range: append`, or `bytes=<offset>-`, up to the end of the file). Like overwriting with `PUT`, changing a file that already has content needs `--allow-delete`.

Health checks

```sh
curl http://127.0.0.1:5000/__dufs__/health
```

On SIGINT/SIGTERM dufs stops accepting connections and waits up to `--shutdown-timeout` seconds for in-flight requests before aborting them. While draining, the health check answers `503`.

<details>
<summary><h2>Advanced Topics</h2></summary>

### Access Control

Dufs supports account based access control. You can control who can do what on which path with `--auth`/`-a`.

```
dufs -a admin:admin@/:rw -a guest:guest@/
dufs -a user:pass@/:rw,/dir1 -a @/
```

1. Use `@` to separate the account and paths. No account means anonymous user.
2. Use `:` to separate the username and password of the account.
3. Use `,` to separate paths.
4. Use path suffix `:rw`/`:ro` set permissions: `read-write`/`read-only`. `:ro` can be omitted.

- `-a admin:admin@/:rw`: `admin` has complete permissions for all paths.
- `-a guest:guest@/`: `guest` has read-only permissions for all paths.
- `-a user:pass@/:rw,/dir1`: `user` has read-write permissions for `/*`, has read-only permissions for `/dir1/*`.
- `-a @/`: All paths is publicly accessible, everyone can view/download it.

Recursive operations respect nested rules: deleting or moving a directory is refused when its subtree contains a path with stricter permissions. An account with `:rw` on `/` can therefore not remove or relocate a child that is restricted to `:ro` through `DELETE /` or a directory `MOVE`. Such a refusal returns `403 Forbidden` with `X-Dufs-Error-Code: readonly-descendant`; a plain permission denial returns `X-Dufs-Error-Code: forbidden`.

**Auth permissions are restricted by dufs global permissions.** If dufs does not enable upload permissions via `--allow-upload`, then the account will not have upload permissions even if it is granted `read-write`(`:rw`) permissions.

#### Hashed Password

DUFS supports the use of sha-512 hashed password.

Create hashed password:

```sh
$ openssl passwd -6 123456 # or `mkpasswd -m sha-512 123456`
$6$tWMB51u6Kb2ui3wd$5gVHP92V9kZcMwQeKTjyTRgySsYJu471Jb1I6iHQ8iZ6s07GgCIO69KcPBRuwPE5tDq05xMAzye0NxVKuJdYs/
```

Use hashed password:

```sh
dufs -a 'admin:$6$tWMB51u6Kb2ui3wd$5gVHP92V9kZcMwQeKTjyTRgySsYJu471Jb1I6iHQ8iZ6s07GgCIO69KcPBRuwPE5tDq05xMAzye0NxVKuJdYs/@/:rw'
```
> The hashed password contains `$6`, which can expand to a variable in some shells, so you have to use **single quotes** to wrap it.

Two important things for hashed passwords:

1. Dufs only supports sha-512 hashed passwords, so ensure that the password string always starts with `$6$`.
2. Digest authentication does not function properly with hashed passwords.

#### Share Tokens

`?tokengen` returns a token that grants access to one path for 3 days (this is what the web UI's share button uses). Tokens are signed with a key derived from a server-side secret and the user name, never from the password or its hash.

By default the secret is random and lives only in memory, so every restart invalidates all issued tokens. To keep tokens valid across restarts, store the secret in a file:

```sh
dufs -a admin:admin@/:rw --token-secret-file token.secret
```

The file is created with mode `0600` and a random 32-byte secret (64 hex digits) if it does not exist. On Unix, dufs refuses to start if an existing file is accessible by other users. To use your own secret: `openssl rand -hex 32 > token.secret && chmod 600 token.secret`.

To revoke all tokens, change the secret: delete the file (a new one is created at the next start) or replace its content, then restart. Changing a user's password does not revoke tokens.


### Hide Paths

Dufs supports hiding paths from directory listings via option `--hidden <glob>,...`.

```
dufs --hidden .git,.DS_Store,tmp
```

> The glob used in --hidden only matches file and directory names, not paths. So `--hidden dir1/file` is invalid.

```sh
dufs --hidden '.*'                          # hidden dotfiles
dufs --hidden '*/'                          # hidden all folders
dufs --hidden '*.log,*.lock'                # hidden by exts
dufs --hidden '*.log' --hidden '*.lock'
```

### Log Format

Dufs supports customize http log format with option `--log-format`.

The log format can use following variables.

| variable     | description                                                               |
| ------------ | ------------------------------------------------------------------------- |
| $remote_addr | client address                                                            |
| $remote_user | user name supplied with authentication                                    |
| $request     | full original request line                                                |
| $status      | response status                                                           |
| $http_       | arbitrary request header field. examples: $http_user_agent, $http_referer |

Secrets are redacted from the log: the value of the `token` query parameter in `$request` / `$request_uri` is replaced with `***`, and `$http_authorization`, `$http_proxy_authorization` and `$http_cookie` only log the scheme (`Basic ***`, `Digest username="u" …***`) or the cookie names (`sid=***`).


The default log format is `'$time_iso8601 $log_level - $remote_addr "$request" $status`.
```
2022-08-06T06:59:31+08:00 INFO - 127.0.0.1 "GET /" 200
```

A json log format is also supported.
```
dufs --log-format '{"time":"$time_local","addr":"$remote_addr","uri":"$request_uri", "method":"$request_method","status":$status}'

{"time":"2022-08-06T06:59:31+08:00","addr":"127.0.0.1","uri":"/", "method":"GET","status":200}
```

Disable http log
```
dufs --log-format=''
```

Log user-agent
```
dufs --log-format '$remote_addr "$request" $status $http_user_agent'
```
```
2022-08-06T06:53:55+08:00 INFO - 127.0.0.1 "GET /" 200 Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/104.0.0.0 Safari/537.36
```

Log remote-user
```
dufs --log-format '$remote_addr $remote_user "$request" $status' -a /@admin:admin -a /folder1@user1:pass1
```
```
2022-08-06T07:04:37+08:00 INFO - 127.0.0.1 admin "GET /" 200
```

Besides the variables above, an access line can use:

| variable        | description                                                                  |
| --------------- | ---------------------------------------------------------------------------- |
| $request_time   | seconds from receiving the request to having the response ready, 3 decimals |
| $bytes_sent     | length of the response body, `-` if it is streamed without a known length    |
| $bytes_received | bytes of the request body that dufs read (uploads), `0` otherwise            |
| $error_code     | the `x-dufs-error-code` sent with the response, or `-`                       |
| $upload_id      | the id of a resumable upload (`?upload=`), or `-`                            |

#### JSON logs

`--log-json` (`DUFS_LOG_JSON`, `log-json`) writes every line as one JSON object instead of text. It cannot be combined with `--log-format`.

```
{"ts":"2026-10-24T08:51:01.123Z","level":"INFO","msg":"PUT /inbox/a.bin 201","remote":"172.18.0.1","user":null,"method":"PUT","uri":"/inbox/a.bin","status":201,"duration_ms":12.482,"bytes_sent":0,"bytes_received":1048576,"error_code":null,"upload_id":null}
```

An access line has `ts` (UTC), `level`, `msg`, `remote`, `user` (the name the client authenticated as), `method`, `uri` (the `token` parameter is redacted), `status`, `duration_ms`, `bytes_sent`, `bytes_received`, `error_code` and `upload_id`; a value that does not apply is `null`. `bytes_sent` is the announced length of the response body, `null` for a body streamed without one (such as a zip). A request whose client left in the middle of an upload is logged and counted with status `499`, as nginx does. A request that failed with an I/O error adds `io_kind` (the Rust `ErrorKind`, such as `PermissionDenied`) and `error`; file contents and credentials are never logged.

Everything else dufs logs becomes an object with `ts`, `level` and `msg` as well. Events also have an `event` field and their own fields:

| `event`          | when                                         | fields                                                        |
| ---------------- | -------------------------------------------- | ------------------------------------------------------------- |
| `listening`      | at startup                                   | `addrs`                                                       |
| `part_sweep`     | a sweep removed stale upload files           | `removed`, `remaining`, `remaining_bytes`, `duration_ms`      |
| `walk`           | a search or zip was truncated or cancelled   | `kind` (`search`, `zip`), `result`, `entries`, `duration_ms`  |
| `upload_failed`  | an upload failed or the client left          | `reason` (`io_error`, `aborted`), `upload_id`, `io_kind`, `error`, `bytes_received` |
| `shutdown`       | SIGINT/SIGTERM was received, or the timeout hit | `phase` (`draining`, `timeout`), `timeout_s`               |
| `listener_error` | accepting a connection or the signal handling failed | `io_kind`, `error`                                    |

`--log-skip-health` (`DUFS_LOG_SKIP_HEALTH`, `log-skip-health`) leaves out the access lines of `/__dufs__/health` that answered `200`; failures, such as the `503` while draining, are still logged. Use it when a load balancer or an orchestrator probes the server every few seconds.

### Metrics

`--metrics-bind <addr:port>` (`DUFS_METRICS_BIND`, `metrics-bind`) starts a second listener that serves Prometheus metrics (text format 0.0.4) on `GET /metrics` and answers `404` to everything else. It is off by default and the metrics are never served on the main port.

```sh
dufs --metrics-bind 127.0.0.1:9101
curl http://127.0.0.1:9101/metrics
```

**The metrics listener has no authentication, so do not publish it.** Bind it to a loopback or private address, or in a container to a port that is not mapped to the host and is not behind a reverse proxy or `tailscale serve`; let only the scraper reach it (for example `dufs:9101` on the compose network).

Health checks are not counted in the request metrics. Labels are small fixed sets, never paths, users or ids, so there are at most about 70 series.

| metric | type | labels | meaning |
| ------ | ---- | ------ | ------- |
| `dufs_build_info` | gauge, always 1 | `version`, `commit` | the build |
| `dufs_http_requests_total` | counter | `method` (`GET`, `HEAD`, `PUT`, `PATCH`, `DELETE`, `MOVE`, `COPY`, `PROPFIND`, `other`), `class` (`2xx`, `3xx`, `4xx`, `5xx`) | requests, a series appears with its first request |
| `dufs_http_request_duration_seconds` | histogram, buckets 0.005 0.025 0.1 0.5 2.5 10 60 | | time until the response (headers) is ready, not until a download has been sent |
| `dufs_http_requests_in_flight` | gauge | | requests being handled |
| `dufs_upload_bytes_total` | counter | | request body bytes read by uploads, also of those that did not finish |
| `dufs_uploads_total` | counter | `result` | `ok` stored, `failed` the server failed to store it, `aborted` the client left, `resumed` a resumed upload completed, `refused` turned away with a 4xx before taking data (a `401` challenge is not counted) |
| `dufs_uploads_active` | gauge | | uploads receiving data |
| `dufs_part_files`, `dufs_part_bytes` | gauge | | upload temp files left on disk, as found by the last sweep |
| `dufs_part_files_swept_total` | counter | | stale upload temp files removed |
| `dufs_walks_total` | counter | `kind` (`search`, `zip`), `result` (`complete`, `truncated`, `cancelled`) | directory walks by how they ended |
| `dufs_walks_active`, `dufs_walks_waiting` | gauge | | walks running, and requests waiting for one of the limited walk slots |
| `dufs_auth_failures_total` | counter | `scheme` (`basic`, `digest`, `token`) | credentials that were presented and rejected |
| `dufs_draining` | gauge 0/1 | | 1 once SIGINT/SIGTERM was received. The metrics listener keeps answering while the server drains |
| `process_threads`, `process_open_fds`, `process_resident_memory_bytes`, `process_start_time_seconds` | gauge | | read from `/proc/self` when scraped; Linux only |

`commit` is the git commit of the build; set the `DUFS_COMMIT` environment variable when building without a git checkout (for example `docker build --build-arg DUFS_COMMIT=$(git rev-parse --short=12 HEAD) .`).

### Health check of a container

The image is built `FROM scratch`, so it has no `curl` for a container health check. `dufs --healthcheck [URL]` probes a running server instead: it sends `GET` to `URL` (default `http://127.0.0.1:${DUFS_PORT:-5000}/__dufs__/health`) and exits `0` if the answer is `200` within 3 seconds, otherwise it exits `1` with one line on stderr giving the reason. It starts no server and reads no configuration, only `DUFS_PORT`. Only `http://` URLs are supported; when the server serves a `--path-prefix`, add it to the URL.

```yaml
services:
  dufs:
    image: sigoden/dufs
    command: /data -A
    environment:
      DUFS_PORT: "5000"
    healthcheck:
      test: ["CMD", "/bin/dufs", "--healthcheck"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s
```

While the server drains on SIGTERM the health check answers `503`, so the container turns unhealthy until it exits.

## Environment variables

All options can be set using environment variables prefixed with `DUFS_`.

```
[serve-path]                DUFS_SERVE_PATH="."
    --config <file>         DUFS_CONFIG=config.yaml
-b, --bind <addrs>          DUFS_BIND=0.0.0.0
-p, --port <port>           DUFS_PORT=5000
    --path-prefix <path>    DUFS_PATH_PREFIX=/dufs
    --hidden <value>        DUFS_HIDDEN=tmp,*.log,*.lock
-a, --auth <rules>          DUFS_AUTH="admin:admin@/:rw|@/" 
-A, --allow-all             DUFS_ALLOW_ALL=true
    --allow-upload          DUFS_ALLOW_UPLOAD=true
    --allow-delete          DUFS_ALLOW_DELETE=true
    --allow-search          DUFS_ALLOW_SEARCH=true
    --search-limit <count>  DUFS_SEARCH_LIMIT=1000
    --allow-symlink         DUFS_ALLOW_SYMLINK=true
    --allow-archive         DUFS_ALLOW_ARCHIVE=true
    --allow-hash            DUFS_ALLOW_HASH=true
    --enable-cors           DUFS_ENABLE_CORS=true
    --render-index          DUFS_RENDER_INDEX=true
    --render-try-index      DUFS_RENDER_TRY_INDEX=true
    --render-spa            DUFS_RENDER_SPA=true
    --assets <path>         DUFS_ASSETS=./assets
    --log-format <format>   DUFS_LOG_FORMAT=""
    --log-file <file>       DUFS_LOG_FILE=./dufs.log
    --compress <compress>   DUFS_COMPRESS=low
    --shutdown-timeout <seconds>  DUFS_SHUTDOWN_TIMEOUT=10
    --upload-part-ttl <duration>  DUFS_UPLOAD_PART_TTL=24h
    --log-json              DUFS_LOG_JSON=true
    --log-skip-health       DUFS_LOG_SKIP_HEALTH=true
    --metrics-bind <addr:port>  DUFS_METRICS_BIND=0.0.0.0:9101
    --tls-cert <path>       DUFS_TLS_CERT=cert.pem
    --tls-key <path>        DUFS_TLS_KEY=key.pem
    --token-secret-file <path>  DUFS_TOKEN_SECRET_FILE=token.secret
```

## Configuration File

You can specify and use the configuration file by selecting the option `--config <path-to-config.yaml>`.

The following are the configuration items:

```yaml
serve-path: '.'
bind: 0.0.0.0
port: 5000
path-prefix: /dufs
hidden:
  - tmp
  - '*.log'
  - '*.lock'
auth:
  - admin:admin@/:rw
  - user:pass@/src:rw,/share
  - '@/'  # According to the YAML spec, quoting is required.
allow-all: false
allow-upload: true
allow-delete: true
allow-search: true
search-limit: 1000
allow-symlink: true
allow-archive: true
allow-hash: true
enable-cors: true
render-index: true
render-try-index: true
render-spa: true
assets: ./assets/
log-format: '$remote_addr "$request" $status $http_user_agent'
log-file: ./dufs.log
log-json: false
log-skip-health: false
metrics-bind: 0.0.0.0:9101
compress: low
shutdown-timeout: 10
upload-part-ttl: 24h
tls-cert: tests/data/cert.pem
tls-key: tests/data/key_pkcs1.pem
token-secret-file: token.secret
```

### Customize UI

Dufs allows users to customize the UI with your own assets.

```
dufs --assets my-assets-dir/
```

> If you only need to make slight adjustments to the current UI, you copy dufs's [assets](https://github.com/sigoden/dufs/tree/main/assets) directory and modify it accordingly. The current UI doesn't use any frameworks, just plain HTML/JS/CSS. As long as you have some basic knowledge of web development, it shouldn't be difficult to modify.

Your assets folder must contains a `index.html` file.

`index.html` can use the following placeholder variables to retrieve internal data.

- `__INDEX_DATA__`: directory listing data
- `__ASSETS_PREFIX__`: assets url prefix
- `__PATH_PREFIX__`: path prefix

> A customized 404.html page is also supported.

Here are some Third-party customize UI project:

- https://github.com/TransparentLC/dufs-material-assets
- https://github.com/cercky/dufs_web
- https://github.com/52funny/dufs-tabler-web
- https://github.com/mikigo/dufs-shadcn-assets

</details>

## License

Copyright (c) 2022-2024 dufs-developers.

dufs is made available under the terms of either the MIT License or the Apache License 2.0, at your option.

See the LICENSE-APACHE and LICENSE-MIT files for license details.
