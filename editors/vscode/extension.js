// LZ80N language support for VS Code.
// Plain JavaScript, no build step and no dependencies besides the VS Code API.
'use strict';

const vscode = require('vscode');
const cp = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');
const B = require('./builtins');

const LANG = 'lz80n';
let output;
let diagnostics;
const checks = new Map(); // uri -> running check process

// ---------------------------------------------------------------- compiler

function cfg() {
  return vscode.workspace.getConfiguration('lz80n');
}

function exe(name) {
  return process.platform === 'win32' ? name + '.exe' : name;
}

/** The lz80nc to run: setting, then the workspace's own build, then PATH. */
function findCompiler(docUri) {
  const set = cfg().get('compilerPath');
  if (set) return expandHome(set);
  const roots = [];
  const wf = docUri && vscode.workspace.getWorkspaceFolder(docUri);
  if (wf) roots.push(wf.uri.fsPath);
  for (const f of vscode.workspace.workspaceFolders || []) roots.push(f.uri.fsPath);
  if (docUri) {
    // walk up from the file: examples/x.lz80n inside the LZ80N repository
    let d = path.dirname(docUri.fsPath);
    for (let i = 0; i < 6; i++) {
      roots.push(d);
      const up = path.dirname(d);
      if (up === d) break;
      d = up;
    }
  }
  for (const r of roots) {
    for (const kind of ['release', 'debug']) {
      const c = path.join(r, 'compiler', 'target', kind, exe('lz80nc'));
      if (fs.existsSync(c)) return c;
    }
  }
  return 'lz80nc';
}

function expandHome(p) {
  return p.startsWith('~') ? path.join(os.homedir(), p.slice(1)) : p;
}

function compilerArgs(file, extra) {
  const args = [file, ...extra];
  const sj = cfg().get('sjasmplusPath');
  if (sj) args.push('--sjasmplus', expandHome(sj));
  return args;
}

/** Run lz80nc; resolves {code, out} (stdout + stderr interleaved). */
function runCompiler(docUri, args, onSpawn) {
  return new Promise((resolve) => {
    const compiler = findCompiler(docUri);
    let out = '';
    let proc;
    try {
      proc = cp.spawn(compiler, args, { cwd: path.dirname(args[0]), windowsHide: true });
    } catch (e) {
      resolve({ code: -1, out: String(e), compiler });
      return;
    }
    if (onSpawn) onSpawn(proc);
    proc.stdout.on('data', (d) => (out += d));
    proc.stderr.on('data', (d) => (out += d));
    proc.on('error', (e) => {
      out += `\ncould not run ${compiler}: ${e.message}\n` +
        'Build the compiler (cd compiler && cargo build --release) or set "lz80n.compilerPath".';
      resolve({ code: -1, out, compiler });
    });
    proc.on('close', (code) => resolve({ code, out, compiler }));
  });
}

const ERR_RE = /^(.*\.lz80n):(\d+):(\d+): (error|warning): (.*)$/;

/** Turn compiler output into diagnostics for `file`. */
function parseDiagnostics(out, file, doc) {
  const list = [];
  for (const line of out.split(/\r?\n/)) {
    const m = ERR_RE.exec(line);
    if (!m) continue;
    if (path.resolve(m[1]).toLowerCase() !== path.resolve(file).toLowerCase()) continue;
    const ln = Math.max(0, +m[2] - 1);
    const col = Math.max(0, +m[3] - 1);
    let range = new vscode.Range(ln, col, ln, col + 1);
    if (doc && ln < doc.lineCount) {
      const w = doc.getWordRangeAtPosition(new vscode.Position(ln, col));
      if (w) range = w;
      else if (col >= doc.lineAt(ln).text.length) range = doc.lineAt(ln).range;
    }
    const d = new vscode.Diagnostic(range, m[5],
      m[4] === 'warning' ? vscode.DiagnosticSeverity.Warning : vscode.DiagnosticSeverity.Error);
    d.source = 'lz80nc';
    list.push(d);
  }
  return list;
}

/** Background check: compile to assembly only, into a temporary folder. */
async function check(doc) {
  if (doc.languageId !== LANG || doc.uri.scheme !== 'file') return;
  const key = doc.uri.toString();
  const prev = checks.get(key);
  if (prev) prev.kill();
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'lz80n-'));
  const base = path.basename(doc.fileName, path.extname(doc.fileName));
  const args = compilerArgs(doc.fileName, ['-S', '-o', path.join(tmp, base + '.nex')]);
  let me;
  const res = await runCompiler(doc.uri, args, (p) => { me = p; checks.set(key, p); });
  if (checks.get(key) !== me) return; // a newer check replaced this one
  checks.delete(key);
  fs.rm(tmp, { recursive: true, force: true }, () => {});
  if (res.code === -1) {
    output.appendLine(res.out);
    return;
  }
  diagnostics.set(doc.uri, parseDiagnostics(res.out, doc.fileName, doc));
}

/**
 * The document a command works on: the file right-clicked in the explorer or the
 * editor (`uri`), else the active editor. Saved first when it has changes.
 */
async function activeDoc(uri) {
  let doc;
  if (uri instanceof vscode.Uri && uri.scheme === 'file') {
    doc = await vscode.workspace.openTextDocument(uri);
  } else {
    doc = vscode.window.activeTextEditor && vscode.window.activeTextEditor.document;
  }
  if (!doc || (doc.languageId !== LANG && !/\.lz80n$/i.test(doc.fileName))) {
    vscode.window.showWarningMessage('Open an .lz80n file first.');
    return undefined;
  }
  if (doc.isUntitled) {
    vscode.window.showWarningMessage('Save the file first.');
    return undefined;
  }
  if (doc.isDirty) await doc.save();
  return doc;
}

/** Full build to .nex next to the source. Returns the .nex path or undefined. */
async function build(doc, extra = []) {
  const args = compilerArgs(doc.fileName, [...cfg().get('compilerArgs'), ...extra]);
  output.clear();
  output.appendLine(`> ${findCompiler(doc.uri)} ${args.join(' ')}`);
  const res = await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Window, title: `LZ80N: compiling ${path.basename(doc.fileName)}` },
    () => runCompiler(doc.uri, args));
  output.append(res.out);
  diagnostics.set(doc.uri, parseDiagnostics(res.out, doc.fileName, doc));
  if (res.code !== 0) {
    output.show(true);
    vscode.window.showErrorMessage(`LZ80N: build failed (${path.basename(doc.fileName)})`);
    return undefined;
  }
  return doc.fileName.replace(/\.[^.\\/]*$/, '') + (extra.includes('-S') ? '.asm' : '.nex');
}

async function cmdBuild(uri, extra = []) {
  const doc = await activeDoc(uri);
  if (!doc) return;
  const nex = await build(doc, extra);
  if (nex) vscode.window.setStatusBarMessage(`$(check) LZ80N: wrote ${path.basename(nex)}`, 5000);
}

/** Shows the .nex of the file in the system file manager, building it first if there is none. */
async function cmdRevealNex(uri) {
  const doc = await activeDoc(uri);
  if (!doc) return;
  let nex = doc.fileName.replace(/\.[^.\\/]*$/, '') + '.nex';
  if (!fs.existsSync(nex)) nex = await build(doc);
  if (nex) vscode.commands.executeCommand('revealFileInOS', vscode.Uri.file(nex));
}

async function cmdShowAsm(uri) {
  const doc = await activeDoc(uri);
  if (!doc) return;
  const asm = await build(doc, ['-S']);
  if (asm) {
    const d = await vscode.workspace.openTextDocument(asm);
    vscode.window.showTextDocument(d, vscode.ViewColumn.Beside);
  }
}

async function cmdRun(uri) {
  const doc = await activeDoc(uri);
  if (!doc) return;
  let emu = cfg().get('emulatorPath');
  if (!emu) {
    const pick = await vscode.window.showOpenDialog({
      title: 'Select the emulator (e.g. CSpect.exe)',
      canSelectMany: false,
      filters: process.platform === 'win32' ? { Programs: ['exe'] } : undefined,
    });
    if (!pick) return;
    emu = pick[0].fsPath;
    await cfg().update('emulatorPath', emu, vscode.ConfigurationTarget.Global);
  }
  emu = expandHome(emu);
  const nex = await build(doc);
  if (!nex) return;
  let args = cfg().get('emulatorArgs').map((a) => a.split('${file}').join(nex));
  if (!cfg().get('emulatorArgs').some((a) => a.includes('${file}'))) args.push(nex);
  output.appendLine(`> ${emu} ${args.join(' ')}`);
  try {
    const p = cp.spawn(emu, args, { cwd: path.dirname(emu), detached: true, stdio: 'ignore' });
    p.on('error', (e) => vscode.window.showErrorMessage(`LZ80N: could not start the emulator: ${e.message}`));
    p.unref();
  } catch (e) {
    vscode.window.showErrorMessage(`LZ80N: could not start the emulator: ${e.message}`);
  }
}

// ---------------------------------------------------------------- tasks

class TaskProvider {
  provideTasks() {
    return [this.make({ type: LANG }, 'build current file')];
  }
  resolveTask(task) {
    return this.make(task.definition, task.name);
  }
  make(def, name) {
    const args = compilerArgs(def.file || '${file}', def.args || cfg().get('compilerArgs'));
    const ed = vscode.window.activeTextEditor;
    const uri = ed && ed.document.languageId === LANG ? ed.document.uri : undefined;
    const task = new vscode.Task(def, vscode.TaskScope.Workspace, name, LANG,
      new vscode.ProcessExecution(findCompiler(uri), args, { cwd: '${fileDirname}' }),
      '$lz80nc');
    task.group = vscode.TaskGroup.Build;
    return task;
  }
}

// ---------------------------------------------------------------- source model

const OPENERS = /\b(function|if|while|for|class|struct|interrupt|asm)\b/g;

/** Strip strings, char literals and comments from one line (keeps columns). */
function code(line) {
  return line
    .replace(/"(\\.|[^"\\])*"?/g, (s) => ' '.repeat(s.length))
    .replace(/'(\\.|[^'\\])'/g, '   ')
    .replace(/--.*$/, '');
}

/**
 * A light outline of the file: functions, classes/structs (with fields and
 * methods), consts, assets, interrupts and global variables.
 */
function scan(doc) {
  const syms = [];
  const stack = []; // { kind, sym }
  let inAsm = false;
  const asm = []; // [first body line, end line]
  let inBlockComment = false;
  const typeNames = 'bool|byte|sbyte|word|sword|fixed|string|[A-Z][A-Za-z0-9_]*';
  const varRe = new RegExp(`^\\s*(far\\s+)?(${typeNames})\\s+([A-Za-z_][A-Za-z0-9_]*)\\s*(\\[[^\\]]*\\](\\s*\\[[^\\]]*\\])*)?\\s*(=|$)`);
  for (let i = 0; i < doc.lineCount; i++) {
    const raw = doc.lineAt(i).text;
    if (inAsm) {
      if (/^\s*end\s*(;.*)?$/.test(raw)) { inAsm = false; stack.pop(); asm[asm.length - 1][1] = i; }
      continue;
    }
    let text = raw;
    if (inBlockComment) {
      const e = text.indexOf(']]');
      if (e < 0) continue;
      inBlockComment = false;
      text = ' '.repeat(e + 2) + text.slice(e + 2);
    }
    const bc = text.indexOf('--[[');
    if (bc >= 0 && !/"[^"]*--\[\[/.test(text.slice(0, bc + 4))) {
      const e = text.indexOf(']]', bc + 4);
      if (e < 0) { inBlockComment = true; text = text.slice(0, bc); }
      else text = text.slice(0, bc) + ' '.repeat(e + 2 - bc) + text.slice(e + 2);
    }
    const c = code(text);
    const parent = [...stack].reverse().find((s) => s.sym && (s.kind === 'class' || s.kind === 'struct'));
    const inFunc = stack.some((s) => s.kind === 'function' || s.kind === 'interrupt');
    const depth = stack.length;
    let m;
    const range = (start, len) => new vscode.Range(i, start, i, start + len);
    let opened = null;

    if ((m = /^\s*function\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)\s*(->\s*([A-Za-z_][A-Za-z0-9_]*))?/.exec(c))) {
      const col = c.indexOf(m[1], c.indexOf('function') + 8);
      opened = {
        kind: 'function', name: m[1], params: m[2].trim(), ret: m[4], line: i,
        sel: range(col, m[1].length), detail: `(${m[2].trim()})${m[4] ? ' -> ' + m[4] : ''}`,
        parent: parent && parent.sym, children: [],
      };
    } else if ((m = /^\s*(class|struct)\s+([A-Za-z_][A-Za-z0-9_]*)/.exec(c))) {
      const col = c.indexOf(m[2], c.indexOf(m[1]) + m[1].length);
      opened = { kind: m[1], name: m[2], line: i, sel: range(col, m[2].length), detail: m[1], children: [] };
    } else if ((m = /^\s*interrupt\s+(raster\s*\([^)]*\)|frame)/.exec(c))) {
      const col = c.indexOf('interrupt');
      opened = { kind: 'interrupt', name: 'interrupt ' + m[1].replace(/\s+/g, ''), line: i, sel: range(col, 9 + 1 + m[1].length), detail: '', children: [] };
    } else if ((m = /^\s*const\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$/.exec(c)) && !inFunc) {
      syms.push({ kind: 'const', name: m[1], line: i, end: i, sel: range(c.indexOf(m[1], c.indexOf('const') + 5), m[1].length), detail: '= ' + m[2].trim() });
    } else if ((m = /^\s*asset\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$/.exec(c))) {
      syms.push({ kind: 'asset', name: m[1], line: i, end: i, sel: range(c.indexOf(m[1], c.indexOf('asset') + 5), m[1].length), detail: m[2].trim() });
    } else if ((m = varRe.exec(c)) && !inFunc && !/^\s*(return|if|while|for|elseif|and|or|not)\b/.test(c)) {
      const s = { kind: parent ? 'field' : 'var', name: m[3], type: m[2] + (m[4] ? m[4].replace(/\s+/g, '') : ''), line: i, end: i, sel: range(c.indexOf(m[3], c.indexOf(m[2]) + m[2].length), m[3].length) };
      s.detail = s.type;
      if (parent) parent.sym.children.push(s);
      else if (depth === 0) syms.push(s);
    }

    // block depth: count openers and `end`s on the line
    if (/^\s*asm\s*$/.test(c)) {
      inAsm = true;
      asm.push([i + 1, doc.lineCount]);
      stack.push({ kind: 'asm' });
      continue;
    }
    const tokens = c.match(/\b(function|if|while|for|class|struct|interrupt|end)\b/g) || [];
    let first = true;
    for (const t of tokens) {
      if (t === 'end') {
        const top = stack.pop();
        if (top && top.sym) {
          top.sym.end = i;
          if (top.sym.parent) top.sym.parent.children.push(top.sym);
          else syms.push(top.sym);
        }
      } else {
        stack.push({ kind: t, sym: first && opened ? opened : null });
        first = false;
      }
    }
  }
  // unterminated blocks still appear in the outline
  for (const s of stack) {
    if (s.sym) {
      s.sym.end = doc.lineCount - 1;
      if (s.sym.parent) s.sym.parent.children.push(s.sym);
      else syms.push(s.sym);
    }
  }
  syms.sort((a, b) => a.line - b.line);
  syms.asm = asm;
  return syms;
}

const scanCache = new WeakMap();
function model(doc) {
  const c = scanCache.get(doc);
  if (c && c.version === doc.version) return c.syms;
  const syms = scan(doc);
  scanCache.set(doc, { version: doc.version, syms });
  return syms;
}

function* walk(syms) {
  for (const s of syms) {
    yield s;
    if (s.children) yield* walk(s.children);
  }
}

const KIND = {
  function: vscode.SymbolKind.Function,
  class: vscode.SymbolKind.Class,
  struct: vscode.SymbolKind.Struct,
  interrupt: vscode.SymbolKind.Event,
  const: vscode.SymbolKind.Constant,
  asset: vscode.SymbolKind.File,
  var: vscode.SymbolKind.Variable,
  field: vscode.SymbolKind.Field,
};

function toDocSymbol(doc, s) {
  const full = new vscode.Range(s.line, 0, s.end, doc.lineAt(Math.min(s.end, doc.lineCount - 1)).text.length);
  const kind = s.kind === 'function' && s.parent ? vscode.SymbolKind.Method : KIND[s.kind];
  const ds = new vscode.DocumentSymbol(s.name, s.detail || '', kind, full, s.sel);
  ds.children = (s.children || []).sort((a, b) => a.line - b.line).map((c) => toDocSymbol(doc, c));
  return ds;
}

/** Comment lines directly above a declaration. */
function docComment(doc, line) {
  const out = [];
  for (let i = line - 1; i >= 0; i--) {
    const m = /^\s*--(?!\[\[)\s?(.*)$/.exec(doc.lineAt(i).text);
    if (!m) break;
    out.unshift(m[1]);
  }
  return out.join('\n');
}

function userSymbolMarkdown(doc, s) {
  const md = new vscode.MarkdownString();
  let sig;
  switch (s.kind) {
    case 'function': sig = `function ${s.parent ? s.parent.name + '.' : ''}${s.name}(${s.params})${s.ret ? ' -> ' + s.ret : ''}`; break;
    case 'const': sig = `const ${s.name} ${s.detail}`; break;
    case 'asset': sig = `asset ${s.name} = ${s.detail}`; break;
    case 'class': case 'struct': sig = `${s.kind} ${s.name}`; break;
    default: sig = `${s.type} ${s.name}`;
  }
  md.appendCodeblock(sig, LANG);
  if (s.kind === 'class' || s.kind === 'struct') {
    const fields = (s.children || []).filter((c) => c.kind === 'field').map((c) => `${c.type} ${c.name}`);
    if (fields.length) md.appendCodeblock(fields.join('\n'), LANG);
  }
  const dc = docComment(doc, s.line);
  if (dc) md.appendMarkdown('\n' + dc.replace(/\n/g, '  \n'));
  return md;
}

function builtinMarkdown(word) {
  const f = B.FUNCTIONS[word] || B.ARRAYS[word];
  if (f) {
    const md = new vscode.MarkdownString();
    md.appendCodeblock(f[0], LANG);
    md.appendMarkdown(`${f[1]}\n\n*Built-in — manual chapter ${f[2]}*`);
    return md;
  }
  if (B.TYPES[word]) return new vscode.MarkdownString(`**${word}** — ${B.TYPES[word]}`);
  if (B.CONSTANTS[word]) return new vscode.MarkdownString(`**${word}** — ${B.CONSTANTS[word]}`);
  return undefined;
}

// ---------------------------------------------------------------- member types

/** Find the declared type name of `name` visible at `line`. */
function typeOf(doc, name, line) {
  const syms = model(doc);
  // parameter of the enclosing function
  for (const s of walk(syms)) {
    if (s.kind === 'function' && s.line <= line && line <= s.end) {
      const m = new RegExp(`(?:^|,)\\s*([A-Za-z_][A-Za-z0-9_]*)\\s+${name}\\s*(?:,|$)`).exec(s.params);
      if (m) return m[1];
    }
  }
  // local or global declaration above the line
  const re = new RegExp(`^\\s*(?:far\\s+)?([A-Za-z_][A-Za-z0-9_]*)\\s+${name}\\b`);
  for (let i = line; i >= 0; i--) {
    const m = re.exec(code(doc.lineAt(i).text));
    if (m && !B.KEYWORDS.includes(m[1])) return m[1];
  }
  return undefined;
}

function enclosingClass(doc, line) {
  return model(doc).find((s) => (s.kind === 'class' || s.kind === 'struct') && s.line <= line && line <= s.end);
}

const SPRITE_MEMBERS = [
  ['x', 'field', 'word'], ['y', 'field', 'word'], ['pattern', 'field', 'byte'], ['palette', 'field', 'byte  (offset 0..15)'],
  ['mirror_x', 'field', 'bool'], ['mirror_y', 'field', 'bool'], ['rotate', 'field', 'bool'],
  ['scale', 'field', 'byte  (0..3: x1, x2, x4, x8)'], ['visible', 'field', 'bool'], ['id', 'field', 'byte  (read only)'],
  ['position', 'method', '(x, y) — move and update'], ['show', 'method', '(pattern) — set the pattern and show it'],
  ['hide', 'method', '() — hide it'], ['update', 'method', '() — send all fields to the hardware'],
  ['overlaps', 'method', '(other) -> bool — do the squares (16 << scale) of two visible sprites overlap?'],
];

/** Resolve `a.b.c` before the cursor to a class/struct symbol. */
function resolveChain(doc, pos, chain) {
  const syms = model(doc);
  const types = new Map(syms.filter((s) => s.kind === 'class' || s.kind === 'struct').map((s) => [s.name, s]));
  const parts = chain.split('.');
  let t;
  const head = parts.shift();
  if (/^sprite\[/.test(head)) t = 'Sprite';
  else if (head === 'self') { const c = enclosingClass(doc, pos.line); t = c && c.name; }
  else t = typeOf(doc, head.replace(/\[.*$/, ''), pos.line);
  for (const p of parts) {
    if (!t) return undefined;
    const cls = types.get(t);
    if (!cls) return undefined;
    const f = cls.children.find((c) => c.name === p.replace(/\[.*$/, ''));
    t = f && f.type ? f.type.replace(/\[.*$/, '') : undefined;
  }
  if (t === 'Sprite') return 'Sprite';
  return t && types.get(t);
}

// ---------------------------------------------------------------- providers

function wordAt(doc, pos) {
  const r = doc.getWordRangeAtPosition(pos, /[A-Za-z_][A-Za-z0-9_]*/);
  return r ? doc.getText(r) : undefined;
}

function inAsmOrComment(doc, pos) {
  const t = doc.lineAt(pos.line).text.slice(0, pos.character);
  if (model(doc).asm.some(([a, b]) => a <= pos.line && pos.line < b)) return true;
  return code(t).length < t.length || (t.replace(/\\./g, '').match(/"/g) || []).length % 2 === 1;
}

const hoverProvider = {
  provideHover(doc, pos) {
    const word = wordAt(doc, pos);
    if (!word || inAsmOrComment(doc, pos)) return undefined;
    const before = doc.lineAt(pos.line).text.slice(0, doc.getWordRangeAtPosition(pos).start.character);
    const member = /([A-Za-z_][\w]*(\[[^\]]*\])?(\.[A-Za-z_]\w*)*)\.\s*$/.exec(before);
    if (member) {
      const t = resolveChain(doc, pos, member[1]);
      if (t === 'Sprite') {
        const m = SPRITE_MEMBERS.find((x) => x[0] === word);
        if (m) return new vscode.Hover(new vscode.MarkdownString(`**Sprite.${m[0]}** ${m[2]}`));
      } else if (t) {
        const c = t.children.find((x) => x.name === word);
        if (c) return new vscode.Hover(userSymbolMarkdown(doc, c));
      }
      return undefined;
    }
    const user = [...walk(model(doc))].find((s) => s.name === word && !s.parent && s.kind !== 'field');
    if (user) return new vscode.Hover(userSymbolMarkdown(doc, user));
    const md = builtinMarkdown(word);
    return md && new vscode.Hover(md);
  },
};

const symbolProvider = {
  provideDocumentSymbols(doc) {
    return model(doc).map((s) => toDocSymbol(doc, s));
  },
};

const definitionProvider = {
  provideDefinition(doc, pos) {
    const word = wordAt(doc, pos);
    if (!word) return undefined;
    const before = doc.lineAt(pos.line).text.slice(0, doc.getWordRangeAtPosition(pos).start.character);
    const member = /([A-Za-z_][\w]*(\[[^\]]*\])?(\.[A-Za-z_]\w*)*)\.\s*$/.exec(before);
    let target;
    if (member) {
      const t = resolveChain(doc, pos, member[1]);
      if (t && t !== 'Sprite') target = t.children.find((c) => c.name === word);
    } else {
      target = [...walk(model(doc))].find((s) => s.name === word && !s.parent && s.kind !== 'field');
    }
    return target && new vscode.Location(doc.uri, target.sel);
  },
};

const completionProvider = {
  provideCompletionItems(doc, pos) {
    if (inAsmOrComment(doc, pos)) return undefined;
    const before = doc.lineAt(pos.line).text.slice(0, pos.character);
    const member = /([A-Za-z_][\w]*(\[[^\]]*\])?(\.[A-Za-z_]\w*)*)\.\s*[A-Za-z_0-9]*$/.exec(before);
    const CK = vscode.CompletionItemKind;
    if (member) {
      const t = resolveChain(doc, pos, member[1]);
      if (t === 'Sprite') {
        return SPRITE_MEMBERS.map(([n, k, d]) => {
          const it = new vscode.CompletionItem(n, k === 'method' ? CK.Method : CK.Field);
          it.detail = d;
          return it;
        });
      }
      if (t) {
        return t.children.map((c) => {
          const it = new vscode.CompletionItem(c.name, c.kind === 'function' ? CK.Method : CK.Field);
          it.detail = c.detail;
          it.documentation = userSymbolMarkdown(doc, c);
          return it;
        });
      }
      return [];
    }
    const items = [];
    const seen = new Set();
    const add = (label, kind, detail, docs) => {
      if (seen.has(label)) return;
      seen.add(label);
      const it = new vscode.CompletionItem(label, kind);
      if (detail) it.detail = detail;
      if (docs) it.documentation = docs;
      items.push(it);
    };
    for (const k of B.KEYWORDS) add(k, CK.Keyword);
    for (const [t, d] of Object.entries(B.TYPES)) add(t, CK.TypeParameter, undefined, new vscode.MarkdownString(d));
    for (const [n, f] of Object.entries(B.FUNCTIONS)) add(n, CK.Function, f[0], new vscode.MarkdownString(f[1]));
    for (const [n, f] of Object.entries(B.ARRAYS)) add(n, CK.Variable, f[0], new vscode.MarkdownString(f[1]));
    for (const [n, d] of Object.entries(B.CONSTANTS)) add(n, CK.Constant, undefined, new vscode.MarkdownString(d));
    const syms = model(doc);
    const kinds = { function: CK.Function, class: CK.Class, struct: CK.Struct, const: CK.Constant, asset: CK.File, var: CK.Variable };
    for (const s of syms) if (kinds[s.kind]) add(s.name, kinds[s.kind], s.detail, userSymbolMarkdown(doc, s));
    // locals and parameters of the enclosing function
    const fn = [...walk(syms)].find((s) => s.kind === 'function' && s.line <= pos.line && pos.line <= s.end);
    if (fn) {
      for (const p of fn.params.split(',')) {
        const m = /([A-Za-z_]\w*)\s+([A-Za-z_]\w*)/.exec(p);
        if (m) add(m[2], CK.Variable, m[1] + ' (parameter)');
      }
      for (let i = fn.line + 1; i <= pos.line; i++) {
        const m = /^\s*(?:far\s+)?(bool|byte|sbyte|word|sword|fixed|string|[A-Z]\w*)\s+([A-Za-z_]\w*)/.exec(code(doc.lineAt(i).text));
        if (m) add(m[2], CK.Variable, m[1]);
        const f = /^\s*for\s+([A-Za-z_]\w*)\s*=/.exec(doc.lineAt(i).text);
        if (f) add(f[1], CK.Variable, 'loop variable');
      }
    }
    return items;
  },
};

const signatureProvider = {
  provideSignatureHelp(doc, pos) {
    const text = doc.getText(new vscode.Range(Math.max(0, pos.line - 5), 0, pos.line, pos.character));
    const c = code(text);
    // find the innermost open '(' and count commas at its level
    let depth = 0;
    let commas = 0;
    let i = c.length - 1;
    for (; i >= 0; i--) {
      const ch = c[i];
      if (ch === ')' || ch === ']' || ch === '}') depth++;
      else if (ch === '(' || ch === '[' || ch === '{') {
        if (depth === 0) break;
        depth--;
      } else if (ch === ',' && depth === 0) commas++;
    }
    if (i < 0 || c[i] !== '(') return undefined;
    const m = /([A-Za-z_]\w*)\s*$/.exec(c.slice(0, i));
    if (!m) return undefined;
    const name = m[1];
    let label;
    let docs;
    if (B.FUNCTIONS[name]) {
      label = B.FUNCTIONS[name][0];
      docs = B.FUNCTIONS[name][1];
    } else {
      const s = [...walk(model(doc))].find((x) => x.kind === 'function' && x.name === name);
      if (!s) return undefined;
      label = `${name}(${s.params})${s.ret ? ' -> ' + s.ret : ''}`;
      docs = docComment(doc, s.line);
    }
    const sig = new vscode.SignatureInformation(label, new vscode.MarkdownString(docs || ''));
    const inner = /\((.*)\)/.exec(label);
    if (inner && inner[1].trim()) {
      sig.parameters = inner[1].split(',').map((p) => new vscode.ParameterInformation(p.trim()));
    }
    const help = new vscode.SignatureHelp();
    help.signatures = [sig];
    help.activeSignature = 0;
    help.activeParameter = Math.min(commas, Math.max(0, (sig.parameters || []).length - 1));
    return help;
  },
};

// ---------------------------------------------------------------- activation

function activate(context) {
  output = vscode.window.createOutputChannel('LZ80N');
  diagnostics = vscode.languages.createDiagnosticCollection(LANG);
  const sel = { language: LANG };
  context.subscriptions.push(
    output,
    diagnostics,
    vscode.commands.registerCommand('lz80n.build', (uri) => cmdBuild(uri)),
    vscode.commands.registerCommand('lz80n.buildNoOpt', (uri) => cmdBuild(uri, ['-O0'])),
    vscode.commands.registerCommand('lz80n.buildBankAll', (uri) => cmdBuild(uri, ['--bank-all'])),
    vscode.commands.registerCommand('lz80n.run', cmdRun),
    vscode.commands.registerCommand('lz80n.showAsm', cmdShowAsm),
    vscode.commands.registerCommand('lz80n.revealNex', cmdRevealNex),
    vscode.commands.registerCommand('lz80n.check', async (uri) => {
      const d = await activeDoc(uri);
      if (d) check(d);
    }),
    vscode.languages.registerHoverProvider(sel, hoverProvider),
    vscode.languages.registerDocumentSymbolProvider(sel, symbolProvider),
    vscode.languages.registerDefinitionProvider(sel, definitionProvider),
    vscode.languages.registerCompletionItemProvider(sel, completionProvider, '.'),
    vscode.languages.registerSignatureHelpProvider(sel, signatureProvider, '(', ','),
    vscode.tasks.registerTaskProvider(LANG, new TaskProvider()),
    vscode.workspace.onDidSaveTextDocument((d) => {
      if (d.languageId === LANG && cfg().get('checkOnSave')) check(d);
    }),
    vscode.workspace.onDidOpenTextDocument((d) => {
      if (d.languageId === LANG && cfg().get('checkOnOpen')) check(d);
    }),
    vscode.workspace.onDidCloseTextDocument((d) => diagnostics.delete(d.uri)),
  );
  if (cfg().get('checkOnOpen')) {
    for (const d of vscode.workspace.textDocuments) if (d.languageId === LANG) check(d);
  }
}

function deactivate() {
  for (const p of checks.values()) p.kill();
}

module.exports = { activate, deactivate, _test: { scan, code, parseDiagnostics, ERR_RE } };
