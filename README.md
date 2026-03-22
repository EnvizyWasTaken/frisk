# frisk

`frisk` is a lightweight Linux package manager prototype written in Rust.

## Warning !
`frisk` only supports rust packages and it is a project that i work on in my free time, improvements and support for more languages will come but i cant guarantee its goinna work properly
-EnvizyWasTaken

## Supported sources

- GitHub releases: `frisk github -g owner/repo`
- GitHub source builds (fallback for Rust repos without releases)
- HTTP mirrors: `frisk default -g package_name`
- Local `.frisk` zip packages: `frisk local -g ./package.frisk`

## CLI

```bash
frisk {mirror} {argument} {package}
```

Examples:

```bash
frisk github -g sharkdp/bat
frisk default -g hello
frisk default -d bat
frisk default -u bat
frisk default -U
frisk default -c bat
frisk default -C
frisk local -g ./hello.frisk
```

## Config

`~/.config/frisk/config.json`

```json
{
  "mirrors": [
    "https://example.com/repo"
  ]
}
```

## HTTP mirror format

Each package needs a metadata file named `<package>.json`.

Example:

```json
{
  "name": "hello",
  "version": "1.0.0",
  "file": "files/hello-1.0.0.frisk"
}
```

## `.frisk` package format

A `.frisk` file is a zip archive containing either:

- `manifest.json` and `payload/...`
- or executables directly in the archive

Example `manifest.json`:

```json
{
  "name": "hello",
  "version": "1.0.0",
  "bin": ["payload/hello"]
}
```

## Notes

- GitHub source builds currently assume the repository is a Rust project and that the binary name matches the repo name.
- The installer puts executables into `~/.local/bin`.
- The package database lives at `~/.local/share/frisk/installed.json`.
- This is still a prototype: dependency resolution, checksums, signatures, rollback, and file ownership tracking are not implemented yet.
