const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { test } = require('node:test');
const { check, destinations } = require('./check-docs.cjs');
function fixture(t, entries) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'sashimi-docs-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const [name, content] of Object.entries(entries)) {
    fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true });
    fs.writeFileSync(path.join(root, name), content);
  }
  return { root, files: Object.keys(entries) };
}
test('relative files, directories, encoded names, references, and translations', t => {
  const f = fixture(t, {
    'docs/README.md': '[English](README.en.md)\n[code](../src/a.rs#part)\n[dir](../src/)\n[space](<../src/a b.rs>)\n[id]: ../src/a%20b.rs "source"',
    'docs/README.en.md': '[JP](README.md)\n[other](https://github.com/Hokutaka/stelle)\n[anchor](#local)',
    'docs/topic.ja.md': '[EN](topic.en.md)',
    'docs/topic.en.md': '[JA](topic.ja.md)',
    'src/a.rs': '', 'src/a b.rs': '', 'SECURITY.md': 'Policy',
  });
  assert.deepEqual(check(f.root, f.files).errors, []);
});
test('missing target and case mismatch fail even if an ignored file exists', t => {
  const f = fixture(t, { 'README.md': '[bad](missing.md)\n[case](readme.en.md)', 'README.en.md': '' });
  fs.writeFileSync(path.join(f.root, 'missing.md'), '');
  assert.equal(check(f.root, f.files).errors.length, 2);
});
test('missing Japanese and English partners and unnamed public guides fail', t => {
  const f = fixture(t, { 'docs/a.ja.md': '', 'docs/b.en.md': '', 'examples/README.md': '', 'experiments/note.md': '' });
  assert.equal(check(f.root, f.files).errors.length, 4);
});
test('old Cerune URL is rejected without rejecting other projects', t => {
  const f = fixture(t, { 'README.md': '[old](https://github.com/Hokutaka/Sashimi/issues/81)', 'README.en.md': '' });
  assert.match(check(f.root, f.files).errors[0], /obsolete/);
});
test('code examples are not mistaken for documentation links', () => {
  assert.deepEqual(destinations('```md\n[x](missing)\n```\n~~~\n[y](missing)\n~~~\n`[z](missing)`\n    [indented](missing)\n![real](image.png)'), ['image.png']);
});
