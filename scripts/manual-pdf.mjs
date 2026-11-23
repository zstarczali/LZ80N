// docs/manual-{en,hu}.pdf from the .md files: docs/pdf.css and headless
// Chrome (or Edge). Run it from the repository root:
//   npm i --no-save --prefix <TMP> marked@15.0.7 marked-gfm-heading-id@4.1.1 dejavu-fonts-ttf@2.37.3
//   node scripts/manual-pdf.mjs <TMP>
// CHROME=path overrides the browser (Chrome 131+: the page numbers of pdf.css).
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const tmp = path.resolve(process.argv[2] ?? '');
const req = createRequire(path.join(tmp, 'x.js'));
const { Marked } = req('marked');
const { gfmHeadingId } = req('marked-gfm-heading-id');
const browser = [
  process.env.CHROME,
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
].find((p) => p && fs.existsSync(p));
if (!browser) throw new Error('no Chrome or Edge found (set CHROME)');

const ttf = path.join(tmp, 'node_modules/dejavu-fonts-ttf/ttf');
const face = (family, file, extra) =>
  `@font-face { font-family: "${family}"; src: url("${pathToFileURL(path.join(ttf, file))}"); ${extra} }\n`;
let fonts = '';
for (const [family, stem] of [['DejaVu Sans', 'DejaVuSans'], ['DejaVu Sans Mono', 'DejaVuSansMono']]) {
  fonts += face(family, `${stem}.ttf`, '') + face(family, `${stem}-Bold.ttf`, 'font-weight: bold;');
  fonts += face(family, `${stem}-Oblique.ttf`, 'font-style: italic;');
}
const css = fs.readFileSync('docs/pdf.css', 'utf8');
for (const lang of ['en', 'hu']) {
  const body = new Marked(gfmHeadingId()).parse(fs.readFileSync(`docs/manual-${lang}.md`, 'utf8'));
  const html = path.join(tmp, `manual-${lang}.html`);
  const base = pathToFileURL(path.resolve('docs')) + '/';
  // (the images are relative to docs/; the #links must stay in the page)
  const page = body.replace(/(<img [^>]*src=")(?![a-z]+:)/g, `$1${base}`);
  fs.writeFileSync(html, `<!doctype html><html lang="${lang}"><meta charset="utf-8"><title>LZ80N manual (${lang})</title><style>${fonts}${css}</style><body>${page}</body></html>`);
  const pdf = path.join(tmp, `manual-${lang}.pdf`);
  fs.rmSync(pdf, { force: true });
  execFileSync(browser, ['--headless=new', '--disable-gpu', `--user-data-dir=${path.join(tmp, 'profile')}`,
    '--allow-file-access-from-files', '--no-pdf-header-footer', '--generate-pdf-document-outline',
    `--print-to-pdf=${pdf}`, pathToFileURL(html).href], { stdio: 'ignore' });
  for (let i = 0; i < 100 && !fs.existsSync(pdf); i++) Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 100);
  fs.copyFileSync(pdf, `docs/manual-${lang}.pdf`);
  console.log(`docs/manual-${lang}.pdf: ${fs.statSync(pdf).size} bytes`);
}
