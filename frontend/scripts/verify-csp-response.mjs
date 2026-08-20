import { spawn } from 'node:child_process'
import { once } from 'node:events'
import path from 'node:path'
import process from 'node:process'

const host = '127.0.0.1'
const port = Number(process.env.CSP_TEST_PORT ?? 3412)
const origin = `http://${host}:${port}`
const nextBin = path.join(process.cwd(), 'node_modules', 'next', 'dist', 'bin', 'next')
const output = []

const server = spawn(process.execPath, [nextBin, 'start', '--hostname', host, '--port', String(port)], {
  env: {
    ...process.env,
    NEXT_PUBLIC_CANARY_MODE: 'cloud',
  },
  stdio: ['ignore', 'pipe', 'pipe'],
})

server.stdout.on('data', (chunk) => output.push(chunk.toString()))
server.stderr.on('data', (chunk) => output.push(chunk.toString()))

async function fetchPage() {
  const deadline = Date.now() + 20_000
  let lastError

  while (Date.now() < deadline) {
    if (server.exitCode !== null) {
      throw new Error(`Next.js exited before the CSP check:\n${output.join('')}`)
    }

    try {
      const response = await fetch(`${origin}/sign-in`)

      if (response.ok) {
        return { response, html: await response.text() }
      }

      lastError = new Error(`Next.js returned HTTP ${response.status}`)
    } catch (error) {
      lastError = error
    }

    await new Promise((resolve) => setTimeout(resolve, 250))
  }

  throw new Error(`Timed out waiting for Next.js: ${lastError}\n${output.join('')}`)
}

function verifyNonceHandoff(response, html) {
  const contentSecurityPolicy = response.headers.get('content-security-policy')
  const nonce = contentSecurityPolicy?.match(/script-src[^;]*'nonce-([^']+)'/)?.[1]

  if (!nonce) {
    throw new Error('The production response did not include a script nonce in its CSP header')
  }

  const inlineExecutableTags = [
    ...html.matchAll(/<(script|style)\b[^>]*>/gi),
  ].map((match) => match[0])

  if (inlineExecutableTags.length === 0) {
    throw new Error('The production response did not contain any script or style tags')
  }

  const tagsWithoutMatchingNonce = inlineExecutableTags.filter(
    (tag) => !tag.includes(`nonce="${nonce}"`),
  )

  if (tagsWithoutMatchingNonce.length > 0) {
    throw new Error(`Production tags are missing the CSP nonce:\n${tagsWithoutMatchingNonce.join('\n')}`)
  }

  if (!html.includes('canary-theme')) {
    throw new Error('The production response did not include the first-paint theme initializer')
  }
}

try {
  const { response, html } = await fetchPage()
  verifyNonceHandoff(response, html)
  console.log('Production CSP nonce handoff verified')
} finally {
  if (server.exitCode === null) {
    const exitPromise = once(server, 'exit')
    server.kill('SIGTERM')
    const exited = await Promise.race([
      exitPromise.then(() => true),
      new Promise((resolve) => setTimeout(() => resolve(false), 5_000)),
    ])

    if (!exited) {
      server.kill('SIGKILL')
      await exitPromise
    }
  }
}
