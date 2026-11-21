# LZ80N

## Version bump checklist

On every new version update all of these, in one go:

1. `compiler/Cargo.toml` and `compiler/Cargo.lock` (`lz80nc --version`). The Windows `.exe`
   version information (Properties -> Details) is built from `Cargo.toml` by `compiler/build.rs`;
   only the copyright year (`year`) and `COMPANY` are written there by hand
2. `CHANGELOG.md` — a new `## x.y.z — YYYY-MM-DD` section
3. `README.md` — the version badge and the `Status:` line
4. `docs/manual-en.md` and `docs/manual-hu.md` — the version line, plus the new features
5. `docs/spec.md` — new language elements and library functions
6. `docs/manual-en.pdf` and `docs/manual-hu.pdf` — regenerate them the way
   `.github/workflows/release.yml` does: pandoc 3.5 (`-f gfm -t html5 --standalone --css pdf.css`)
   then WeasyPrint 63.1, with the DejaVu Sans / DejaVu Sans Mono fonts
7. VS Code extension (`editors/vscode`), when the language or library changed:
   `package.json` version, `CHANGELOG.md`, `builtins.js` (hover/completion), the function regex in
   `syntaxes/lz80n.tmLanguage.json`, `SPRITE_MEMBERS` etc. in `extension.js`.
   Check with `node --check builtins.js extension.js`, then build the package with
   `npx @vscode/vsce package` (the `.vsix` is gitignored)
8. **The product site, `F:\Development\product-site`** (a separate git repository):
   - `lz80n.html`: `"softwareVersion"` in the JSON-LD, the `Download LZ80N x.y.z` hero button,
     and the feature cards / example count if they changed
   - `index.html`: a new `STUDIO NEWS` section for the version (dated, with the main changes),
     above the older LZ80N sections
   - the download archives need no change: `download.php` and `downloads/LZ80N/index.php`
     serve whatever the `publish` workflow uploaded

Releasing: the `v*` tag must match `Cargo.toml`; it starts `release.yml`. `publish.yml`
(workflow_dispatch) uploads the archives to the site by FTP. Push, merge to `main` and tagging only
when asked.
