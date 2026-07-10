import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { installRuntimeBundle } from '../dist/download.js';
import { hasPrebuiltRuntimeBundle } from '../dist/platform.js';

test('published runtime matrix matches release workflow assets', () => {
  assert.equal(hasPrebuiltRuntimeBundle('linux', 'x64'), true);
  assert.equal(hasPrebuiltRuntimeBundle('windows', 'x64'), true);
  assert.equal(hasPrebuiltRuntimeBundle('linux', 'arm64'), false);
  assert.equal(hasPrebuiltRuntimeBundle('macos', 'x64'), false);
  assert.equal(hasPrebuiltRuntimeBundle('macos', 'arm64'), false);
});

test('missing release asset returns false so setup can build from source', async (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-orb-download-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));

  const platform = testPlatform(root);

  const installed = await installRuntimeBundle(platform, {
    force: true,
    releaseDir: path.join(root, 'empty-release'),
  });

  assert.equal(installed, false);
  assert.equal(fs.existsSync(platform.runtimeDir), false);
});

test('HTTP 404 falls back but other download failures remain fatal', async (t) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'agent-orb-download-http-test-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));

  let status = 404;
  const server = http.createServer((_request, response) => {
    response.writeHead(status).end();
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  t.after(() => server.close());
  const address = server.address();
  assert.ok(address && typeof address === 'object');
  const releaseBaseUrl = `http://127.0.0.1:${address.port}`;

  const platform = testPlatform(root);
  assert.equal(
    await installRuntimeBundle(platform, { force: true, releaseBaseUrl }),
    false,
  );

  status = 500;
  await assert.rejects(
    installRuntimeBundle(platform, { force: true, releaseBaseUrl }),
    /Download failed \(500/,
  );
});

function testPlatform(root) {
  return {
    platform: 'linux',
    arch: 'x64',
    exeSuffix: '',
    pathDelimiter: ':',
    runtimeDir: path.join(root, 'runtime'),
    configDir: path.join(root, 'config'),
    bundleName: 'agent-orb-linux-x64.tar.gz',
  };
}
