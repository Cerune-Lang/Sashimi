const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

// Deliberately limited to inline links/images and reference definitions used in
// repository guides. No network, anchor validation, or semantic-status claims.
function destinations(markdown) {
  let fence = null;
  const links = [];
  for (const line of markdown.split(/\r?\n/)) {
    const marker = line.match(/^ {0,3}(`{3,}|~{3,})/);
    if (marker) {
      if (!fence) fence = marker[1];
      else if (marker[1][0] === fence[0] && marker[1].length >= fence.length) fence = null;
      continue;
    }
    if (fence || /^( {4}|\t)/.test(line)) continue;
    const prose = line.replace(/(`+).*?\1/g, '');
    const inline = /\[[^\]\n]*\]\(\s*(<[^>\n]*>|[^\s)]+)(?:\s+["'][^]*?["'])?\s*\)/g;
    for (const match of prose.matchAll(inline)) links.push(match[1].replace(/^<|>$/g, ''));
    const reference = prose.match(/^ {0,3}\[[^\]]+\]:\s*(<[^>]*>|\S+)/);
    if (reference) links.push(reference[1].replace(/^<|>$/g, ''));
  }
  return links;
}

function check(root, files) {
  const known = new Set(files);
  const errors = [];
  const docs = files.filter(file => file.endsWith('.md'));
  let links = 0;
  for (const file of docs) {
    let markdown;
    try { markdown = fs.readFileSync(path.join(root, file), 'utf8'); }
    catch { errors.push(file + ': missing tracked document'); continue; }
    if (/https:\/\/github\.com\/Hokutaka\/Sashimi(?:[/#?\s)]|$)/i.test(markdown)) {
      errors.push(file + ': obsolete Sashimi repository URL');
    }
    for (const dest of destinations(markdown)) {
      if (/^(?:[a-z][a-z0-9+.-]*:|\/\/|#)/i.test(dest)) continue;
      let relative;
      try { relative = decodeURIComponent(dest.split(/[?#]/)[0]); }
      catch { errors.push(file + ': invalid URL encoding: ' + dest); continue; }
      if (!relative) continue;
      links++;
      const resolved = path.posix.normalize(relative.startsWith('/')
        ? relative.slice(1) : path.posix.join(path.posix.dirname(file), relative));
      // Validate against Git's file set, so local ignored files cannot hide
      // broken links in CI. Exact names also catch case-only Windows mistakes.
      if (!known.has(resolved) && !files.some(name => name.startsWith(resolved.replace(/\/$/, '') + '/'))) {
        errors.push(file + ': missing local target: ' + dest);
      }
    }
    // SECURITY.md is an English GitHub policy; AGENTS are local instructions.
    // All public guides must have a named translation pair.
    const guide = /^(?:docs|examples|experiments|tests\/fixtures)\//.test(file) || /^README(?:\.en)?\.md$/.test(file);
    if (!guide) continue;
    let partner;
    if (file.endsWith('.ja.md')) partner = file.replace(/\.ja\.md$/, '.en.md');
    else if (file.endsWith('.en.md')) partner = file.endsWith('README.en.md')
      ? file.replace(/README\.en\.md$/, 'README.md') : file.replace(/\.en\.md$/, '.ja.md');
    else if (file.endsWith('README.md')) partner = file.replace(/README\.md$/, 'README.en.md');
    else { errors.push(file + ': use .ja.md/.en.md or README.md/README.en.md'); continue; }
    if (!known.has(partner)) errors.push(file + ': missing translation: ' + partner);
  }
  return { errors, documents: docs.length, links };
}

if (require.main === module) {
  const root = path.resolve(__dirname, '..');
  const files = execFileSync('git', ['ls-files', '-z', '--cached', '--others', '--exclude-standard'], { cwd: root, encoding: 'utf8' })
    .split('\0').filter(Boolean);
  const result = check(root, [...new Set(files)]);
  for (const error of result.errors) console.error(error);
  console.log('Documentation: ' + result.documents + ' files, ' + result.links + ' local links, ' + result.errors.length + ' errors');
  process.exitCode = result.errors.length ? 1 : 0;
}
module.exports = { check, destinations };
